//! Modbus RTU/TCP protocol implementation for industrial automation.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
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
#[derive(Clone, Debug, Default)]
pub struct ModbusParser;

impl ModbusParser {
    pub fn new() -> Self {
        Self
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

        // TCP if protocol ID == 0x0000 AND the length field fits.
        let looks_like_tcp = bytes.len() >= 8 && bytes[2] == 0x00 && bytes[3] == 0x00 && {
            let tcp_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
            6 + tcp_len <= bytes.len()
        };

        if looks_like_tcp {
            let (_tid, uid, frame) = parse_tcp_pdu(bytes)?;
            let mut raw = Vec::with_capacity(bytes.len() + 4);
            raw.push(uid);
            raw.push(frame_to_func_code(&frame));
            raw.extend_from_slice(&pdu_data(&frame));
            let crc = crc16_modbus(&raw);
            Ok(ModbusRtuFrame {
                slave_address: uid,
                frame,
                crc,
            })
        } else {
            let (addr, frame, crc) = parse_rtu_pdu(bytes)?;
            Ok(ModbusRtuFrame {
                slave_address: addr,
                frame,
                crc,
            })
        }
    }

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        if bytes.len() < 4 {
            return Ok(None);
        }
        self.parse(bytes).map(|f| Some((f, bytes.len())))
    }
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
        ModbusFrame::ReadExceptionStatus { .. } => Vec::new(),
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
        ModbusFrame::Raw { data, .. } => data.clone(),
    }
}

fn parse_rtu_pdu(bytes: &[u8]) -> Result<(u8, ModbusFrame, u16), Error> {
    if bytes.len() < 4 {
        return Err(Error::Parse(format!(
            "RTU frame too short: {} bytes",
            bytes.len()
        )));
    }

    let slave_address = bytes[0];
    let func = bytes[1];
    let data_len = bytes.len() - 4; // minus addr, func, CRC(2)
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

    let frame = parse_pdu(func, data, data_len)?;
    Ok((slave_address, frame, expected_crc))
}

fn parse_tcp_pdu(bytes: &[u8]) -> Result<(u16, u8, ModbusFrame), Error> {
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

    let frame = parse_pdu(func, data, data.len())?;
    Ok((tid, unit_id, frame))
}

fn parse_pdu(func: u8, data: &[u8], _data_len: usize) -> Result<ModbusFrame, Error> {
    match func {
        0x01 => {
            if data.len() < 4 {
                return Err(Error::Parse(
                    "ReadCoils: need 4 bytes (addr+quantity)".into(),
                ));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let quantity = u16::from_be_bytes([data[2], data[3]]);
            Ok(ModbusFrame::ReadCoils { address, quantity })
        }
        0x02 => {
            if data.len() < 4 {
                return Err(Error::Parse("ReadDiscreteInputs: need 4 bytes".into()));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let quantity = u16::from_be_bytes([data[2], data[3]]);
            Ok(ModbusFrame::ReadDiscreteInputs { address, quantity })
        }
        0x03 => {
            if data.len() < 4 {
                return Err(Error::Parse("ReadHoldingRegisters: need 4 bytes".into()));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let quantity = u16::from_be_bytes([data[2], data[3]]);
            Ok(ModbusFrame::ReadHoldingRegisters { address, quantity })
        }
        0x04 => {
            if data.len() < 4 {
                return Err(Error::Parse("ReadInputRegisters: need 4 bytes".into()));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let quantity = u16::from_be_bytes([data[2], data[3]]);
            Ok(ModbusFrame::ReadInputRegisters { address, quantity })
        }
        0x05 => {
            if data.len() < 4 {
                return Err(Error::Parse("WriteSingleCoil: need 4 bytes".into()));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let raw = u16::from_be_bytes([data[2], data[3]]);
            let value = raw == 0xFF00;
            Ok(ModbusFrame::WriteSingleCoil { address, value })
        }
        0x06 => {
            if data.len() < 4 {
                return Err(Error::Parse("WriteSingleRegister: need 4 bytes".into()));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let value = u16::from_be_bytes([data[2], data[3]]);
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
        0x10 => {
            if data.len() < 5 {
                return Err(Error::Parse(
                    "WriteMultipleRegisters: need at least 5 bytes".into(),
                ));
            }
            let address = u16::from_be_bytes([data[0], data[1]]);
            let quantity = u16::from_be_bytes([data[2], data[3]]);
            let _byte_count = data[4] as usize;
            let mut values = Vec::with_capacity((quantity as usize).min(MAX_MODBUS_REGISTERS));
            let mut offset = 5;
            for _ in 0..quantity as usize {
                if offset + 2 > data.len() {
                    return Err(Error::Parse(
                        "WriteMultipleRegisters: truncated register data".into(),
                    ));
                }
                let v = u16::from_be_bytes([data[offset], data[offset + 1]]);
                values.push(v);
                offset += 2;
            }
            Ok(ModbusFrame::WriteMultipleRegisters {
                address,
                quantity,
                values,
            })
        }
        _ => Ok(ModbusFrame::Raw {
            function_code: func,
            data: data.to_vec(),
        }),
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

impl DataNormalizer for ModbusNormalizer {
    type Frame = ModbusRtuFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("modbus:{}", frame.slave_address);
        let timestamp = 0; // caller should set a real timestamp

        match &frame.frame {
            ModbusFrame::ReadCoils { address, quantity } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("coil:{}", address),
                timestamp,
                TupValue::Int(*quantity as i64),
                Quality::Good,
            )),
            ModbusFrame::ReadDiscreteInputs { address, quantity } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("discrete_input:{}", address),
                timestamp,
                TupValue::Int(*quantity as i64),
                Quality::Good,
            )),
            ModbusFrame::ReadHoldingRegisters { address, quantity } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("holding_register:{}", address),
                timestamp,
                TupValue::Int(*quantity as i64),
                Quality::Good,
            )),
            ModbusFrame::ReadInputRegisters { address, quantity } => Ok(DataPoint::new(
                SourceProtocol::Modbus,
                &device_id,
                format!("input_register:{}", address),
                timestamp,
                TupValue::Int(*quantity as i64),
                Quality::Good,
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
    fn write_multiple_huge_quantity_no_unbounded_alloc() {
        // func=0x10 WriteMultipleRegisters with quantity=0xFFFF but only 2 bytes
        // of register data present. Must return Err without allocating 65k u16.
        let bytes = build_rtu_frame(0x01, 0x10, &[0x00, 0x00, 0xFF, 0xFF, 0x02, 0x00, 0x00]);
        let parser = ModbusParser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
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
}
