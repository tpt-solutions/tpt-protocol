//! Real transports: TCP (client or listener) and UDP.
//!
//! Bytes are framed with the protocol's `try_parse` and normalized into TUP
//! data points. A stream that fails to parse drops its buffer and resyncs. Frames
//! that parse but carry no data (such as read requests) are skipped and counted.
//!
//! Serial and CAN backends are deferred: they need platform-specific code and
//! hardware to test, and neither is available here.

use std::time::Duration;

use anyhow::{bail, Result};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

use crate::plugins;

/// How long a TCP client waits before reconnecting.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
/// Largest buffered-but-unframed byte count before the stream resyncs.
const MAX_BUFFER: usize = 64 * 1024;

/// Where the daemon gets its bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum Endpoint {
    /// Connect to a device, and reconnect when the link drops.
    TcpConnect(String),
    /// Accept connections from devices.
    TcpListen(String),
    /// Receive datagrams. Each datagram holds whole frames.
    UdpBind(String),
}

impl Endpoint {
    /// Picks the endpoint from the command-line flags. At most one may be set.
    pub fn from_flags(
        tcp_connect: Option<String>,
        tcp_listen: Option<String>,
        udp_bind: Option<String>,
    ) -> Result<Option<Self>> {
        let chosen: Vec<Endpoint> = [
            tcp_connect.map(Endpoint::TcpConnect),
            tcp_listen.map(Endpoint::TcpListen),
            udp_bind.map(Endpoint::UdpBind),
        ]
        .into_iter()
        .flatten()
        .collect();
        match chosen.len() {
            0 => Ok(None),
            1 => Ok(chosen.into_iter().next()),
            _ => bail!("choose only one of --tcp-connect, --tcp-listen and --udp-bind"),
        }
    }
}

/// Decodes every complete frame at the front of `buf`, removing the bytes
/// used. Points are appended to `out`, so the points decoded before an error
/// are kept. Returns the number of frames that parsed but carried no data.
pub fn drain(protocol: &str, buf: &mut Vec<u8>, out: &mut Vec<DataPoint>) -> Result<usize, String> {
    let mut skipped = 0;
    loop {
        match plugins::decode_one(protocol, buf)? {
            None => return Ok(skipped),
            Some((points, used)) => {
                match points {
                    Ok(points) => out.extend(points),
                    Err(_) => skipped += 1,
                }
                buf.drain(..used);
            }
        }
    }
}

/// Reads from `reader` until it closes, sending each decoded point on `tx`.
async fn pump<R: AsyncRead + Unpin>(
    mut reader: R,
    protocol: &str,
    tx: &broadcast::Sender<DataPoint>,
) -> Result<()> {
    let mut buf = Vec::with_capacity(4096);
    loop {
        if reader.read_buf(&mut buf).await? == 0 {
            return Ok(());
        }
        let mut points = Vec::new();
        let result = drain(protocol, &mut buf, &mut points);
        for dp in points {
            let _ = tx.send(dp);
        }
        if let Err(e) = result {
            eprintln!("{protocol}: {e}; discarding {} buffered bytes", buf.len());
            buf.clear();
        }
        if buf.len() > MAX_BUFFER {
            eprintln!(
                "{protocol}: {} bytes without a complete frame; resyncing",
                buf.len()
            );
            buf.clear();
        }
    }
}

/// Connects to `addr` and reads until the link drops, then reconnects forever.
pub async fn run_tcp_connect(
    addr: String,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    loop {
        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                eprintln!("{protocol}: connected to {addr}");
                if let Err(e) = pump(stream, &protocol, &tx).await {
                    eprintln!("{protocol}: read from {addr} failed: {e}");
                }
                eprintln!("{protocol}: link to {addr} closed");
            }
            Err(e) => eprintln!("{protocol}: connect to {addr} failed: {e}"),
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// Accepts connections on `listener`, reading each one in its own task.
pub async fn run_tcp_listen(
    listener: TcpListener,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        eprintln!("{protocol}: accepted {peer}");
        let protocol = protocol.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = pump(stream, &protocol, &tx).await {
                eprintln!("{protocol}: {peer}: {e}");
            }
            eprintln!("{protocol}: {peer} disconnected");
        });
    }
}

/// Receives datagrams on `socket`. Each datagram is decoded on its own, so a
/// frame split across datagrams is an error.
pub async fn run_udp(
    socket: UdpSocket,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    let mut datagram = vec![0u8; 65_535];
    loop {
        let (n, peer) = socket.recv_from(&mut datagram).await?;
        let mut buf = datagram[..n].to_vec();
        let mut points = Vec::new();
        match drain(&protocol, &mut buf, &mut points) {
            Ok(_) => {}
            Err(e) => eprintln!("{protocol}: {peer}: {e}"),
        }
        if !buf.is_empty() {
            eprintln!(
                "{protocol}: {peer}: {} trailing bytes are not a complete frame",
                buf.len()
            );
        }
        for dp in points {
            let _ = tx.send(dp);
        }
    }
}

/// Binds the endpoint and runs it until it fails.
pub async fn run(
    endpoint: Endpoint,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    match endpoint {
        Endpoint::TcpConnect(addr) => run_tcp_connect(addr, protocol, tx).await,
        Endpoint::TcpListen(addr) => {
            run_tcp_listen(TcpListener::bind(&addr).await?, protocol, tx).await
        }
        Endpoint::UdpBind(addr) => run_udp(UdpSocket::bind(&addr).await?, protocol, tx).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tpt_protocol_tup::TupValue;

    /// Modbus TCP: Write Single Register (unit 1, address 1, value 10).
    const WRITE_REGISTER: [u8; 12] = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x06, 0x00, 0x01, 0x00, 0x0A,
    ];
    /// Modbus TCP: Write Single Register (unit 1, address 2, value 20).
    const WRITE_REGISTER_2: [u8; 12] = [
        0x00, 0x02, 0x00, 0x00, 0x00, 0x06, 0x01, 0x06, 0x00, 0x02, 0x00, 0x14,
    ];

    #[test]
    fn drain_decodes_back_to_back_frames() {
        let mut buf = [WRITE_REGISTER, WRITE_REGISTER_2].concat();
        let mut out = Vec::new();
        assert_eq!(drain("modbus", &mut buf, &mut out), Ok(0));
        assert!(buf.is_empty());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].value, TupValue::Int(10));
        assert_eq!(out[1].value, TupValue::Int(20));
    }

    #[test]
    fn drain_waits_for_a_split_frame() {
        let mut buf = WRITE_REGISTER[..7].to_vec();
        let mut out = Vec::new();
        assert_eq!(drain("modbus", &mut buf, &mut out), Ok(0));
        assert!(out.is_empty());
        buf.extend_from_slice(&WRITE_REGISTER[7..]);
        assert_eq!(drain("modbus", &mut buf, &mut out), Ok(0));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn drain_skips_frames_without_data_and_counts_them() {
        // A read request has no measured value, so it is skipped.
        let read = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x0A,
        ];
        let mut buf = [read.to_vec(), WRITE_REGISTER.to_vec()].concat();
        let mut out = Vec::new();
        assert_eq!(drain("modbus", &mut buf, &mut out), Ok(1));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn drain_keeps_points_before_a_parse_error() {
        let mut buf = [WRITE_REGISTER.to_vec(), vec![0xAA; 300]].concat();
        let mut out = Vec::new();
        assert!(drain("modbus", &mut buf, &mut out).is_err());
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn endpoint_rejects_more_than_one_choice() {
        assert!(matches!(Endpoint::from_flags(None, None, None), Ok(None)));
        assert!(Endpoint::from_flags(Some("a".into()), Some("b".into()), None).is_err());
        assert!(matches!(
            Endpoint::from_flags(None, None, Some("0.0.0.0:502".into())),
            Ok(Some(Endpoint::UdpBind(ref a))) if a == "0.0.0.0:502"
        ));
    }

    #[tokio::test]
    async fn tcp_listener_decodes_a_split_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = broadcast::channel(16);
        tokio::spawn(run_tcp_listen(listener, "modbus".into(), tx));

        let mut client = TcpStream::connect(addr).await.unwrap();
        // Two frames sent in three pieces, so the framer must wait for the rest.
        client.write_all(&WRITE_REGISTER[..5]).await.unwrap();
        client.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        client
            .write_all(&[&WRITE_REGISTER[5..], &WRITE_REGISTER_2[..3]].concat())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        client.write_all(&WRITE_REGISTER_2[3..]).await.unwrap();

        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let second = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.value, TupValue::Int(10));
        assert_eq!(second.value, TupValue::Int(20));
        assert_eq!(
            first.source_protocol,
            tpt_protocol_tup::SourceProtocol::Modbus
        );
    }

    #[tokio::test]
    async fn tcp_connect_reads_from_a_device_and_reconnects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let (tx, mut rx) = broadcast::channel(16);
        tokio::spawn(run_tcp_connect(addr, "modbus".into(), tx));

        // First connection sends one frame and closes. The client reconnects.
        let (mut first, _) = listener.accept().await.unwrap();
        first.write_all(&WRITE_REGISTER).await.unwrap();
        drop(first);
        let dp = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dp.value, TupValue::Int(10));

        let (mut second, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
            .await
            .unwrap()
            .unwrap();
        second.write_all(&WRITE_REGISTER_2).await.unwrap();
        let dp = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dp.value, TupValue::Int(20));
    }

    #[tokio::test]
    async fn udp_decodes_one_frame_per_datagram() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let (tx, mut rx) = broadcast::channel(16);
        tokio::spawn(run_udp(socket, "modbus".into(), tx));

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(&WRITE_REGISTER, addr).await.unwrap();
        let dp = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dp.value, TupValue::Int(10));
    }
}
