//! TCP (connect or listen) and UDP transports.

use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

use crate::transport::{supervise, ByteLink, Framing, ReaderLink, Transport};

/// Largest UDP payload the socket can receive.
const MAX_DATAGRAM: usize = 65_535;

pub async fn run_tcp_connect(
    addr: String,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    supervise(TcpConnect { addr }, protocol, tx).await
}

pub async fn run_tcp_listen(
    listener: TcpListener,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    supervise(TcpListen { listener }, protocol, tx).await
}

pub async fn run_udp(
    socket: UdpSocket,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    let label = socket
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    supervise(
        UdpBind {
            socket: Some(socket),
            label,
        },
        protocol,
        tx,
    )
    .await
}

/// Connects to a device. A failed or closed link is retried by the supervisor.
struct TcpConnect {
    addr: String,
}

impl Transport for TcpConnect {
    type Link = ReaderLink<TcpStream>;
    const CONCURRENT: bool = false;

    fn describe(&self) -> String {
        format!("tcp connect {}", self.addr)
    }

    async fn next_link(&mut self) -> Result<Option<Self::Link>> {
        let stream = TcpStream::connect(&self.addr)
            .await
            .with_context(|| format!("connect to {} failed", self.addr))?;
        Ok(Some(ReaderLink::new(stream)))
    }
}

/// Accepts devices. Each accepted connection is its own link, served concurrently.
struct TcpListen {
    listener: TcpListener,
}

impl Transport for TcpListen {
    type Link = ReaderLink<TcpStream>;
    const CONCURRENT: bool = true;

    fn describe(&self) -> String {
        let addr = self
            .listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        format!("tcp listen {addr}")
    }

    async fn next_link(&mut self) -> Result<Option<Self::Link>> {
        let (stream, peer) = self.listener.accept().await?;
        tracing::info!("accepted {peer}");
        Ok(Some(ReaderLink::new(stream)))
    }
}

/// One UDP socket, read as datagrams for as long as the daemon runs.
struct UdpBind {
    socket: Option<UdpSocket>,
    label: String,
}

impl Transport for UdpBind {
    type Link = DatagramLink;
    const CONCURRENT: bool = false;

    fn describe(&self) -> String {
        format!("udp {}", self.label)
    }

    async fn next_link(&mut self) -> Result<Option<Self::Link>> {
        Ok(self.socket.take().map(|socket| DatagramLink {
            socket,
            buf: vec![0u8; MAX_DATAGRAM],
        }))
    }
}

/// Each UDP datagram is one message.
pub struct DatagramLink {
    socket: UdpSocket,
    buf: Vec<u8>,
}

impl ByteLink for DatagramLink {
    fn framing(&self) -> Framing {
        Framing::Datagram
    }

    async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        let (n, _peer) = self.socket.recv_from(&mut self.buf).await?;
        Ok(Some(self.buf[..n].to_vec()))
    }
}
