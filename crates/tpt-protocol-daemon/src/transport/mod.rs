//! Real transports and the code that runs them.
//!
//! A [`Transport`] yields [`ByteLink`]s: a connection to a device, an accepted
//! peer, a datagram socket, a serial port, a CAN bus, or stdin. The generic
//! [`supervise`] loop runs each link through the protocol's `try_parse` and
//! normalizer, reconnects where needed, and sends the TUP points on a broadcast
//! channel. Adding a transport means implementing these two traits; the framer
//! and supervisor do not change.
//!
//! Stream links (TCP, serial, stdin) are split into frames by `try_parse`. A
//! datagram link (UDP, CAN) is one unit per read, so each read is decoded on
//! its own. A stream that fails to parse drops its buffer and resyncs. Frames
//! that parse but carry no data (such as read requests) are skipped.

mod can;
mod net;
#[cfg(feature = "modbus")]
mod poll;
mod serial;
mod stdin;

pub use can::can_message;
pub(crate) use poll::register_points;
#[cfg(feature = "modbus")]
pub use poll::PollSpec;

use std::future::Future;
use std::time::Duration;

use anyhow::{bail, Result};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

use crate::plugins;
use can::run_can;
use net::{run_tcp_connect, run_tcp_listen, run_udp};
use serial::run_serial;
use stdin::run_stdin;

/// How long a client waits before reconnecting.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
/// Largest buffered-but-unframed byte count before the stream resyncs.
const MAX_BUFFER: usize = 64 * 1024;
/// Bytes requested per read from a stream.
const READ_CHUNK: usize = 4096;

/// Where the daemon gets its bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum Endpoint {
    /// Connect to a device, and reconnect when the link drops.
    TcpConnect(String),
    /// Accept connections from devices.
    TcpListen(String),
    /// Receive datagrams. Each datagram holds whole frames.
    UdpBind(String),
    /// Read a serial port, reopening it when it fails. Needs the `serial` feature.
    Serial { path: String, baud: u32 },
    /// Read a SocketCAN interface such as `can0` or `vcan0`. Needs the
    /// `socketcan` feature and Linux.
    Can { interface: String },
    /// Read bytes piped into the daemon, such as `cat capture.bin | ...`.
    Stdin,
    /// Poll a Modbus TCP server for registers every `period`. Needs the
    /// `modbus` feature. Use [`poll_endpoint`] to build one.
    #[cfg(feature = "modbus")]
    ModbusPoll {
        addr: String,
        spec: PollSpec,
        period: Duration,
    },
}

/// Builds a Modbus TCP poll endpoint, checking the spec. Without the `modbus`
/// feature this is an error, because there is no Modbus code to poll with.
pub fn poll_endpoint(
    addr: String,
    unit: u8,
    function: u8,
    address: u16,
    quantity: u16,
    period: Duration,
) -> Result<Endpoint> {
    #[cfg(feature = "modbus")]
    {
        let spec = PollSpec::new(unit, function, address, quantity)?;
        Ok(Endpoint::ModbusPoll { addr, spec, period })
    }
    #[cfg(not(feature = "modbus"))]
    {
        let _ = (addr, unit, function, address, quantity, period);
        bail!("--modbus-poll needs a daemon built with the `modbus` feature")
    }
}

impl Endpoint {
    /// Picks the endpoint from the command-line flags. At most one may be set.
    pub fn from_flags(
        tcp_connect: Option<String>,
        tcp_listen: Option<String>,
        udp_bind: Option<String>,
        serial: Option<String>,
        baud: u32,
        can: Option<String>,
        stdin: bool,
    ) -> Result<Option<Self>> {
        let chosen: Vec<Endpoint> = [
            tcp_connect.map(Endpoint::TcpConnect),
            tcp_listen.map(Endpoint::TcpListen),
            udp_bind.map(Endpoint::UdpBind),
            serial.map(|path| Endpoint::Serial { path, baud }),
            can.map(|interface| Endpoint::Can { interface }),
            stdin.then_some(Endpoint::Stdin),
        ]
        .into_iter()
        .flatten()
        .collect();
        match chosen.len() {
            0 => Ok(None),
            1 => Ok(chosen.into_iter().next()),
            _ => bail!(
                "choose only one of --tcp-connect, --tcp-listen, --udp-bind, --serial, --can and --stdin"
            ),
        }
    }
}

/// How a link's bytes divide into messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// A byte stream. Frames may split across reads or share one.
    Stream,
    /// Each read is one whole message, such as a UDP datagram or a CAN frame.
    Datagram,
}

/// A connected source of bytes.
pub trait ByteLink: Send + 'static {
    /// Whether reads are a stream or whole messages.
    fn framing(&self) -> Framing;
    /// The next chunk of bytes, or `Ok(None)` when the link has ended.
    fn read_chunk(&mut self) -> impl Future<Output = Result<Option<Vec<u8>>>> + Send;
}

/// A source of links.
pub trait Transport: Send + 'static {
    type Link: ByteLink;
    /// True when accepted links run side by side. False when links run one at
    /// a time, and the next one is only requested after the previous ends.
    const CONCURRENT: bool;
    /// A short description for the logs.
    fn describe(&self) -> String;
    /// The next link, or `Ok(None)` when no more links will come. An `Err`
    /// is logged and retried after [`RECONNECT_DELAY`].
    fn next_link(&mut self) -> impl Future<Output = Result<Option<Self::Link>>> + Send;
}

/// Adapts any async reader (a TCP stream, a serial port, stdin) to a stream link.
pub struct ReaderLink<R> {
    reader: R,
}

impl<R> ReaderLink<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: AsyncRead + Unpin + Send + 'static> ByteLink for ReaderLink<R> {
    fn framing(&self) -> Framing {
        Framing::Stream
    }

    async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        let mut buf = vec![0u8; READ_CHUNK];
        let n = self.reader.read(&mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.truncate(n);
        Ok(Some(buf))
    }
}

/// Decodes every complete frame at the front of `buf`, removing the bytes
/// used. Points are appended to `out`, so the points decoded before an error
/// are kept. Returns the number of frames that parsed but carried no data.
pub fn drain(protocol: &str, buf: &mut Vec<u8>, out: &mut Vec<DataPoint>) -> Result<usize, String> {
    let mut skipped = 0;
    loop {
        // An empty buffer holds no message yet. It is not a parse error, so it
        // must not reach the protocol's parser, which may report one.
        if buf.is_empty() {
            return Ok(skipped);
        }
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

/// Reads one link until it ends, sending each decoded point on `tx`.
pub async fn pump<L: ByteLink>(
    mut link: L,
    protocol: &str,
    tx: &broadcast::Sender<DataPoint>,
) -> Result<()> {
    let framing = link.framing();
    let mut buf: Vec<u8> = Vec::with_capacity(READ_CHUNK);
    while let Some(chunk) = link.read_chunk().await? {
        match framing {
            Framing::Stream => buf.extend_from_slice(&chunk),
            Framing::Datagram => buf = chunk,
        }
        let mut points = Vec::new();
        let result = drain(protocol, &mut buf, &mut points);
        for dp in points {
            let _ = tx.send(dp);
        }
        if let Err(e) = result {
            tracing::warn!("{protocol}: {e}; discarding {} buffered bytes", buf.len());
            buf.clear();
        }
        if framing == Framing::Datagram && !buf.is_empty() {
            tracing::warn!(
                "{protocol}: {} trailing bytes are not a complete frame",
                buf.len()
            );
            buf.clear();
        }
        if buf.len() > MAX_BUFFER {
            tracing::warn!(
                "{protocol}: {} bytes without a complete frame; resyncing",
                buf.len()
            );
            buf.clear();
        }
    }
    Ok(())
}

fn finish_link(result: Result<()>, protocol: &str) {
    match result {
        Ok(()) => tracing::info!("{protocol}: link closed"),
        Err(e) => tracing::warn!("{protocol}: link failed: {e:#}"),
    }
}

/// Runs a transport for the life of the daemon: opens links, reads them, and
/// reopens them as the transport allows.
pub async fn supervise<T: Transport>(
    mut transport: T,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    loop {
        match transport.next_link().await {
            Ok(Some(link)) => {
                tracing::info!("{protocol}: link opened ({})", transport.describe());
                if T::CONCURRENT {
                    let (protocol, tx) = (protocol.clone(), tx.clone());
                    tokio::spawn(async move {
                        finish_link(pump(link, &protocol, &tx).await, &protocol);
                    });
                } else {
                    finish_link(pump(link, &protocol, &tx).await, &protocol);
                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            }
            Ok(None) => return Ok(()),
            Err(e) => {
                tracing::warn!("{protocol}: {e:#}");
                tokio::time::sleep(RECONNECT_DELAY).await;
            }
        }
    }
}

/// Binds the endpoint and runs it until it is finished (a stdin or UDP source
/// ends; network and serial sources run for the life of the daemon).
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
        Endpoint::Serial { path, baud } => run_serial(path, baud, protocol, tx).await,
        Endpoint::Can { interface } => run_can(interface, protocol, tx).await,
        Endpoint::Stdin => run_stdin(protocol, tx).await,
        #[cfg(feature = "modbus")]
        Endpoint::ModbusPoll { addr, spec, period } => {
            poll::run_modbus_poll(addr, spec, period, tx).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tokio::io::AsyncWriteExt;
    use tokio::net::{TcpListener, TcpStream, UdpSocket};
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
    fn an_empty_buffer_is_not_a_parse_error() {
        // A datagram that ends exactly at a frame boundary leaves nothing to parse.
        // The MAVLink parser reports an empty input as an error, which must not reach
        // the log as one.
        let mut out = Vec::new();
        let mut empty = Vec::new();
        assert_eq!(drain("mavlink", &mut empty, &mut out), Ok(0));
        let mut one = WRITE_REGISTER.to_vec();
        assert_eq!(drain("modbus", &mut one, &mut out), Ok(0));
        assert!(one.is_empty());
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
        assert!(matches!(
            Endpoint::from_flags(None, None, None, None, 9600, None, false),
            Ok(None)
        ));
        assert!(Endpoint::from_flags(
            Some("a".into()),
            Some("b".into()),
            None,
            None,
            9600,
            None,
            false
        )
        .is_err());
        assert!(matches!(
            Endpoint::from_flags(None, None, None, Some("/dev/ttyUSB0".into()), 9600, None, false),
            Ok(Some(Endpoint::Serial { ref path, baud: 9600 })) if path == "/dev/ttyUSB0"
        ));
        assert!(matches!(
            Endpoint::from_flags(None, None, Some("0.0.0.0:502".into()), None, 9600, None, false),
            Ok(Some(Endpoint::UdpBind(ref a))) if a == "0.0.0.0:502"
        ));
        assert!(matches!(
            Endpoint::from_flags(None, None, None, None, 9600, None, true),
            Ok(Some(Endpoint::Stdin))
        ));
    }

    #[test]
    fn can_message_frames_each_protocol_as_its_parser_expects() {
        use self::can::can_message;
        // CANopen: TPDO1 (0x181), COB-ID big-endian, then the payload.
        assert_eq!(
            can_message("canopen", 0x181, &[0x01, 0x02]),
            Some(vec![0x01, 0x81, 0x01, 0x02])
        );
        // J1939: 29-bit identifier big-endian, then the payload.
        assert_eq!(
            can_message("j1939", 0x18F00412, &[0xAA]),
            Some(vec![0x18, 0xF0, 0x04, 0x12, 0xAA])
        );
        // Protocols that are not on CAN.
        assert_eq!(can_message("modbus", 0x181, &[0x01]), None);
    }

    #[test]
    fn can_endpoint_is_part_of_the_flag_choice() {
        assert!(matches!(
            Endpoint::from_flags(None, None, None, None, 9600, Some("vcan0".into()), false),
            Ok(Some(Endpoint::Can { ref interface })) if interface == "vcan0"
        ));
        assert!(Endpoint::from_flags(
            Some("a".into()),
            None,
            None,
            None,
            9600,
            Some("vcan0".into()),
            false
        )
        .is_err());
    }

    /// A transport that hands out a fixed script of results, one per call.
    struct Scripted(VecDeque<Result<Option<ReaderLink<&'static [u8]>>>>);

    impl Transport for Scripted {
        type Link = ReaderLink<&'static [u8]>;
        const CONCURRENT: bool = false;
        fn describe(&self) -> String {
            "scripted".into()
        }
        async fn next_link(&mut self) -> Result<Option<Self::Link>> {
            self.0.pop_front().unwrap_or(Ok(None))
        }
    }

    #[tokio::test]
    async fn supervisor_retries_a_failed_link_then_stops_at_none() {
        static FRAME: &[u8] = &WRITE_REGISTER;
        let (tx, mut rx) = broadcast::channel(16);
        let transport = Scripted(VecDeque::from(vec![
            Err(anyhow::anyhow!("device not ready")),
            Ok(Some(ReaderLink::new(FRAME))),
            Ok(None),
        ]));
        tokio::time::timeout(
            Duration::from_secs(10),
            supervise(transport, "modbus".into(), tx),
        )
        .await
        .expect("supervisor returns once the script ends")
        .unwrap();
        let dp = rx.try_recv().expect("the scripted link delivered a point");
        assert_eq!(dp.value, TupValue::Int(10));
    }

    /// A datagram link: each item is one read.
    struct Datagrams(VecDeque<Vec<u8>>);

    impl ByteLink for Datagrams {
        fn framing(&self) -> Framing {
            Framing::Datagram
        }
        async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
            Ok(self.0.pop_front())
        }
    }

    #[tokio::test]
    async fn datagram_links_decode_each_read_on_its_own() {
        let (tx, mut rx) = broadcast::channel(16);
        let link = Datagrams(VecDeque::from(vec![
            WRITE_REGISTER.to_vec(),
            // A frame cut short by the datagram boundary is discarded, not
            // joined to the next datagram.
            WRITE_REGISTER[..7].to_vec(),
            WRITE_REGISTER_2.to_vec(),
        ]));
        pump(link, "modbus", &tx).await.unwrap();
        assert_eq!(rx.try_recv().unwrap().value, TupValue::Int(10));
        assert_eq!(rx.try_recv().unwrap().value, TupValue::Int(20));
        assert!(rx.try_recv().is_err());
    }

    /// Runs against a SocketCAN interface. Create it with
    /// `sudo modprobe vcan && sudo ip link add dev vcan0 type vcan && sudo ip link set up vcan0`,
    /// then run `TPT_TEST_CAN_IF=vcan0 cargo test --features socketcan -- --ignored can_`.
    #[cfg(all(feature = "socketcan", target_os = "linux"))]
    #[tokio::test]
    #[ignore = "needs a SocketCAN interface (see the doc comment)"]
    async fn can_interface_decodes_canopen_pdo() {
        use socketcan::tokio::CanSocket;
        use socketcan::{CanFrame, EmbeddedFrame, StandardId};
        let interface = std::env::var("TPT_TEST_CAN_IF").expect("TPT_TEST_CAN_IF");
        let (tx, mut rx) = broadcast::channel(16);
        tokio::spawn(run_can(interface.clone(), "canopen".into(), tx));
        tokio::time::sleep(Duration::from_millis(300)).await;

        // TPDO1 from node 1 (COB-ID 0x181), payload 0x0201 little-endian.
        let writer = CanSocket::open(&interface).unwrap();
        let frame =
            CanFrame::new(StandardId::new(0x181).unwrap(), &[0x01, 0x02, 0x00, 0x00]).unwrap();
        writer.write_frame(frame).await.unwrap();

        let dp = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dp.value, TupValue::Int(0x0201));
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

        // The first connection sends one frame and closes. The client reconnects.
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

    /// Runs against a real serial port pair. Set TPT_TEST_SERIAL_READ to the
    /// port the daemon reads and TPT_TEST_SERIAL_PATH to the port the test
    /// writes, such as the two ends of a `socat` pseudo-terminal pair. Run with
    /// `cargo test --features serial -- --ignored`.
    #[cfg(feature = "serial")]
    #[tokio::test]
    #[ignore = "needs a serial port pair (see the doc comment)"]
    async fn serial_port_decodes_frames() {
        use tokio_serial::SerialPortBuilderExt;
        let read_path = std::env::var("TPT_TEST_SERIAL_READ").expect("TPT_TEST_SERIAL_READ");
        let write_path = std::env::var("TPT_TEST_SERIAL_PATH").expect("TPT_TEST_SERIAL_PATH");
        let (tx, mut rx) = broadcast::channel(16);
        tokio::spawn(run_serial(read_path, 115_200, "modbus".into(), tx));
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut writer = tokio_serial::new(&write_path, 115_200)
            .open_native_async()
            .unwrap();
        // Modbus RTU: Write Single Register (slave 1, address 1, value 10).
        let rtu = [0x01, 0x06, 0x00, 0x01, 0x00, 0x0A, 0x58, 0x0D]; // CRC 0x0D58, low byte first
        writer.write_all(&rtu).await.unwrap();
        let dp = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dp.value, TupValue::Int(10));
    }
}
