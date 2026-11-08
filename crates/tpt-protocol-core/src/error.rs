use alloc::string::String;
use core::fmt;

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    Parse(String),
    State(String),
    Normalize(String),
    Cbor(String),
    Transport(String),
    Config(String),
    /// A checksum or CRC mismatch. `expected` is the value computed from the
    /// data, and `actual` is the value that arrived. `u32` covers CRC-32.
    Checksum {
        expected: u32,
        actual: u32,
        message: String,
    },
    Overflow {
        max: usize,
        actual: usize,
        message: String,
    },
    InvalidFrame {
        reason: String,
        offset: usize,
    },
}

impl Error {
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }

    pub fn state(msg: impl Into<String>) -> Self {
        Self::State(msg.into())
    }

    pub fn normalize(msg: impl Into<String>) -> Self {
        Self::Normalize(msg.into())
    }

    pub fn cbor(msg: impl Into<String>) -> Self {
        Self::Cbor(msg.into())
    }

    pub fn transport(msg: impl Into<String>) -> Self {
        Self::Transport(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn checksum(expected: u32, actual: u32, msg: impl Into<String>) -> Self {
        Self::Checksum {
            expected,
            actual,
            message: msg.into(),
        }
    }

    pub fn overflow(max: usize, actual: usize, msg: impl Into<String>) -> Self {
        Self::Overflow {
            max,
            actual,
            message: msg.into(),
        }
    }

    pub fn invalid_frame(reason: impl Into<String>, offset: usize) -> Self {
        Self::InvalidFrame {
            reason: reason.into(),
            offset,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::State(msg) => write!(f, "state error: {msg}"),
            Self::Normalize(msg) => write!(f, "normalize error: {msg}"),
            Self::Cbor(msg) => write!(f, "cbor error: {msg}"),
            Self::Transport(msg) => write!(f, "transport error: {msg}"),
            Self::Config(msg) => write!(f, "config error: {msg}"),
            Self::Checksum {
                expected,
                actual,
                message,
            } => {
                write!(
                    f,
                    "checksum mismatch: expected {expected:#x}, got {actual:#x} — {message}"
                )
            }
            Self::Overflow {
                max,
                actual,
                message,
            } => {
                write!(f, "overflow: max {max} bytes, got {actual} — {message}")
            }
            Self::InvalidFrame { reason, offset } => {
                write!(f, "invalid frame at offset {offset}: {reason}")
            }
        }
    }
}

impl From<core::str::Utf8Error> for Error {
    fn from(e: core::str::Utf8Error) -> Self {
        Self::Parse(alloc::format!("UTF-8 error: {e}"))
    }
}

#[cfg(feature = "std")]
impl core::error::Error for Error {}
