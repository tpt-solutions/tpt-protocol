//! SocketCAN. Needs Linux and the `socketcan` feature.
//!
//! A CAN frame carries no length prefix, so each frame is one message. The
//! identifier is prepended in the form the protocol's parser expects, which is
//! what [`can_message`] builds.

use anyhow::{bail, Result};
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

/// Wraps one CAN frame in the framing that a CAN protocol's parser expects.
/// Returns `None` for protocols that are not carried on CAN.
#[cfg_attr(not(all(feature = "socketcan", target_os = "linux")), allow(dead_code))]
pub fn can_message(protocol: &str, raw_id: u32, data: &[u8]) -> Option<Vec<u8>> {
    let header = match protocol {
        // CANopen: the 11-bit COB-ID, big-endian.
        "canopen" => (raw_id as u16 & 0x07FF).to_be_bytes().to_vec(),
        // J1939 and ISOBUS: the 29-bit identifier, big-endian.
        "j1939" | "isobus" => raw_id.to_be_bytes().to_vec(),
        _ => return None,
    };
    let mut out = header;
    out.extend_from_slice(data);
    Some(out)
}

#[cfg(all(feature = "socketcan", target_os = "linux"))]
pub async fn run_can(
    interface: String,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    if can_message(&protocol, 0, &[]).is_none() {
        bail!("{protocol} is not carried on CAN");
    }
    let bus = imp::CanBus {
        interface,
        protocol: protocol.clone(),
    };
    crate::transport::supervise(bus, protocol, tx).await
}

/// Without SocketCAN support the endpoint fails with an explanation.
#[cfg(not(all(feature = "socketcan", target_os = "linux")))]
pub async fn run_can(
    _interface: String,
    _protocol: String,
    _tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    bail!("SocketCAN needs Linux and the daemon built with the `socketcan` feature")
}

#[cfg(all(feature = "socketcan", target_os = "linux"))]
mod imp {
    use anyhow::{Context, Result};
    use socketcan::tokio::CanSocket;
    use socketcan::{EmbeddedFrame, Id};

    use super::can_message;
    use crate::transport::{ByteLink, Framing, Transport};

    /// Reopens the interface whenever a read fails, so a bus that comes back is picked up.
    pub struct CanBus {
        pub interface: String,
        pub protocol: String,
    }

    impl Transport for CanBus {
        type Link = CanLink;
        const CONCURRENT: bool = false;

        fn describe(&self) -> String {
            format!("CAN interface {}", self.interface)
        }

        async fn next_link(&mut self) -> Result<Option<Self::Link>> {
            let socket = CanSocket::open(&self.interface)
                .with_context(|| format!("open CAN interface {} failed", self.interface))?;
            Ok(Some(CanLink {
                socket,
                protocol: self.protocol.clone(),
            }))
        }
    }

    pub struct CanLink {
        socket: CanSocket,
        protocol: String,
    }

    impl ByteLink for CanLink {
        fn framing(&self) -> Framing {
            Framing::Datagram
        }

        async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
            loop {
                let frame = self.socket.read_frame().await?;
                let raw_id = match frame.id() {
                    Id::Standard(id) => id.as_raw() as u32,
                    Id::Extended(id) => id.as_raw(),
                };
                // Frames for other protocols on the same bus are skipped.
                if let Some(message) = can_message(&self.protocol, raw_id, frame.data()) {
                    return Ok(Some(message));
                }
            }
        }
    }
}
