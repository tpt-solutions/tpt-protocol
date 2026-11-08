use core::fmt;
use core::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SourceProtocol {
    Modbus,
    J1939,
    Isobus,
    Uds,
    Canopen,
    DoipSomeip,
    Nmea,
    Mavlink,
    Bacnet,
    Opcua,
    Dnp3,
    Ethercat,
    Sparkplugb,
    Ocpp,
    Ieee11073Sdc,
    Hl7v2,
    Knx,
    MatterThread,
    Coap,
}

impl SourceProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Modbus => "modbus",
            Self::J1939 => "j1939",
            Self::Isobus => "isobus",
            Self::Uds => "uds",
            Self::Canopen => "canopen",
            Self::DoipSomeip => "doip-someip",
            Self::Nmea => "nmea",
            Self::Mavlink => "mavlink",
            Self::Bacnet => "bacnet",
            Self::Opcua => "opcua",
            Self::Dnp3 => "dnp3",
            Self::Ethercat => "ethercat",
            Self::Sparkplugb => "sparkplugb",
            Self::Ocpp => "ocpp",
            Self::Ieee11073Sdc => "ieee11073-sdc",
            Self::Hl7v2 => "hl7v2",
            Self::Knx => "knx",
            Self::MatterThread => "matter-thread",
            Self::Coap => "coap",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        Self::from_str_inner(s)
    }

    fn from_str_inner(s: &str) -> Option<Self> {
        match s {
            "modbus" => Some(Self::Modbus),
            "j1939" => Some(Self::J1939),
            "isobus" => Some(Self::Isobus),
            "uds" => Some(Self::Uds),
            "canopen" => Some(Self::Canopen),
            "doip-someip" => Some(Self::DoipSomeip),
            "nmea" => Some(Self::Nmea),
            "mavlink" => Some(Self::Mavlink),
            "bacnet" => Some(Self::Bacnet),
            "opcua" => Some(Self::Opcua),
            "dnp3" => Some(Self::Dnp3),
            "ethercat" => Some(Self::Ethercat),
            "sparkplugb" => Some(Self::Sparkplugb),
            "ocpp" => Some(Self::Ocpp),
            "ieee11073-sdc" => Some(Self::Ieee11073Sdc),
            "hl7v2" => Some(Self::Hl7v2),
            "knx" => Some(Self::Knx),
            "matter-thread" => Some(Self::MatterThread),
            "coap" => Some(Self::Coap),
            _ => None,
        }
    }
}

impl FromStr for SourceProtocol {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_inner(s).ok_or(())
    }
}

impl fmt::Display for SourceProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
