//! Modbus RTU/TCP protocol implementation for industrial automation.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameEncoder, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Upper bound on pre-allocated registers from a wire-controlled quantity field.
const MAX_MODBUS_REGISTERS: usize = 1024;

// ---------------------------------------------------------------------------
// Modbus CRC-16 (CRC-16/Modbus)
// ---------------------------------------------------------------------------

fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// Frame types
// ---------------------------------------------------------------------------

/// Modbus PDU – the function code and its payload.
///
/// The variants correspond to the most common Modbus function codes.
/// Unrecognised codes are captured in `Raw`.
#[derive(Clone, Debug, PartialEq)]
pub enum ModbusFrame {
    /// FC 0x01
    ReadCoils { address: u16, quantity: u16 },
    /// FC 0x02
    ReadDiscreteInputs { address: u16, quantity: u16 },
    /// FC 0x03
    ReadHoldingRegisters { address: u16, quantity: u16 },
    /// FC 0x04
    ReadInputRegisters { address: u16, quantity: u16 },
    /// FC 0x05
    WriteSingleCoil { address: u16, value: bool },
    /// FC 0x06
    WriteSingleRegister { address: u16, value: u16 },
    /// FC 0x07
    ReadExceptionStatus { data: u8 },
    /// FC 0x08
    Diagnostics { sub_function: u16, data: Vec<u8> },
    /// FC 0x10
    WriteMultipleRegisters {
        address: u16,
        quantity: u16,
        values: Vec<u16>,
    },
    /// Read response for FC 0x01-0x04. `bytes` is the data after the byte count.
    ReadResponse { function_code: u8, bytes: Vec<u8> },
    /// Response to FC 0x10: echoes the start address and quantity.
    WriteMultipleResponse { address: u16, quantity: u16 },
    /// Exception response. `function_code` is the request's code without bit 7.
    Exception {
        function_code: u8,
        exception_code: u8,
    },
    /// Any unrecognized function code.
    Raw { function_code: u8, data: Vec<u8> },
}

impl fmt::Display for ModbusFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadCoils { address, quantity } => {
                write!(f, "ReadCoils(addr={address}, qty={quantity})")
            }
            Self::ReadDiscreteInputs { address, quantity } => {
                write!(f, "ReadDiscreteInputs(addr={address}, qty={quantity})")
            }
            Self::ReadHoldingRegisters { address, quantity } => {
                write!(f, "ReadHoldingRegisters(addr={address}, qty={quantity})")
            }
            Self::ReadInputRegisters { address, quantity } => {
                write!(f, "ReadInputRegisters(addr={address}, qty={quantity})")
            }
            Self::WriteSingleCoil { address, value } => {
                write!(f, "WriteSingleCoil(addr={address}, val={value})")
            }
            Self::WriteSingleRegister { address, value } => {
                write!(f, "WriteSingleRegister(addr={address}, val={value})")
            }
            Self::ReadExceptionStatus { data } => {
                write!(f, "ReadExceptionStatus(data={data})")
            }
            Self::Diagnostics { sub_function, data } => {
                write!(
                    f,
                    "Diagnostics(sub=0x{sub_function:04X}, {} bytes)",
                    data.len()
                )
            }
            Self::WriteMultipleRegisters {
                address,
                quantity,
                values,
            } => {
                write!(
                    f,
                    "WriteMultipleRegisters(addr={address}, qty={quantity}, {} vals)",
                    values.len()
                )
            }
            Self::ReadResponse {
                function_code,
                bytes,
            } => write!(f, "ReadResponse(fc={function_code}, {} bytes)", bytes.len()),
            Self::WriteMultipleResponse { address, quantity } => {
                write!(f, "WriteMultipleResponse(addr={address}, qty={quantity})")
            }
            Self::Exception {
                function_code,
                exception_code,
            } => write!(
                f,
                "Exception(fc=0x{function_code:02X}, code={exception_code})"
            ),
            Self::Raw {
                function_code,
                data,
            } => {
                write!(f, "Raw(fc=0x{function_code:02X}, {} bytes)", data.len())
            }
        }
    }
}

/// A complete Modbus RTU frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ModbusRtuFrame {
    pub slave_address: u8,
    pub frame: ModbusFrame,
    pub crc: u16,
    /// MBAP transaction id, present only for TCP frames.
    pub transaction_id: Option<u16>,
}

impl fmt::Display for ModbusRtuFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RTU[slave={}, crc=0x{:04X}, {}]",
            self.slave_address, self.crc, self.frame
        )
    }
}

/// A complete Modbus TCP frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ModbusTcpFrame {
    pub transaction_id: u16,
    pub unit_id: u8,
    pub frame: ModbusFrame,
}

impl fmt::Display for ModbusTcpFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TCP[tid={}, uid={}, {}]",
            self.transaction_id, self.unit_id, self.frame
        )
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parses raw bytes into either a Modbus RTU or TCP frame.
///
/// Detection – if bytes [2..4] == 0x0000 *and* the TCP length field
/// (bytes [4..6]) is consistent with the available data, the frame is
/// treated as TCP.  Otherwise it is parsed as RTU.
/// Which side of an exchange the bytes come from. Requests and responses share
/// function codes but have different layouts, and neither TCP nor RTU framing
/// says which one a frame is, so the caller chooses with the parser constructor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Client-to-server frames (the default).
    #[default]
    Request,
    /// Server-to-client frames: read data, write echoes and exceptions.
    Response,
}

/// Parses Modbus RTU or TCP frames. See [`Direction`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ModbusParser {
    direction: Direction,
}

impl ModbusParser {
    /// Parser for request frames.
    pub fn new() -> Self {
        Self {
            direction: Direction::Request,
        }
    }

    /// Parser for response frames.
    pub fn response() -> Self {
        Self {
            direction: Direction::Response,
        }
    }
}

impl FrameParser for ModbusParser {
    type Frame = ModbusRtuFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 4 {
            return Err(Error::Parse(format!(
                "frame too short: {} bytes, need at least 4",
                bytes.len()
            )));
        }

        // A frame whose trailing RTU CRC checks out is RTU. Without this check,
        // an RTU frame with zero bytes at offsets 2-3 would look like TCP.
        let n = bytes.len();
        let rtu_crc_ok =
            crc16_modbus(&bytes[..n - 2]) == u16::from_le_bytes([bytes[n - 2], bytes[n - 1]]);
        // Otherwise TCP if the protocol ID is 0x0000 and the length field fits.
        let looks_like_tcp =
            !rtu_crc_ok && bytes.len() >= 8 && bytes[2] == 0x00 && bytes[3] == 0x00 && {
                let tcp_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
                // The MBAP length counts unit id + PDU, so it must be at least 2.
                tcp_len >= 2 && 6 + tcp_len <= bytes.len()
            };

        if looks_like_tcp {
            let (tid, uid, frame) = parse_tcp_pdu(bytes, self.direction)?;
            let mut raw = Vec::with_capacity(bytes.len() + 4);
            raw.push(uid);
            raw.push(frame_to_func_code(&frame));
            raw.extend_from_slice(&pdu_data(&frame));
            let crc = crc16_modbus(&raw);
            Ok(ModbusRtuFrame {
                slave_address: uid,
                frame,
                crc,
                transaction_id: Some(tid),
            })
        } else {
            let (addr, frame, crc) = parse_rtu_pdu(bytes, self.direction)?;
            Ok(ModbusRtuFrame {
                slave_address: addr,
                frame,
                crc,
                transaction_id: None,
            })
        }
    }

    /// Frames one message from the front of `bytes` (a stream buffer).
    ///
    /// TCP frames carry their length in the MBAP header, so they are framed
    /// exactly. RTU frames have no length field, so the shortest prefix that
    /// parses with a valid CRC is taken. Ambiguity is possible: a prefix can
    /// match by chance (about 1 in 65,536 per candidate length).
    /// `Ok(None)` means more bytes are needed.
    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        if bytes.len() < 4 {
            return Ok(None);
        }
        // RTU first: the shortest prefix with a valid CRC. A valid CRC is strong
        // evidence, so it wins over the weaker TCP header check. The longest RTU
        // ADU is 256 bytes.
        let max = bytes.len().min(256);
        for len in 4..=max {
            if let Ok(frame) = self.parse(&bytes[..len]) {
                return Ok(Some((frame, len)));
            }
        }
        // TCP: the MBAP length gives the exact frame length.
        if let Some(total) = tcp_frame_length(bytes) {
            if bytes.len() < total {
                return Ok(None);
            }
            return Ok(Some((self.parse(&bytes[..total])?, total)));
        }
        if bytes.len() >= 256 {
            return Err(Error::Parse(
                "no Modbus RTU frame in the first 256 bytes".into(),
            ));
        }
        Ok(None)
    }
}

/// The total length of a TCP frame at the start of `bytes`, if it looks like one
/// (protocol id 0, a valid MBAP length, and an RTU CRC that does not match).
fn tcp_frame_length(bytes: &[u8]) -> Option<usize> {
    let n = bytes.len();
    if n < 8 || bytes[2] != 0x00 || bytes[3] != 0x00 {
        return None;
    }
    let rtu_crc_ok =
        n >= 4 && crc16_modbus(&bytes[..n - 2]) == u16::from_le_bytes([bytes[n - 2], bytes[n - 1]]);
    if rtu_crc_ok {
        return None;
    }
    let tcp_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    (tcp_len >= 2).then_some(6 + tcp_len)
}

fn frame_to_func_code(frame: &ModbusFrame) -> u8 {
    match frame {
        ModbusFrame::ReadCoils { .. } => 0x01,
        ModbusFrame::ReadDiscreteInputs { .. } => 0x02,
        ModbusFrame::ReadHoldingRegisters { .. } => 0x03,
        ModbusFrame::ReadInputRegisters { .. } => 0x04,
        ModbusFrame::WriteSingleCoil { .. } => 0x05,
        ModbusFrame::WriteSingleRegister { .. } => 0x06,
        ModbusFrame::ReadExceptionStatus { .. } => 0x07,
        ModbusFrame::Diagnostics { .. } => 0x08,
        ModbusFrame::WriteMultipleRegisters { .. } => 0x10,
        ModbusFrame::ReadResponse { function_code, .. } => *function_code,
        ModbusFrame::WriteMultipleResponse { .. } => 0x10,
        ModbusFrame::Exception { function_code, .. } => function_code | 0x80,
        ModbusFrame::Raw { function_code, .. } => *function_code,
    }
}

fn pdu_data(frame: &ModbusFrame) -> Vec<u8> {
    match frame {
        ModbusFrame::ReadCoils { address, quantity }
        | ModbusFrame::ReadDiscreteInputs { address, quantity }
        | ModbusFrame::ReadHoldingRegisters { address, quantity }
        | ModbusFrame::ReadInputRegisters { address, quantity } => {
            let mut d = Vec::with_capacity(4);
            d.extend_from_slice(&address.to_be_bytes());
            d.extend_from_slice(&quantity.to_be_bytes());
            d
        }
        ModbusFrame::WriteSingleCoil { address, value } => {
            let mut d = Vec::with_capacity(4);
            d.extend_from_slice(&address.to_be_bytes());
            if *value {
                d.extend_from_slice(&[0xFF, 0x00]);
            } else {
                d.extend_from_slice(&[0x00, 0x00]);
            }
            d
        }
        ModbusFrame::WriteSingleRegister { address, value } => {
            let mut d = Vec::with_capacity(4);
            d.extend_from_slice(&address.to_be_bytes());
            d.extend_from_slice(&value.to_be_bytes());
            d
        }
        ModbusFrame::ReadExceptionStatus { data } => alloc::vec![*data],
        ModbusFrame::Diagnostics { sub_function, data } => {
            let mut d = Vec::with_capacity(2 + data.len());
            d.extend_from_slice(&sub_function.to_be_bytes());
            d.extend_from_slice(data);
            d
        }
        ModbusFrame::WriteMultipleRegisters {
            address,
            quantity,
            values,
        } => {
            let mut d = Vec::with_capacity(5 + values.len() * 2);
            d.extend_from_slice(&address.to_be_bytes());
            d.extend_from_slice(&quantity.to_be_bytes());
            d.push((values.len() * 2) as u8);
            for v in values {
                d.extend_from_slice(&v.to_be_bytes());
            }
            d
        }
        ModbusFrame::ReadResponse { bytes, .. } => {
            let mut d = Vec::with_capacity(1 + bytes.len());
            d.push(bytes.len() as u8);
            d.extend_from_slice(bytes);
            d
        }
        ModbusFrame::WriteMultipleResponse { address, quantity } => {
            let mut d = Vec::with_capacity(4);
            d.extend_from_slice(&address.to_be_bytes());
            d.extend_from_slice(&quantity.to_be_bytes());
            d
        }
        ModbusFrame::Exception { exception_code, .. } => alloc::vec![*exception_code],
        ModbusFrame::Raw { data, .. } => data.clone(),
    }
}

fn parse_rtu_pdu(bytes: &[u8], dir: Direction) -> Result<(u8, ModbusFrame, u16), Error> {
    if bytes.len() < 4 {
        return Err(Error::Parse(format!(
            "RTU frame too short: {} bytes",
            bytes.len()
        )));
    }

    let slave_address = bytes[0];
    let func = bytes[1];
    let data = &bytes[2..bytes.len() - 2];

    // Validate CRC.
    let expected_crc = u16::from_le_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]]);
    let crc_input = &bytes[..bytes.len() - 2];
    let computed_crc = crc16_modbus(crc_input);
    if computed_crc != expected_crc {
        return Err(Error::Parse(format!(
            "CRC mismatch: computed 0x{computed_crc:04X}, expected 0x{expected_crc:04X}"
        )));
    }

    let frame = parse_pdu(func, data, dir)?;
    Ok((slave_address, frame, expected_crc))
}

fn parse_tcp_pdu(bytes: &[u8], dir: Direction) -> Result<(u16, u8, ModbusFrame), Error> {
    if bytes.len() < 8 {
        return Err(Error::Parse(format!(
            "TCP frame too short: {} bytes",
            bytes.len()
        )));
    }

    let tid = u16::from_be_bytes([bytes[0], bytes[1]]);
    let protocol_id = u16::from_be_bytes([bytes[2], bytes[3]]);
    if protocol_id != 0x0000 {
        return Err(Error::Parse(format!(
            "invalid Modbus protocol identifier: 0x{protocol_id:04X}"
        )));
    }

    let length = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    if length < 2 {
        return Err(Error::Parse(format!(
            "invalid MBAP length: {length}, need at least 2 (unit id + function code)"
        )));
    }
    if bytes.len() < 6 + length {
        return Err(Error::Parse(format!(
            "TCP frame truncated: have {} bytes, need {}",
            bytes.len(),
            6 + length
        )));
    }

    let unit_id = bytes[6];
    let func = bytes[7];
    let data = &bytes[8..6 + length];

    let frame = parse_pdu(func, data, dir)?;
    Ok((tid, unit_id, frame))
}

fn parse_pdu(func: u8, data: &[u8], dir: Direction) -> Result<ModbusFrame, Error> {
    if func & 0x80 != 0 {
        // Exception response: the request's function code with bit 7 set,
        // followed by exactly one exception code.
        return match data {
            [code] => Ok(ModbusFrame::Exception {
                function_code: func & 0x7F,
                exception_code: *code,
            }),
            _ => Err(Error::Parse(format!(
                "exception response needs exactly 1 byte, got {}",
                data.len()
            ))),
        };
    }
    match func {
        0x01..=0x04 => match dir {
            Direction::Request => {
                let (address, quantity) = fixed_pair(func, data)?;
                let max = if func <= 0x02 { 2000 } else { 125 };
                check_quantity(func, quantity, max)?;
                Ok(match func {
                    0x01 => ModbusFrame::ReadCoils { address, quantity },
                    0x02 => ModbusFrame::ReadDiscreteInputs { address, quantity },
                    0x03 => ModbusFrame::ReadHoldingRegisters { address, quantity },
                    _ => ModbusFrame::ReadInputRegisters { address, quantity },
                })
            }
            Direction::Response => {
                let (&byte_count, bytes) = data.split_first().ok_or_else(|| {
                    Error::Parse(format!("read response for function 0x{func:02X} is empty"))
                })?;
                if byte_count == 0 || bytes.len() != byte_count as usize {
                    return Err(Error::Parse(format!(
                        "read response byte_count {byte_count} does not match {} data bytes",
                        bytes.len()
                    )));
                }
                Ok(ModbusFrame::ReadResponse {
                    function_code: func,
                    bytes: bytes.to_vec(),
                })
            }
        },
        0x05 => {
            let (address, raw) = fixed_pair(func, data)?;
            let value = match raw {
                0xFF00 => true,
                0x0000 => false,
                other => {
                    return Err(Error::Parse(format!(
                        "WriteSingleCoil: invalid value 0x{other:04X}, expected 0xFF00 or 0x0000"
                    )))
                }
            };
            Ok(ModbusFrame::WriteSingleCoil { address, value })
        }
        0x06 => {
            let (address, value) = fixed_pair(func, data)?;
            Ok(ModbusFrame::WriteSingleRegister { address, value })
        }
        0x07 => {
            let d = if data.is_empty() { 0 } else { data[0] };
            Ok(ModbusFrame::ReadExceptionStatus { data: d })
        }
        0x08 => {
            if data.len() < 2 {
                return Err(Error::Parse("Diagnostics: need at least 2 bytes".into()));
            }
            let sub_function = u16::from_be_bytes([data[0], data[1]]);
            let payload = if data.len() > 2 {
                data[2..].to_vec()
            } else {
                Vec::new()
            };
            Ok(ModbusFrame::Diagnostics {
                sub_function,
                data: payload,
            })
        }
        0x10 => match dir {
            Direction::Request => {
                if data.len() < 5 {
                    return Err(Error::Parse(
                        "WriteMultipleRegisters: need at least 5 bytes".into(),
                    ));
                }
                let address = u16::from_be_bytes([data[0], data[1]]);
                let quantity = u16::from_be_bytes([data[2], data[3]]);
                check_quantity(func, quantity, 123)?;
                let byte_count = data[4] as usize;
                if byte_count != 2 * quantity as usize || data.len() != 5 + byte_count {
                    return Err(Error::Parse(format!(
                        "WriteMultipleRegisters: byte_count {byte_count} does not match \
                         quantity {quantity} and {} data bytes",
                        data.len() - 5
                    )));
                }
                let values: Vec<u16> = data[5..]
                    .chunks_exact(2)
                    .take(MAX_MODBUS_REGISTERS)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                Ok(ModbusFrame::WriteMultipleRegisters {
                    address,
                    quantity,
                    values,
                })
            }
            Direction::Response => {
                let (address, quantity) = fixed_pair(func, data)?;
                check_quantity(func, quantity, 123)?;
                Ok(ModbusFrame::WriteMultipleResponse { address, quantity })
            }
        },
        _ => Ok(ModbusFrame::Raw {
            function_code: func,
            data: data.to_vec(),
        }),
    }
}

/// Reads exactly two big-endian words (address and quantity, or address and value).
fn fixed_pair(func: u8, data: &[u8]) -> Result<(u16, u16), Error> {
    match data {
        [a, b, c, d] => Ok((u16::from_be_bytes([*a, *b]), u16::from_be_bytes([*c, *d]))),
        _ => Err(Error::Parse(format!(
            "function 0x{func:02X}: expected 4 data bytes, got {}",
            data.len()
        ))),
    }
}

fn check_quantity(func: u8, quantity: u16, max: u16) -> Result<(), Error> {
    if quantity == 0 || quantity > max {
        return Err(Error::Parse(format!(
            "function 0x{func:02X}: quantity {quantity} is outside 1..={max}"
        )));
    }
    Ok(())
}

/// Encodes RTU frames: slave address, PDU and CRC. Requests and responses
/// share the layout, so one encoder serves both directions.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModbusEncoder;

/// The longest RTU frame (ADU) is 256 bytes.
const MAX_RTU_FRAME: usize = 256;

impl FrameEncoder for ModbusEncoder {
    type Frame = ModbusRtuFrame;

    fn encode(&self, frame: &ModbusRtuFrame) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(MAX_RTU_FRAME);
        out.push(frame.slave_address);
        out.push(frame_to_func_code(&frame.frame));
        out.extend_from_slice(&pdu_data(&frame.frame));
        // Checked here, not in pdu_data: a longer read response would wrap its byte count.
        if out.len() + 2 > MAX_RTU_FRAME {
            return Err(Error::Parse(format!(
                "Modbus RTU frame of {} bytes exceeds the {MAX_RTU_FRAME}-byte limit",
                out.len() + 2
            )));
        }
        let crc = crc16_modbus(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Connection state for a Modbus device.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModbusState {
    pub connected: bool,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
    pub unit_id: u64,
}

/// Validates Modbus connection health.
#[derive(Clone, Debug, Default)]
pub struct ModbusValidator;

impl ModbusValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for ModbusValidator {
    type State = ModbusState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("Modbus device not connected".into()));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "Modbus device has {} consecutive errors",
                state.error_count
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Normalizer
// ---------------------------------------------------------------------------

/// Maps parsed Modbus frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct ModbusNormalizer;

impl ModbusNormalizer {
    pub fn new() -> Self {
        Self
    }
}

/// Turns the data of a read response into one data point for its first
/// coil, bit or register. A response does not carry the start address, so
/// the point id uses the index within the response.
fn normalize_read_response(
    device_id: &str,
    function_code: u8,
    bytes: &[u8],
) -> Result<DataPoint, Error> {
    let (name, value) = match function_code {
        0x01 | 0x02 => {
            let first = bytes
                .first()
                .ok_or_else(|| Error::Normalize("empty read response".into()))?;
            let name = if function_code == 0x01 {
                "coil"
            } else {
                "discrete_input"
            };
            (name, TupValue::Bool(first & 0x01 != 0))
        }
        0x03 | 0x04 => {
            let [hi, lo, ..] = bytes else {
                return Err(Error::Normalize(
                    "register read response has no register".into(),
                ));
            };
            let name = if function_code == 0x03 {
                "holding_register"
            } else {
                "input_register"
            };
            (name, TupValue::Int(u16::from_be_bytes([*hi, *lo]) as i64))
        }
        other => {
            return Err(Error::Normalize(format!(
                "function 0x{other:02X} is not a read response"
            )))
        }
    };
    Ok(DataPoint::new(
        SourceProtocol::Modbus,
        device_id,
        format!("{name}[0]"),
        0,
        value,
        Quality::Good,
    ))
}

impl DataNormalizer for ModbusNormalizer {
    type Frame = ModbusRtuFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("modbus:{}", frame.slave_address);
        let timestamp = 0; // caller should set a real timestamp

        match &frame.frame {
            ModbusFrame::ReadCoils { .. }
            | ModbusFrame::ReadDiscreteInputs { .. }
            | ModbusFrame::ReadHoldingRegisters { .. }
            | ModbusFrame::ReadInputRegisters { .. } => Err(Error::Normalize(
                "read request carries no measured value; normalize the response instead".into(),
            )),
            ModbusFrame::ReadResponse {
                function_code,
                bytes,
            } => normalize_read_response(&device_id, *function_code, bytes),
            ModbusFrame::Exception {
                function_code,
                exception_code,
            } => Err(Error::Normalize(format!(
                "modbus exception 0x{exception_code:02X} for function 0x{function_code:02X}"
            ))),
            ModbusFrame::WriteMultipleResponse { .. } => Err(Error::Normalize(
                "write-multiple response carries no value".into(),
            )),
            ModbusFrame::WriteSingleCoil { address, value } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("coil:{}", address),
                timestamp,
                TupValue::Bool(*value),
                Quality::Good,
            )),
            ModbusFrame::WriteSingleRegister { address, value } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("holding_register:{}", address),
                timestamp,
                TupValue::Int(*value as i64),
                Quality::Good,
            )),
            ModbusFrame::ReadExceptionStatus { data } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                "exception_status",
                timestamp,
                TupValue::Int(*data as i64),
                Quality::Good,
            )),
            ModbusFrame::Diagnostics { sub_function, data } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("diagnostics:0x{sub_function:04X}"),
                timestamp,
                TupValue::Bytes(data.clone()),
                Quality::Good,
            )),
            ModbusFrame::WriteMultipleRegisters {
                address, values, ..
            } => {
                if values.is_empty() {
                    return Err(Error::Normalize(
                        "WriteMultipleRegisters with no register values".into(),
                    ));
                }
                // Return the first register value; full multi-register handling
                // would produce multiple DataPoints via a batch API.
                Ok(DataPoint::new(
                    SourceProtocol::Modbus,
                    &device_id,
                    format!("holding_register:{}", address),
                    timestamp,
                    TupValue::Int(values[0] as i64),
                    Quality::Good,
                ))
            }
            ModbusFrame::Raw {
                function_code,
                data,
            } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("raw:0x{function_code:02X}"),
                timestamp,
                TupValue::Bytes(data.clone()),
                Quality::Uncertain,
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    // ---- Helper ----

    fn build_rtu_frame(slave: u8, func: u8, data: &[u8]) -> Vec<u8> {
        let mut raw = Vec::with_capacity(2 + data.len() + 2);
        raw.push(slave);
        raw.push(func);
        raw.extend_from_slice(data);
        let crc = crc16_modbus(&raw);
        raw.extend_from_slice(&crc.to_le_bytes());
        raw
    }

    fn build_tcp_frame(tid: u16, unit: u8, func: u8, data: &[u8]) -> Vec<u8> {
        let mut raw = Vec::with_capacity(8 + data.len());
        raw.extend_from_slice(&tid.to_be_bytes());
        raw.extend_from_slice(&[0x00, 0x00]); // protocol ID
        let length = 2 + data.len(); // unit_id + func + data
        raw.extend_from_slice(&(length as u16).to_be_bytes());
        raw.push(unit);
        raw.push(func);
        raw.extend_from_slice(data);
        raw
    }

    // ---- CRC unit tests ----

    #[test]
    fn crc_modbus_known_values() {
        // Modbus spec example (PI-MBUS-300 Rev J):
        // query "11 03 00 6B 00 03" → CRC-16/Modbus = 0x8776
        let crc = crc16_modbus(&[0x11, 0x03, 0x00, 0x6B, 0x00, 0x03]);
        assert_eq!(crc, 0x8776);
    }

    // ---- RTU parser ----

    #[test]
    fn valid_rtu_read_holding_registers() {
        // Request: slave=1, fc=3, addr=0x0000, qty=0x000A, CRC
        let bytes = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
        let parser = ModbusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.slave_address, 0x01);
        match &frame.frame {
            ModbusFrame::ReadHoldingRegisters { address, quantity } => {
                assert_eq!(*address, 0x0000);
                assert_eq!(*quantity, 0x000A);
            }
            _ => panic!("expected ReadHoldingRegisters"),
        }
    }

    #[test]
    fn valid_rtu_write_single_coil() {
        // slave=1, fc=5, addr=0x000A, value=0xFF00 (ON), CRC
        let bytes = build_rtu_frame(0x01, 0x05, &[0x00, 0x0A, 0xFF, 0x00]);
        let parser = ModbusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.slave_address, 0x01);
        match &frame.frame {
            ModbusFrame::WriteSingleCoil { address, value } => {
                assert_eq!(*address, 0x000A);
                assert!(*value);
            }
            _ => panic!("expected WriteSingleCoil"),
        }
    }

    #[test]
    fn invalid_crc_returns_error() {
        let mut bytes = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
        let len = bytes.len();
        bytes[len - 1] ^= 0xFF; // corrupt CRC
        let parser = ModbusParser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Parse(_)));
        assert!(err.to_string().contains("CRC"));
    }

    #[test]
    fn truncated_rtu_frame() {
        let parser = ModbusParser::new();
        let result = parser.parse(&[0x01, 0x03]);
        assert!(result.is_err());
    }

    #[test]
    fn empty_input() {
        let parser = ModbusParser::new();
        let result = parser.parse(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = ModbusParser::new();
        let result = parser.parse(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03]);
        // Should fail with CRC error (treated as RTU, CRC invalid)
        assert!(result.is_err());
    }

    #[test]
    fn rtu_diagnostics() {
        // slave=1, fc=8, sub=0x0000, data=[0x12, 0x34], CRC
        let bytes = build_rtu_frame(0x01, 0x08, &[0x00, 0x00, 0x12, 0x34]);
        let parser = ModbusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.slave_address, 0x01);
        match &frame.frame {
            ModbusFrame::Diagnostics { sub_function, data } => {
                assert_eq!(*sub_function, 0x0000);
                assert_eq!(data.as_slice(), &[0x12, 0x34]);
            }
            _ => panic!("expected Diagnostics"),
        }
    }

    // ---- TCP parser ----

    #[test]
    fn valid_tcp_read_holding_registers() {
        // tid=0x0001, unit=0xFF, fc=3, addr=0x0000, qty=0x0005
        let bytes = build_tcp_frame(0x0001, 0xFF, 0x03, &[0x00, 0x00, 0x00, 0x05]);
        let parser = ModbusParser::new();
        let rtu_frame = parser.parse(&bytes).unwrap();
        // The parser wraps TCP into RTU; slave_address = unit_id
        assert_eq!(rtu_frame.slave_address, 0xFF);
        match &rtu_frame.frame {
            ModbusFrame::ReadHoldingRegisters { address, quantity } => {
                assert_eq!(*address, 0x0000);
                assert_eq!(*quantity, 0x0005);
            }
            _ => panic!("expected ReadHoldingRegisters"),
        }
    }

    #[test]
    fn valid_tcp_write_single_register() {
        let bytes = build_tcp_frame(0x1234, 0x01, 0x06, &[0x00, 0x01, 0x0A, 0x0B]);
        let parser = ModbusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.slave_address, 0x01);
        match &frame.frame {
            ModbusFrame::WriteSingleRegister { address, value } => {
                assert_eq!(*address, 0x0001);
                assert_eq!(*value, 0x0A0B);
            }
            _ => panic!("expected WriteSingleRegister"),
        }
    }

    #[test]
    fn truncated_tcp_frame() {
        let parser = ModbusParser::new();
        // Only 6 bytes, not enough for TCP header
        let result = parser.parse(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x04]);
        assert!(result.is_err());
    }

    #[test]
    fn zero_mbap_length_no_panic() {
        // MBAP length 0 leaves no room for unit id + function code; this used
        // to pass the TCP heuristic and then slice `bytes[8..6]` (start > end).
        let parser = ModbusParser::new();
        let bytes = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(parser.parse(&bytes).is_err());
    }

    #[test]
    fn mbap_length_one_no_panic() {
        let parser = ModbusParser::new();
        let bytes = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(parser.parse(&bytes).is_err());
    }

    #[test]
    fn invalid_protocol_id() {
        let mut bytes = build_tcp_frame(0x0001, 0xFF, 0x03, &[0x00, 0x00, 0x00, 0x05]);
        bytes[3] = 0x01; // invalid protocol ID
        let parser = ModbusParser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    // ---- Normalizer ----

    #[test]
    fn normalize_write_single_register() {
        let mut raw = Vec::new();
        raw.push(0x01); // slave
        raw.push(0x06); // func
        raw.extend_from_slice(&[0x00, 0x10, 0x00, 0x64]); // addr=16, val=100
        let crc = crc16_modbus(&raw);
        raw.extend_from_slice(&crc.to_le_bytes());

        let parser = ModbusParser::new();
        let frame = parser.parse(&raw).unwrap();
        let normalizer = ModbusNormalizer::new();
        let dp = normalizer.normalize(&frame).unwrap();

        assert_eq!(dp.source_protocol, SourceProtocol::Modbus);
        assert_eq!(dp.device_id, "modbus:1");
        assert_eq!(dp.point_id, "holding_register:16");
        assert_eq!(dp.value, TupValue::Int(100));
        assert_eq!(dp.quality, Quality::Good);
    }

    #[test]
    fn normalize_write_single_coil() {
        let mut raw = Vec::new();
        raw.push(0x0A); // slave=10
        raw.push(0x05); // func
        raw.extend_from_slice(&[0x00, 0x01, 0xFF, 0x00]); // addr=1, ON
        let crc = crc16_modbus(&raw);
        raw.extend_from_slice(&crc.to_le_bytes());

        let parser = ModbusParser::new();
        let frame = parser.parse(&raw).unwrap();
        let normalizer = ModbusNormalizer::new();
        let dp = normalizer.normalize(&frame).unwrap();

        assert_eq!(dp.source_protocol, SourceProtocol::Modbus);
        assert_eq!(dp.device_id, "modbus:10");
        assert_eq!(dp.point_id, "coil:1");
        assert_eq!(dp.value, TupValue::Bool(true));
        assert_eq!(dp.quality, Quality::Good);
    }

    #[test]
    fn normalize_raw() {
        let mut raw = Vec::new();
        raw.push(0x01);
        raw.push(0x7F); // unrecognized function code
        raw.extend_from_slice(&[0xAA, 0xBB]);
        let crc = crc16_modbus(&raw);
        raw.extend_from_slice(&crc.to_le_bytes());

        let parser = ModbusParser::new();
        let frame = parser.parse(&raw).unwrap();
        let normalizer = ModbusNormalizer::new();
        let dp = normalizer.normalize(&frame).unwrap();

        assert_eq!(dp.source_protocol, SourceProtocol::Modbus);
        assert!(dp.point_id.starts_with("raw:0x7F"));
        assert_eq!(dp.quality, Quality::Uncertain);
    }

    // ---- Roundtrip: parse → normalize → check fields ----

    #[test]
    fn roundtrip_tcp_write_register() {
        let bytes = build_tcp_frame(0x00A5, 0x11, 0x06, &[0x00, 0x20, 0x03, 0xE8]); // addr=32, val=1000
        let parser = ModbusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        let normalizer = ModbusNormalizer::new();
        let dp = normalizer.normalize(&frame).unwrap();

        assert_eq!(dp.source_protocol, SourceProtocol::Modbus);
        assert_eq!(dp.device_id, "modbus:17"); // 0x11
        assert_eq!(dp.point_id, "holding_register:32");
        assert_eq!(dp.value, TupValue::Int(1000));
        assert_eq!(dp.quality, Quality::Good);
    }

    // ---- Validator ----

    #[test]
    fn validator_connected_ok() {
        let validator = ModbusValidator::new();
        let state = ModbusState {
            connected: true,
            last_ok_timestamp: 1000,
            error_count: 0,
            unit_id: 1,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_disconnected() {
        let validator = ModbusValidator::new();
        let state = ModbusState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_too_many_errors() {
        let validator = ModbusValidator::new();
        let state = ModbusState {
            connected: true,
            last_ok_timestamp: 1000,
            error_count: 11,
            unit_id: 1,
        };
        let err = validator.validate(&state).unwrap_err();
        assert!(err.to_string().contains("errors"));
    }

    // ---- try_parse ----

    #[test]
    fn try_parse_empty_returns_none() {
        let parser = ModbusParser::new();
        let result = parser.try_parse(&[]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn try_parse_too_short_rtu_returns_none() {
        let parser = ModbusParser::new();
        let result = parser.try_parse(&[0x01, 0x03, 0x00]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn try_parse_valid_rtu() {
        let bytes = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
        let parser = ModbusParser::new();
        let result = parser.try_parse(&bytes).unwrap();
        assert!(result.is_some());
        let (frame, consumed) = result.unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(frame.slave_address, 0x01);
    }

    #[test]
    fn exception_status_response_round_trips() {
        // Read Exception Status (0x07) carries one status byte; it used to encode as nothing.
        let frame = ModbusRtuFrame {
            slave_address: 0x01,
            frame: ModbusFrame::ReadExceptionStatus { data: 0x5A },
            crc: 0,
            transaction_id: None,
        };
        let bytes = ModbusEncoder.encode(&frame).unwrap();
        assert_eq!(bytes[..3], [0x01, 0x07, 0x5A]);
        assert_eq!(
            ModbusParser::response().parse(&bytes).unwrap().frame,
            frame.frame
        );
    }

    #[test]
    fn requests_round_trip_through_the_encoder() {
        let frames = [
            ModbusFrame::ReadHoldingRegisters {
                address: 0x006B,
                quantity: 3,
            },
            ModbusFrame::WriteSingleCoil {
                address: 0x00AC,
                value: true,
            },
            ModbusFrame::WriteSingleRegister {
                address: 0x0001,
                value: 0x1234,
            },
            ModbusFrame::WriteMultipleRegisters {
                address: 0x0010,
                quantity: 2,
                values: vec![0x000A, 0x0102],
            },
            ModbusFrame::Diagnostics {
                sub_function: 0x0000,
                data: vec![0xA5, 0x37],
            },
        ];
        for frame in frames {
            let wire = ModbusRtuFrame {
                slave_address: 0x11,
                frame: frame.clone(),
                crc: 0,
                transaction_id: None,
            };
            let bytes = ModbusEncoder.encode(&wire).unwrap();
            let parsed = ModbusParser::new().parse(&bytes).unwrap();
            assert_eq!(parsed.frame, frame, "{frame}");
            assert_eq!(parsed.slave_address, 0x11);
        }
    }

    #[test]
    fn responses_round_trip_through_the_encoder() {
        let frames = [
            ModbusFrame::ReadResponse {
                function_code: 0x03,
                bytes: vec![0x00, 0x0A, 0x01, 0x02],
            },
            ModbusFrame::WriteMultipleResponse {
                address: 0x0010,
                quantity: 2,
            },
            ModbusFrame::Exception {
                function_code: 0x03,
                exception_code: 0x02,
            },
        ];
        for frame in frames {
            let wire = ModbusRtuFrame {
                slave_address: 0x11,
                frame: frame.clone(),
                crc: 0,
                transaction_id: None,
            };
            let bytes = ModbusEncoder.encode(&wire).unwrap();
            assert_eq!(
                ModbusParser::response().parse(&bytes).unwrap().frame,
                frame,
                "{frame}"
            );
        }
    }

    #[test]
    fn encoder_rejects_frames_over_the_rtu_limit() {
        let frame = ModbusRtuFrame {
            slave_address: 0x01,
            frame: ModbusFrame::ReadResponse {
                function_code: 0x03,
                bytes: vec![0; 300],
            },
            crc: 0,
            transaction_id: None,
        };
        assert!(ModbusEncoder.encode(&frame).is_err());
    }

    #[test]
    fn try_parse_tcp_partial_frame_needs_more_data() {
        let bytes = build_tcp_frame(0x0001, 0x01, 0x03, &[0x00, 0x00, 0x00, 0x05]);
        let parser = ModbusParser::new();
        assert_eq!(parser.try_parse(&bytes[..bytes.len() - 1]).unwrap(), None);
    }

    #[test]
    fn try_parse_tcp_consumes_exactly_one_frame() {
        let first = build_tcp_frame(0x0001, 0x01, 0x03, &[0x00, 0x00, 0x00, 0x05]);
        let mut stream = first.clone();
        stream.extend_from_slice(&build_tcp_frame(
            0x0002,
            0x01,
            0x03,
            &[0x00, 0x00, 0x00, 0x01],
        ));
        let (_, consumed) = ModbusParser::new().try_parse(&stream).unwrap().unwrap();
        assert_eq!(consumed, first.len());
    }

    #[test]
    fn try_parse_rtu_partial_frame_needs_more_data() {
        let bytes = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
        let parser = ModbusParser::new();
        assert_eq!(parser.try_parse(&bytes[..bytes.len() - 1]).unwrap(), None);
    }

    #[test]
    fn try_parse_rtu_stream_yields_frames_in_order() {
        let first = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
        let second = build_rtu_frame(0x02, 0x03, &[0x00, 0x10, 0x00, 0x02]);
        let mut stream = first.clone();
        stream.extend_from_slice(&second);
        let parser = ModbusParser::new();
        let (frame, consumed) = parser.try_parse(&stream).unwrap().unwrap();
        assert_eq!(frame.slave_address, 0x01);
        assert_eq!(consumed, first.len());
        let (frame, consumed) = parser.try_parse(&stream[consumed..]).unwrap().unwrap();
        assert_eq!(frame.slave_address, 0x02);
        assert_eq!(consumed, second.len());
    }

    #[test]
    fn write_multiple_huge_quantity_no_unbounded_alloc() {
        // func=0x10 WriteMultipleRegisters with quantity=0xFFFF but only 2 bytes
        // of register data present. Must return Err without allocating 65k u16.
        let bytes = build_rtu_frame(0x01, 0x10, &[0x00, 0x00, 0xFF, 0xFF, 0x02, 0x00, 0x00]);
        let parser = ModbusParser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    // ---- Responses, exceptions, strict request checks (Phase 11) ----

    fn parse_response(bytes: &[u8]) -> Result<ModbusRtuFrame, Error> {
        ModbusParser::response().parse(bytes)
    }

    #[test]
    fn response_read_holding_registers_is_not_misread_as_request() {
        // Response: fc=3, byte_count=4, registers 0x000A and 0x0102.
        let bytes = build_rtu_frame(0x01, 0x03, &[0x04, 0x00, 0x0A, 0x01, 0x02]);
        let frame = parse_response(&bytes).unwrap();
        assert_eq!(
            frame.frame,
            ModbusFrame::ReadResponse {
                function_code: 0x03,
                bytes: vec![0x00, 0x0A, 0x01, 0x02],
            }
        );
        // Read as a request, the same five data bytes are malformed.
        assert!(ModbusParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn response_byte_count_must_match_data() {
        let bytes = build_rtu_frame(0x01, 0x03, &[0x05, 0x00, 0x0A, 0x01, 0x02]);
        assert!(parse_response(&bytes).is_err());
    }

    #[test]
    fn exception_response_parses_in_both_directions() {
        let bytes = build_rtu_frame(0x01, 0x83, &[0x02]);
        let expected = ModbusFrame::Exception {
            function_code: 0x03,
            exception_code: 0x02,
        };
        assert_eq!(ModbusParser::new().parse(&bytes).unwrap().frame, expected);
        assert_eq!(parse_response(&bytes).unwrap().frame, expected);
    }

    #[test]
    fn exception_response_needs_exactly_one_code_byte() {
        assert!(ModbusParser::new()
            .parse(&build_rtu_frame(0x01, 0x83, &[]))
            .is_err());
        assert!(ModbusParser::new()
            .parse(&build_rtu_frame(0x01, 0x83, &[0x02, 0x00]))
            .is_err());
    }

    #[test]
    fn write_single_coil_rejects_invalid_value() {
        // Only 0xFF00 (ON) and 0x0000 (OFF) are valid per the spec.
        let bytes = build_rtu_frame(0x01, 0x05, &[0x00, 0x0A, 0x12, 0x34]);
        assert!(ModbusParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn read_request_quantity_is_range_checked() {
        let zero = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x00]);
        let too_many = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 126]);
        assert!(ModbusParser::new().parse(&zero).is_err());
        assert!(ModbusParser::new().parse(&too_many).is_err());
    }

    #[test]
    fn write_multiple_byte_count_must_match_quantity() {
        // quantity=2 needs byte_count=4, but the frame says 3.
        let bytes = build_rtu_frame(0x01, 0x10, &[0x00, 0x00, 0x00, 0x02, 0x03, 0, 1, 0, 2]);
        assert!(ModbusParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn write_multiple_response_parses() {
        let bytes = build_rtu_frame(0x01, 0x10, &[0x00, 0x00, 0x00, 0x02]);
        assert_eq!(
            parse_response(&bytes).unwrap().frame,
            ModbusFrame::WriteMultipleResponse {
                address: 0,
                quantity: 2,
            }
        );
    }

    #[test]
    fn tcp_transaction_id_is_exposed() {
        let bytes = build_tcp_frame(0x1234, 0x01, 0x03, &[0x00, 0x00, 0x00, 0x01]);
        let frame = ModbusParser::new().parse(&bytes).unwrap();
        assert_eq!(frame.transaction_id, Some(0x1234));
        let rtu = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x01]);
        assert_eq!(
            ModbusParser::new().parse(&rtu).unwrap().transaction_id,
            None
        );
    }

    #[test]
    fn normalize_read_request_has_no_measured_value() {
        let bytes = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
        let frame = ModbusParser::new().parse(&bytes).unwrap();
        assert!(ModbusNormalizer::new().normalize(&frame).is_err());
    }

    #[test]
    fn normalize_read_response_holding_register() {
        let bytes = build_rtu_frame(0x01, 0x03, &[0x04, 0x00, 0x0A, 0x01, 0x02]);
        let frame = parse_response(&bytes).unwrap();
        let dp = ModbusNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.value, TupValue::Int(10));
        assert_eq!(dp.point_id, "holding_register[0]");
    }

    #[test]
    fn normalize_read_response_coil_bit() {
        let bytes = build_rtu_frame(0x01, 0x01, &[0x01, 0x01]);
        let frame = parse_response(&bytes).unwrap();
        let dp = ModbusNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.value, TupValue::Bool(true));
        assert_eq!(dp.point_id, "coil[0]");
    }

    #[test]
    fn normalize_exception_is_an_error() {
        let frame = parse_response(&build_rtu_frame(0x01, 0x83, &[0x02])).unwrap();
        assert!(ModbusNormalizer::new().normalize(&frame).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = ModbusParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }

    /// A TCP frame with a plausible MBAP header. The length field is skewed by
    /// -2..=2 so the length checks are exercised, not just the happy path.
    fn mbap_frame() -> impl Strategy<Value = Vec<u8>> {
        (
            any::<u16>(),
            any::<u8>(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..64),
            -2i32..=2,
        )
            .prop_map(|(tid, unit, func, data, skew)| {
                let mut pdu = alloc::vec![unit, func];
                pdu.extend_from_slice(&data);
                let len = (pdu.len() as i32 + skew).clamp(0, u16::MAX as i32) as u16;
                let mut out = Vec::new();
                out.extend_from_slice(&tid.to_be_bytes());
                out.extend_from_slice(&[0x00, 0x00]);
                out.extend_from_slice(&len.to_be_bytes());
                out.extend_from_slice(&pdu);
                out
            })
    }

    /// Applies one random byte overwrite or one truncation to a valid-looking frame.
    fn mutated_mbap() -> impl Strategy<Value = Vec<u8>> {
        (mbap_frame(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
            |(mut bytes, idx, val, truncate)| {
                if bytes.is_empty() {
                    return bytes;
                }
                if truncate {
                    bytes.truncate(idx % bytes.len());
                } else {
                    let i = idx % bytes.len();
                    bytes[i] = val;
                }
                bytes
            },
        )
    }

    proptest! {
        #[test]
        fn structured_mbap_never_panics(bytes in mutated_mbap()) {
            let parser = ModbusParser::new();
            let _ = parser.parse(&bytes);
            let _ = parser.try_parse(&bytes);
        }
    }
}
