//! Modbus TCP polling: the daemon asks a device for registers on a timer.
//!
//! A Modbus server sends nothing on its own, so the passive transports never see
//! data from one. This client connects to the server, sends a read request every
//! period, and turns each response into TUP points. Responses are matched to
//! requests by transaction id; a response that arrives late is dropped.

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::time::{interval, timeout, MissedTickBehavior};
use tpt_protocol_core::{FrameEncoder, FrameParser};
use tpt_protocol_modbus::{ModbusFrame, ModbusParser, ModbusRtuFrame, ModbusTcpEncoder};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

use super::{MAX_BUFFER, RECONNECT_DELAY};
use crate::plugins::now_ms;

/// How long to wait for a response before the request is counted as lost.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

/// What one poll reads from the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollSpec {
    pub unit: u8,
    /// 3 (holding registers) or 4 (input registers).
    pub function: u8,
    pub address: u16,
    pub quantity: u16,
}

impl PollSpec {
    /// Checks the spec against the Modbus limits: function 3 or 4, and 1 to 125
    /// registers per read.
    pub fn new(unit: u8, function: u8, address: u16, quantity: u16) -> Result<Self> {
        if !matches!(function, 3 | 4) {
            bail!("--poll-function must be 3 (holding registers) or 4 (input registers)");
        }
        if !(1..=125).contains(&quantity) {
            bail!("--poll-quantity must be between 1 and 125");
        }
        Ok(Self {
            unit,
            function,
            address,
            quantity,
        })
    }

    fn request(&self, transaction_id: u16) -> ModbusRtuFrame {
        let frame = if self.function == 3 {
            ModbusFrame::ReadHoldingRegisters {
                address: self.address,
                quantity: self.quantity,
            }
        } else {
            ModbusFrame::ReadInputRegisters {
                address: self.address,
                quantity: self.quantity,
            }
        };
        ModbusRtuFrame {
            slave_address: self.unit,
            frame,
            crc: 0,
            transaction_id: Some(transaction_id),
        }
    }
}

/// Polls `addr` for the life of the daemon, reconnecting after a failure.
pub async fn run_modbus_poll(
    addr: String,
    spec: PollSpec,
    period: Duration,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    loop {
        if let Err(e) = poll_session(&addr, spec, period, &tx).await {
            tracing::warn!("modbus poll: {e:#}; reconnecting");
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn poll_session(
    addr: &str,
    spec: PollSpec,
    period: Duration,
    tx: &broadcast::Sender<DataPoint>,
) -> Result<()> {
    let mut stream = TcpStream::connect(addr)
        .await
        .map_err(|e| anyhow!("cannot connect to {addr}: {e}"))?;
    tracing::info!("modbus poll: connected to {addr} (unit {})", spec.unit);

    let mut ticker = interval(period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut buf = Vec::new();
    let mut transaction_id: u16 = 0;
    loop {
        ticker.tick().await;
        transaction_id = transaction_id.wrapping_add(1);
        let request = ModbusTcpEncoder
            .encode(&spec.request(transaction_id))
            .map_err(|e| anyhow!("cannot encode request: {e:?}"))?;
        stream.write_all(&request).await?;

        match read_response(&mut stream, &mut buf, transaction_id).await {
            Ok(response) => match register_points(spec, &response, now_ms()) {
                Ok(points) => {
                    for dp in points {
                        let _ = tx.send(dp);
                    }
                }
                Err(e) => tracing::warn!("modbus poll: {e}"),
            },
            Err(e) => tracing::warn!("modbus poll: {e:#}"),
        }
    }
}

/// Reads until the response with `transaction_id` arrives, or the device goes
/// quiet. Other frames read in the meantime are discarded.
async fn read_response(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    transaction_id: u16,
) -> Result<ModbusRtuFrame> {
    let parser = ModbusParser::response();
    timeout(RESPONSE_TIMEOUT, async {
        loop {
            match parser.try_parse(buf) {
                Ok(Some((frame, used))) => {
                    buf.drain(..used);
                    if frame.transaction_id == Some(transaction_id) {
                        return Ok(frame);
                    }
                    tracing::debug!("modbus poll: dropped response {:?}", frame.transaction_id);
                    continue;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("modbus poll: unparseable bytes ({e:?}); resyncing");
                    buf.clear();
                }
            }
            let mut chunk = [0u8; 512];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                bail!("device closed the connection");
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_BUFFER {
                buf.clear();
            }
        }
    })
    .await
    .map_err(|_| anyhow!("no response within {RESPONSE_TIMEOUT:?}"))?
}

/// One data point per register in a read response, named by its absolute
/// address (`holding_register[100]`), so a poll of registers 100-109 yields ten
/// points. The normalizer alone would give only the first register, because a
/// response does not carry the start address.
pub(crate) fn register_points(
    spec: PollSpec,
    response: &ModbusRtuFrame,
    timestamp: u64,
) -> Result<Vec<DataPoint>, String> {
    match &response.frame {
        ModbusFrame::ReadResponse {
            function_code,
            bytes,
        } => {
            let expected = 2 * spec.quantity as usize;
            if bytes.len() != expected {
                return Err(format!(
                    "expected {expected} register bytes, got {}",
                    bytes.len()
                ));
            }
            let kind = if *function_code == 0x03 {
                "holding_register"
            } else {
                "input_register"
            };
            let device = format!("modbus:{}", response.slave_address);
            Ok(bytes
                .chunks_exact(2)
                .enumerate()
                .map(|(i, word)| {
                    let value = u16::from_be_bytes([word[0], word[1]]);
                    DataPoint::new(
                        SourceProtocol::Modbus,
                        device.as_str(),
                        format!("{kind}[{}]", spec.address as usize + i),
                        timestamp,
                        TupValue::Int(i64::from(value)),
                        Quality::Good,
                    )
                })
                .collect())
        }
        ModbusFrame::Exception {
            function_code,
            exception_code,
        } => Err(format!(
            "device returned exception {exception_code:#04x} for function {function_code:#04x}"
        )),
        other => Err(format!("unexpected response: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(function_code: u8, bytes: Vec<u8>, unit: u8) -> ModbusRtuFrame {
        ModbusRtuFrame {
            slave_address: unit,
            frame: ModbusFrame::ReadResponse {
                function_code,
                bytes,
            },
            crc: 0,
            transaction_id: Some(1),
        }
    }

    #[test]
    fn spec_checks_function_and_quantity() {
        assert!(PollSpec::new(1, 3, 0, 10).is_ok());
        assert!(PollSpec::new(1, 4, 0, 125).is_ok());
        assert!(PollSpec::new(1, 1, 0, 10).is_err(), "coils are not polled");
        assert!(PollSpec::new(1, 3, 0, 0).is_err());
        assert!(PollSpec::new(1, 3, 0, 126).is_err());
    }

    #[test]
    fn every_register_becomes_a_point_at_its_absolute_address() {
        let spec = PollSpec::new(1, 3, 100, 3).unwrap();
        let points = register_points(
            spec,
            &response(0x03, vec![0x00, 0x0A, 0x01, 0x00, 0xFF, 0xFF], 1),
            5,
        )
        .unwrap();
        let names: Vec<_> = points.iter().map(|p| p.point_id.as_str()).collect();
        assert_eq!(
            names,
            [
                "holding_register[100]",
                "holding_register[101]",
                "holding_register[102]"
            ]
        );
        assert_eq!(points[0].value, TupValue::Int(10));
        assert_eq!(points[1].value, TupValue::Int(256));
        assert_eq!(points[2].value, TupValue::Int(65535));
        assert_eq!(points[0].device_id, "modbus:1");
        assert_eq!(points[0].timestamp, 5);
    }

    #[test]
    fn input_registers_use_their_own_name() {
        let spec = PollSpec::new(2, 4, 0, 1).unwrap();
        let points = register_points(spec, &response(0x04, vec![0x00, 0x07], 2), 0).unwrap();
        assert_eq!(points[0].point_id, "input_register[0]");
        assert_eq!(points[0].device_id, "modbus:2");
    }

    #[test]
    fn a_short_or_long_response_is_rejected_not_truncated() {
        let spec = PollSpec::new(1, 3, 0, 2).unwrap();
        assert!(register_points(spec, &response(0x03, vec![0x00, 0x01], 1), 0).is_err());
        assert!(register_points(spec, &response(0x03, vec![0; 6], 1), 0).is_err());
    }

    #[test]
    fn an_exception_response_is_an_error() {
        let spec = PollSpec::new(1, 3, 0, 2).unwrap();
        let frame = ModbusRtuFrame {
            slave_address: 1,
            frame: ModbusFrame::Exception {
                function_code: 0x03,
                exception_code: 0x02,
            },
            crc: 0,
            transaction_id: Some(1),
        };
        let err = register_points(spec, &frame, 0).unwrap_err();
        assert!(err.contains("exception 0x02"), "{err}");
    }
}
