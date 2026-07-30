use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum TupValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
}

impl fmt::Display for TupValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Text(v) => write!(f, "{v}"),
            Self::Bytes(v) => write!(f, "{} bytes", v.len()),
        }
    }
}

impl From<i64> for TupValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for TupValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<bool> for TupValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<String> for TupValue {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

impl From<&str> for TupValue {
    fn from(v: &str) -> Self {
        Self::Text(v.into())
    }
}

impl From<Vec<u8>> for TupValue {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}

impl From<&[u8]> for TupValue {
    fn from(v: &[u8]) -> Self {
        Self::Bytes(v.to_vec())
    }
}
