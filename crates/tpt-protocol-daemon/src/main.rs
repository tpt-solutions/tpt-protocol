mod anomaly;
mod capture;
mod detect;
mod mapping;
mod plugins;
mod record;
mod transport;

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, OwnedSemaphorePermit, Semaphore};
use tokio_tungstenite::accept_hdr_async_with_config;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tracing_subscriber::EnvFilter;

use tpt_protocol_tup::{json, DataPoint};

const DEFAULT_PORT: u16 = 9080;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CLIENTS: usize = 64;
const MAX_INCOMING_MESSAGE: usize = 64 * 1024;
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

/// Origins a Tauri webview uses to load its frontend. Browsers always send an
/// `Origin` header on WebSocket handshakes, so this is what lets `ui-web`
/// connect while still rejecting arbitrary web pages.
const TAURI_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

#[derive(Parser)]
#[command(name = "tpt-protocol-daemon", version = "0.1.0")]
struct Args {
    /// Simulate this protocol instead of a real device. See `--list-protocols`.
    #[arg(long)]
    simulate: Option<String>,
    /// TOML config file. CLI flags take precedence over values in the file.
    #[arg(long)]
    config: Option<String>,
    /// Port to listen on (default 9080).
    #[arg(long)]
    port: Option<u16>,
    /// Address to bind (default 127.0.0.1).
    #[arg(long)]
    bind: Option<String>,
    /// Print the list of supported protocols and exit.
    #[arg(long)]
    list_protocols: bool,
    /// Replay bytes from a file through the protocol chosen by `--simulate`
    /// instead of running the synthetic simulation.
    #[arg(long)]
    replay: Option<String>,
    /// How `--replay` interprets the file: `hex` (ASCII hex text), `raw` (binary),
    /// or `candump` (a `candump -L` CAN capture, decoded frame by frame).
    #[arg(long, default_value = "hex", value_parser = ["hex", "raw", "candump"])]
    replay_format: String,
    /// Rank the protocols by how well they decode a capture (hex or raw) and exit.
    #[arg(long)]
    detect: Option<String>,
    /// Append every data point to this file as NDJSON, one JSON object per line.
    #[arg(long)]
    record: Option<std::path::PathBuf>,
    /// Require this token on WebSocket connections, as `?token=` or `Authorization: Bearer`.
    #[arg(long)]
    token: Option<String>,
    /// Additional browser Origin allowed to connect (repeatable), e.g. a dev server.
    #[arg(long = "allow-origin")]
    allow_origin: Vec<String>,
    /// Connect to a device at HOST:PORT over TCP, reconnecting when the link drops.
    #[arg(long = "tcp-connect")]
    tcp_connect: Option<String>,
    /// Listen for devices on ADDR:PORT over TCP.
    #[arg(long = "tcp-listen")]
    tcp_listen: Option<String>,
    /// Receive UDP datagrams on ADDR:PORT. Each datagram holds whole frames.
    #[arg(long = "udp-bind")]
    udp_bind: Option<String>,
    /// Read a serial port (needs the `serial` feature).
    #[arg(long = "serial")]
    serial: Option<String>,
    /// Serial port speed in baud.
    #[arg(long = "baud", default_value_t = 115_200)]
    baud: u32,
    /// Read a SocketCAN interface, such as can0 or vcan0 (Linux; needs the `socketcan` feature).
    #[arg(long = "can")]
    can: Option<String>,
    /// Read bytes from standard input, e.g. `cat capture.bin | tpt-protocol-daemon --stdin ...`.
    #[arg(long = "stdin")]
    stdin: bool,
    /// Milliseconds between simulated points (1 to 3600000). Ignored with a
    /// transport, except `--modbus-poll`, which uses it as the poll period.
    #[arg(long = "interval", default_value_t = 1000, value_parser = clap::value_parser!(u64).range(1..=3_600_000))]
    interval: u64,
    /// Poll a Modbus TCP server at HOST:PORT for registers, every --interval ms.
    /// Needs `--simulate modbus` and cannot be combined with another transport.
    #[arg(long = "modbus-poll")]
    modbus_poll: Option<String>,
    /// Unit (slave) id to poll.
    #[arg(long = "poll-unit", default_value_t = 1)]
    poll_unit: u8,
    /// Function to poll: 3 (holding registers) or 4 (input registers).
    #[arg(long = "poll-function", default_value_t = 3)]
    poll_function: u8,
    /// First register to read.
    #[arg(long = "poll-address", default_value_t = 0)]
    poll_address: u16,
    /// Number of registers to read, 1 to 125.
    #[arg(long = "poll-quantity", default_value_t = 10)]
    poll_quantity: u16,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Config {
    simulate: Option<String>,
    port: Option<u16>,
    bind: Option<String>,
    /// Declarative mapping rules: see `mapping.rs`.
    #[serde(default)]
    map: Vec<mapping::MapRule>,
    /// Anomaly checks on the stream: see `anomaly.rs`.
    #[serde(default)]
    anomaly: Vec<anomaly::AnomalyRule>,
}

#[derive(Debug, PartialEq)]
struct Settings {
    simulate: Option<String>,
    bind: String,
    port: u16,
}

/// CLI flags win over the config file, which wins over built-in defaults.
fn resolve_settings(
    cli_simulate: Option<String>,
    cli_port: Option<u16>,
    cli_bind: Option<String>,
    file: Config,
) -> Settings {
    Settings {
        simulate: cli_simulate.or(file.simulate),
        bind: cli_bind
            .or(file.bind)
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        port: cli_port.or(file.port).unwrap_or(DEFAULT_PORT),
    }
}

/// Access rules applied to every WebSocket handshake.
struct WsPolicy {
    token: Option<String>,
    /// Normalized (lowercase, no trailing slash) allowed `Origin` values.
    allowed_origins: Vec<String>,
    /// Only enforced when bound to loopback: DNS-rebinding defence.
    check_host: bool,
}

/// State shared by every connection: the access policy, the data channel, the
/// connection slots and the counters that `/healthz` reports.
struct ServerState {
    policy: WsPolicy,
    tx: broadcast::Sender<DataPoint>,
    slots: Arc<Semaphore>,
    started: Instant,
    messages_sent: AtomicU64,
    lagged_events: AtomicU64,
    dropped_messages: AtomicU64,
    rejected_handshakes: AtomicU64,
}

impl ServerState {
    fn new(policy: WsPolicy, tx: broadcast::Sender<DataPoint>) -> Self {
        Self {
            policy,
            tx,
            slots: Arc::new(Semaphore::new(MAX_CLIENTS)),
            started: Instant::now(),
            messages_sent: AtomicU64::new(0),
            lagged_events: AtomicU64::new(0),
            dropped_messages: AtomicU64::new(0),
            rejected_handshakes: AtomicU64::new(0),
        }
    }
}

fn normalize_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Splits a `Host` header value into `(name, port)`, handling `[v6]:port`.
fn split_host(host: &str) -> (&str, Option<&str>) {
    if let Some(rest) = host.strip_prefix('[') {
        return match rest.split_once(']') {
            Some((inner, after)) => (inner, after.strip_prefix(':')),
            None => (host, None),
        };
    }
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') => (name, Some(port)),
        _ => (host, None),
    }
}

fn is_loopback_name(name: &str) -> bool {
    let name = name.trim_end_matches('.');
    if name.eq_ignore_ascii_case("localhost") {
        return true;
    }
    name.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Returns true if `bind` is a loopback address (`localhost`, `127.x.x.x`,
/// `::1`, bracketed or not). We warn rather than refuse otherwise, since the
/// WebSocket API is only as protected as the network it is exposed on.
fn is_loopback_bind(bind: &str) -> bool {
    is_loopback_name(bind.trim_start_matches('[').trim_end_matches(']'))
}

fn host_header_is_loopback(host: &str) -> bool {
    let (name, _port) = split_host(host);
    is_loopback_name(name)
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn token_matches(req: &Request, expected: &str) -> bool {
    let from_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let from_query = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")));
    [from_header, from_query]
        .into_iter()
        .flatten()
        .any(|candidate| ct_eq(candidate.as_bytes(), expected.as_bytes()))
}

fn authorize(req: &Request, policy: &WsPolicy) -> Result<(), (StatusCode, &'static str)> {
    if policy.check_host {
        let host = req
            .headers()
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !host_header_is_loopback(host) {
            return Err((StatusCode::FORBIDDEN, "host not allowed"));
        }
    }
    if let Some(origin) = req.headers().get("origin") {
        let origin = origin.to_str().map(normalize_origin).unwrap_or_default();
        if !policy.allowed_origins.contains(&origin) {
            return Err((StatusCode::FORBIDDEN, "origin not allowed"));
        }
    }
    if let Some(expected) = &policy.token {
        if !token_matches(req, expected) {
            return Err((StatusCode::UNAUTHORIZED, "missing or invalid token"));
        }
    }
    Ok(())
}

/// Handshake callback. Rejections are HTTP responses, as tungstenite requires,
/// so the large `Err` type is fixed by the `Callback` trait.
#[allow(clippy::result_large_err)]
fn on_handshake(
    req: &Request,
    resp: Response,
    policy: &WsPolicy,
) -> Result<Response, ErrorResponse> {
    match authorize(req, policy) {
        Ok(()) => Ok(resp),
        Err((status, reason)) => {
            tracing::warn!("rejected WebSocket handshake ({status}): {reason}");
            let mut rejection = ErrorResponse::new(Some(reason.to_string()));
            *rejection.status_mut() = status;
            Err(rejection)
        }
    }
}

/// Decodes a hex replay file. Whitespace is ignored and an optional `0x`
/// prefix is accepted. Binary input is rejected with a pointer to `raw` mode.
fn decode_hex_file(raw: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(raw).map_err(|_| {
        anyhow!("replay file is not ASCII hex text; use --replay-format raw for binary captures")
    })?;
    let trimmed = text.trim();
    let trimmed = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let compact: String = trimmed.split_whitespace().collect();
    hex::decode(&compact).map_err(|e| {
        anyhow!("invalid hex in replay file: {e} (use --replay-format raw for binary captures)")
    })
}

/// Turns the bytes of a `--replay` file into the frame bytes to parse.
fn replay_bytes(format: &str, raw: Vec<u8>) -> Result<Vec<u8>> {
    match format {
        "hex" => decode_hex_file(&raw),
        "raw" => Ok(raw),
        other => bail!("unknown --replay-format '{other}' (expected hex or raw)"),
    }
}

/// Decodes a `candump -L` capture with the CAN protocol `protocol`. Each frame is
/// framed the way the live CAN transport frames it, and each point takes the
/// frame's time from the capture.
fn replay_candump(protocol: &str, raw: &[u8]) -> Result<Vec<tpt_protocol_tup::DataPoint>> {
    if transport::can_message(protocol, 0, &[]).is_none() {
        bail!(
            "{protocol} is not carried on CAN, so a candump capture cannot be replayed through it"
        );
    }
    let text = std::str::from_utf8(raw).map_err(|_| anyhow!("a candump file is text"))?;
    let records = capture::parse_candump(text).map_err(|e| anyhow!("candump: {e}"))?;
    let mut out = Vec::new();
    for record in records {
        let Some(mut message) = transport::can_message(protocol, record.id, &record.data) else {
            continue;
        };
        let mut points = Vec::new();
        // A frame the parser rejects is reported and skipped; the rest of the capture
        // still replays. Points decoded before the error are kept.
        if let Err(e) = transport::drain(protocol, &mut message, &mut points) {
            tracing::warn!("candump: {protocol} frame {:#X} skipped: {e}", record.id);
        }
        for mut dp in points {
            // The capture's own time is the time of the frame, so it replaces the
            // ingest time that `decode_one` stamped.
            dp.timestamp = record.timestamp_ms;
            out.push(dp);
        }
    }
    Ok(out)
}

/// Waits for the request line (peeking, so the WebSocket handshake still sees it)
/// and reports whether it is `GET /healthz`.
async fn request_is_healthz(stream: &TcpStream) -> bool {
    let mut buf = [0u8; 256];
    let peeked = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        loop {
            let n = stream.peek(&mut buf).await?;
            if n == 0 || n == buf.len() || buf[..n].contains(&b'\n') {
                return Ok::<usize, std::io::Error>(n);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    matches!(peeked, Ok(Ok(n)) if buf[..n].starts_with(b"GET /healthz "))
}

/// Counters for operators. No data points, no device names: only totals.
fn healthz_body(state: &ServerState) -> String {
    serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": state.started.elapsed().as_secs(),
        "clients": {
            "active": MAX_CLIENTS - state.slots.available_permits(),
            "max": MAX_CLIENTS,
        },
        "messages_sent": state.messages_sent.load(Ordering::Relaxed),
        "lagged_events": state.lagged_events.load(Ordering::Relaxed),
        "dropped_messages": state.dropped_messages.load(Ordering::Relaxed),
        "rejected_handshakes": state.rejected_handshakes.load(Ordering::Relaxed),
    })
    .to_string()
}

async fn write_healthz(stream: &mut TcpStream, state: &ServerState) -> std::io::Result<()> {
    // Read the request so that closing the socket does not reset the response.
    let mut scratch = [0u8; 4096];
    let _ = stream.read(&mut scratch).await?;
    let body = healthz_body(state);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

async fn handle_connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    state: Arc<ServerState>,
    _permit: OwnedSemaphorePermit,
) {
    if request_is_healthz(&stream).await {
        if let Err(e) = write_healthz(&mut stream, &state).await {
            tracing::debug!("health check to {peer} failed: {e}");
        }
        return;
    }

    let cb_state = Arc::clone(&state);
    #[allow(clippy::result_large_err)] // same tungstenite-imposed type as `on_handshake`
    let callback = move |req: &Request, resp: Response| {
        on_handshake(req, resp, &cb_state.policy).inspect_err(|_| {
            cb_state.rejected_handshakes.fetch_add(1, Ordering::Relaxed);
        })
    };

    let config = WebSocketConfig {
        max_message_size: Some(MAX_INCOMING_MESSAGE),
        max_frame_size: Some(MAX_INCOMING_MESSAGE),
        ..Default::default()
    };

    let ws_stream = match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        accept_hdr_async_with_config(stream, callback, Some(config)),
    )
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::warn!("WebSocket handshake error: {e}");
            return;
        }
        Err(_) => {
            tracing::warn!("WebSocket handshake timed out");
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();
    let mut rx = state.tx.subscribe();

    loop {
        tokio::select! {
            incoming = read.next() => match incoming {
                Some(Ok(Message::Ping(payload))) => {
                    if write.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                // Clients have nothing to say; ignore text, binary and pong frames.
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    tracing::warn!("WebSocket read error: {e}");
                    break;
                }
            },
            outgoing = rx.recv() => match outgoing {
                Ok(dp) => {
                    let json = match json::to_json_string(&dp) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::warn!("JSON serialization error: {e}");
                            continue;
                        }
                    };
                    if write.send(Message::Text(format!("[{json}]"))).await.is_err() {
                        break;
                    }
                    state.messages_sent.fetch_add(1, Ordering::Relaxed);
                }
                Err(broadcast::error::RecvError::Lagged(dropped)) => {
                    state.lagged_events.fetch_add(1, Ordering::Relaxed);
                    state.dropped_messages.fetch_add(dropped, Ordering::Relaxed);
                    // Tell the client its stream has a gap. This is a JSON object,
                    // not an array, so clients must check for `"type":"lagged"`.
                    let notice = format!(r#"{{"type":"lagged","dropped":{dropped}}}"#);
                    if write.send(Message::Text(notice)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}

async fn ws_server(listener: TcpListener, state: Arc<ServerState>) -> Result<()> {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                tracing::warn!("accept error: {e}");
                tokio::time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };

        // Every connection, including a health check, takes a slot. That bounds
        // the number of tasks a flood of connections can create.
        let permit = match Arc::clone(&state.slots).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                tracing::warn!("rejecting {peer}: connection limit ({MAX_CLIENTS}) reached");
                continue;
            }
        };

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            handle_connection(stream, peer, state, permit).await;
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.list_protocols {
        println!("Supported protocols:");
        for name in plugins::list_protocols() {
            println!("  {name}");
        }
        return Ok(());
    }

    // Diagnostics go to stderr. Set RUST_LOG (e.g. `RUST_LOG=debug`) to change the level.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        // Colour only when a person is watching; redirected logs stay plain text.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();

    let file_config = match &args.config {
        Some(path) => toml::from_str::<Config>(&tokio::fs::read_to_string(path).await?)?,
        None => Config::default(),
    };
    for rule in &file_config.map {
        rule.validate()
            .map_err(|e| anyhow!("config [[map]]: {e}"))?;
    }
    for rule in &file_config.anomaly {
        rule.validate()
            .map_err(|e| anyhow!("config [[anomaly]]: {e}"))?;
    }
    let anomaly_rules = file_config.anomaly.clone();
    let map_rules = file_config.map.clone();
    let settings = resolve_settings(args.simulate, args.port, args.bind, file_config);

    if let Some(ref detect_path) = args.detect {
        let raw = tokio::fs::read(detect_path).await?;
        let bytes = replay_bytes(&args.replay_format, raw)?;
        println!(
            "{:<16} {:>8} {:>8} {:>7}",
            "protocol", "frames", "points", "errors"
        );
        for score in detect::rank(&bytes) {
            println!(
                "{:<16} {:>8} {:>8} {:>7}",
                score.protocol, score.frames, score.points, score.errors
            );
        }
        return Ok(());
    }

    if let Some(ref replay_path) = args.replay {
        let protocol = settings.simulate.as_deref().ok_or_else(|| {
            anyhow!(
                "--replay needs --simulate <protocol> to choose the parser (see --list-protocols)"
            )
        })?;
        let raw = tokio::fs::read(replay_path).await?;
        if args.replay_format == "candump" {
            for dp in replay_candump(protocol, &raw)? {
                println!("{}", json::to_json_string(&dp)?);
            }
        } else {
            let bytes = replay_bytes(&args.replay_format, raw)?;
            let dp = plugins::replay_one(protocol, &bytes)
                .map_err(|e| anyhow!("replay parse failed: {e}"))?;
            println!("{}", json::to_json_string(&dp)?);
        }
        return Ok(());
    }

    let protocol = match settings.simulate.clone() {
        Some(p) => p,
        None => {
            tracing::warn!("no --simulate given; defaulting to modbus");
            "modbus".to_string()
        }
    };
    // Validates the protocol name before any socket is opened.
    plugins::build_plugin(&protocol).map_err(|e| anyhow!("{e}"))?;
    let transport_endpoint = transport::Endpoint::from_flags(
        args.tcp_connect.clone(),
        args.tcp_listen.clone(),
        args.udp_bind.clone(),
        args.serial.clone(),
        args.baud,
        args.can.clone(),
        args.stdin,
    )?;
    let transport_endpoint = match args.modbus_poll.clone() {
        Some(addr) => {
            if transport_endpoint.is_some() {
                bail!("choose either --modbus-poll or another transport, not both");
            }
            if protocol != "modbus" {
                bail!("--modbus-poll polls Modbus; use --simulate modbus");
            }
            Some(transport::poll_endpoint(
                addr,
                args.poll_unit,
                args.poll_function,
                args.poll_address,
                args.poll_quantity,
                Duration::from_millis(args.interval),
            )?)
        }
        None => transport_endpoint,
    };

    if let Some(ref token) = args.token {
        if token.is_empty() {
            bail!("--token must not be empty");
        }
    }

    let bind = settings.bind;
    let port = settings.port;
    let loopback = is_loopback_bind(&bind);
    let interval = Duration::from_millis(args.interval);
    let using_transport = transport_endpoint.is_some();

    println!("TPT Protocol Daemon v0.1.0");
    if using_transport {
        println!("Decoding {protocol} from a transport");
    } else {
        println!(
            "Simulating: {protocol} (one point every {} ms)",
            args.interval
        );
    }

    if !loopback {
        tracing::warn!(
            "binding to non-loopback address '{bind}'. Host-header checks are disabled; \
             expose the WebSocket API only on a trusted network and use --token."
        );
    }

    let mut allowed_origins: Vec<String> = TAURI_ORIGINS.iter().map(|o| o.to_string()).collect();
    allowed_origins.extend(args.allow_origin.iter().map(|o| normalize_origin(o)));
    let policy = WsPolicy {
        token: args.token,
        allowed_origins,
        check_host: loopback,
    };

    // Devices and the simulation write to `source_tx`. The mapping stage applies
    // the [[map]] rules and forwards to `tx`, which the clients and the recorder read.
    let (tx, _) = broadcast::channel::<DataPoint>(1024);
    let (source_tx, _) = broadcast::channel::<DataPoint>(1024);
    tokio::spawn(mapping::run(
        map_rules,
        anomaly_rules,
        source_tx.subscribe(),
        tx.clone(),
    ));
    if let Some(path) = args.record.clone() {
        let record_tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = record::run(&path, record_tx).await {
                tracing::error!("recording stopped: {e:#}");
            }
        });
    }
    let state = Arc::new(ServerState::new(policy, tx.clone()));

    // Bind before spawning anything, so a busy port is a startup error.
    let listener = TcpListener::bind((bind.as_str(), port))
        .await
        .map_err(|e| anyhow!("cannot listen on {bind}:{port}: {e}"))?;
    let local = listener.local_addr()?;

    match transport_endpoint {
        Some(endpoint) => {
            // Real bytes from a device replace the simulation.
            let real_tx = source_tx.clone();
            let real_protocol = protocol.clone();
            tokio::spawn(async move {
                match transport::run(endpoint, real_protocol.clone(), real_tx).await {
                    Ok(()) => tracing::warn!(
                        "{real_protocol}: transport finished; no more data will arrive"
                    ),
                    Err(e) => tracing::error!("{real_protocol}: transport stopped: {e}"),
                }
            });
        }
        None => {
            let stream =
                plugins::simulate_stream(&protocol, interval).map_err(|e| anyhow!("{e}"))?;
            let sim_tx = source_tx.clone();
            let sim_name = protocol.clone();
            tokio::spawn(async move {
                let mut stream = stream;
                while let Some(dp) = stream.next().await {
                    let _ = sim_tx.send(dp);
                }
                tracing::warn!("simulation stream for {sim_name} ended");
            });
        }
    }

    let server_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = ws_server(listener, server_state).await {
            tracing::error!("WebSocket server error: {e}");
        }
    });

    println!("WebSocket server listening on ws://{local}");
    println!("Health check: http://{local}/healthz");

    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_protocols_buildable() {
        // There are 19 protocol crates wired into the daemon.
        // The 19 built-in protocols at least; scaffolded protocols add to the count.
        assert!(plugins::list_protocols().len() >= 19);
        for name in plugins::list_protocols() {
            assert!(
                plugins::build_plugin(name).is_ok(),
                "failed to build {name}"
            );
        }
        assert!(plugins::build_plugin("not-a-protocol").is_err());
    }

    #[test]
    fn replayed_points_get_an_ingest_timestamp() {
        // Write Single Register (slave 1, address 1, value 10). The normalizer leaves timestamp 0.
        let mut bytes = vec![0x01, 0x06, 0x00, 0x01, 0x00, 0x0A];
        let mut crc: u16 = 0xFFFF;
        for &b in &bytes {
            crc ^= b as u16;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xA001
                } else {
                    crc >> 1
                };
            }
        }
        bytes.extend_from_slice(&crc.to_le_bytes());
        let dp = plugins::replay_one("modbus", &bytes).unwrap();
        assert!(
            dp.timestamp > 1_700_000_000_000,
            "timestamp {}",
            dp.timestamp
        );
    }

    #[test]
    fn loopback_bind_detection() {
        assert!(is_loopback_bind("127.0.0.1"));
        assert!(is_loopback_bind("localhost"));
        assert!(is_loopback_bind("LOCALHOST"));
        assert!(is_loopback_bind("localhost."));
        assert!(is_loopback_bind("::1"));
        assert!(is_loopback_bind("[::1]"));
        assert!(!is_loopback_bind("0.0.0.0"));
        assert!(!is_loopback_bind("192.168.1.10"));
        assert!(!is_loopback_bind("localhost.evil.example"));
    }

    #[test]
    fn host_header_must_be_loopback() {
        assert!(host_header_is_loopback("127.0.0.1:9080"));
        assert!(host_header_is_loopback("localhost:9080"));
        assert!(host_header_is_loopback("[::1]:9080"));
        assert!(!host_header_is_loopback("evil.example:9080"));
        assert!(!host_header_is_loopback("localhost.evil.example"));
        assert!(!host_header_is_loopback(""));
    }

    #[test]
    fn token_comparison_is_exact() {
        assert!(ct_eq(b"secret", b"secret"));
        assert!(!ct_eq(b"secret", b"secreT"));
        assert!(!ct_eq(b"secret", b"secret2"));
    }

    fn handshake(headers: &[(&str, &str)]) -> Request {
        let mut b = Request::builder().uri("/");
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(()).unwrap()
    }

    fn policy(token: Option<&str>, check_host: bool) -> WsPolicy {
        WsPolicy {
            token: token.map(str::to_string),
            allowed_origins: vec!["http://localhost:5173".to_string()],
            check_host,
        }
    }

    #[test]
    fn handshake_accepts_loopback_host_and_allowed_origin() {
        let p = policy(None, true);
        let req = handshake(&[
            ("host", "127.0.0.1:9080"),
            ("origin", "http://localhost:5173"),
        ]);
        assert!(authorize(&req, &p).is_ok());
        // Non-browser clients send no Origin, and that is allowed.
        assert!(authorize(&handshake(&[("host", "localhost:9080")]), &p).is_ok());
    }

    #[test]
    fn handshake_rejects_foreign_origin_and_foreign_host() {
        let p = policy(None, true);
        let evil = handshake(&[
            ("host", "127.0.0.1:9080"),
            ("origin", "https://evil.example"),
        ]);
        assert_eq!(authorize(&evil, &p).unwrap_err().0, StatusCode::FORBIDDEN);
        // DNS rebinding: the page is served from evil.example, so Host is not loopback.
        let rebind = handshake(&[("host", "evil.example:9080")]);
        assert_eq!(authorize(&rebind, &p).unwrap_err().0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn handshake_host_check_is_only_enforced_on_loopback() {
        // A daemon bound to a non-loopback address does not check Host.
        let p = policy(None, false);
        assert!(authorize(&handshake(&[("host", "daemon.lan:9080")]), &p).is_ok());
    }

    #[test]
    fn handshake_requires_the_token_when_one_is_set() {
        let p = policy(Some("s3cret"), true);
        let missing = handshake(&[("host", "127.0.0.1")]);
        assert_eq!(
            authorize(&missing, &p).unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
        let wrong = handshake(&[("host", "127.0.0.1"), ("authorization", "Bearer nope")]);
        assert_eq!(
            authorize(&wrong, &p).unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
        let bearer = handshake(&[("host", "127.0.0.1"), ("authorization", "Bearer s3cret")]);
        assert!(authorize(&bearer, &p).is_ok());
    }

    #[test]
    fn origin_normalization() {
        assert_eq!(
            normalize_origin("HTTP://Tauri.localhost/"),
            "http://tauri.localhost"
        );
    }

    #[test]
    fn cli_overrides_config_file() {
        let file = Config {
            simulate: Some("canopen".into()),
            port: Some(7000),
            bind: Some("0.0.0.0".into()),
            map: Vec::new(),
            anomaly: Vec::new(),
        };
        let s = resolve_settings(Some("modbus".into()), Some(9999), None, file);
        assert_eq!(
            s,
            Settings {
                simulate: Some("modbus".into()),
                bind: "0.0.0.0".into(),
                port: 9999,
            }
        );
    }

    #[test]
    fn defaults_when_nothing_set() {
        let s = resolve_settings(None, None, None, Config::default());
        assert_eq!(
            s,
            Settings {
                simulate: None,
                bind: "127.0.0.1".into(),
                port: DEFAULT_PORT,
            }
        );
    }

    #[test]
    fn config_rejects_unknown_fields() {
        assert!(toml::from_str::<Config>("prot = 1").is_err());
    }

    #[test]
    fn hex_replay_decoding() {
        assert_eq!(decode_hex_file(b"0x0102\n").unwrap(), vec![1, 2]);
        assert_eq!(decode_hex_file(b"01 02 ff").unwrap(), vec![1, 2, 255]);
        assert!(decode_hex_file(b"zz").is_err());
        // Binary input is not valid UTF-8 and must not be silently misread.
        assert!(decode_hex_file(&[0xff, 0x00, 0x12]).is_err());
    }

    /// The bundled samples are real Modbus RTU frames with correct CRCs, so
    /// each one must decode through the same path as `--replay`. `--replay`
    /// parses requests, so the samples are Write Single Register requests from
    /// slave 1: register 1 set to 0x0102, and register 2 set to 0x0064. The hex
    /// format has no comments, so the frames are described here.
    #[test]
    fn bundled_replay_samples_decode() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/samples");
        for (file, first_register) in [
            ("modbus-write-single-register-1.hex", 0x0102),
            ("modbus-write-single-register-2.hex", 0x0064),
        ] {
            let raw = std::fs::read(format!("{dir}/{file}")).unwrap();
            let bytes = decode_hex_file(&raw).unwrap_or_else(|e| panic!("{file}: {e}"));
            let dp =
                plugins::replay_one("modbus", &bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(
                dp.value,
                tpt_protocol_tup::TupValue::Int(first_register),
                "{file}"
            );
        }
    }

    /// Every bundled sample, its protocol, and the format `--replay` reads it in.
    /// Each sample must replay to a data point; that keeps the samples honest.
    const SAMPLES: &[(&str, &str, &str)] = &[
        ("modbus", "modbus-write-single-register-1.hex", "hex"),
        ("modbus", "modbus-write-single-register-2.hex", "hex"),
        ("j1939", "j1939-eec1.hex", "hex"),
        ("isobus", "isobus-pto.hex", "hex"),
        ("canopen", "canopen-tpdo1.hex", "hex"),
        ("uds", "uds-negative-response.hex", "hex"),
        ("mavlink", "mavlink-heartbeat.hex", "hex"),
        ("coap", "coap-response.hex", "hex"),
        ("bacnet", "bacnet-read-property-ack.hex", "hex"),
        ("doip-someip", "doip-diagnostic.hex", "hex"),
        ("dnp3", "dnp3-response.hex", "hex"),
        ("ethercat", "ethercat-lrd.hex", "hex"),
        ("knx", "knx-group-write.hex", "hex"),
        ("opcua", "opcua-message.hex", "hex"),
        ("matter-thread", "matter-report-data.hex", "hex"),
        ("sparkplugb", "sparkplugb-ddata.hex", "hex"),
        ("nmea", "nmea-gga.raw", "raw"),
        ("ocpp", "ocpp-boot-notification.raw", "raw"),
        ("hl7v2", "hl7v2-oru.raw", "raw"),
        ("ieee11073-sdc", "ieee11073-sdc-report.raw", "raw"),
    ];

    #[test]
    fn every_bundled_sample_replays_to_a_data_point() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/samples");
        for (protocol, file, format) in SAMPLES {
            let raw =
                std::fs::read(format!("{dir}/{file}")).unwrap_or_else(|e| panic!("{file}: {e}"));
            let bytes = replay_bytes(format, raw).unwrap_or_else(|e| panic!("{file}: {e}"));
            plugins::replay_one(protocol, &bytes)
                .unwrap_or_else(|e| panic!("{file} ({protocol}): {e}"));
        }
        // Every sample file in the directory is listed above, so none is forgotten.
        let on_disk = std::fs::read_dir(dir).unwrap().count();
        assert_eq!(on_disk, SAMPLES.len(), "a sample file is not in SAMPLES");
    }

    #[test]
    fn candump_capture_replays_through_a_can_protocol() {
        let capture = "(1700000000.25) can0 181#01020000\n(1700000000.5) can0 7FF#00\n";
        let points = replay_candump("canopen", capture.as_bytes()).unwrap();
        // Both frames are valid CANopen messages: 0x181 is TPDO1 from node 1, and 0x7FF
        // is a frame of another function code. Each keeps its capture timestamp.
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].value, tpt_protocol_tup::TupValue::Int(0x0201));
        assert_eq!(points[0].timestamp, 1_700_000_000_250);
        assert_eq!(points[1].timestamp, 1_700_000_000_500);
    }

    #[test]
    fn candump_replay_needs_a_can_protocol() {
        assert!(replay_candump("modbus", b"(1.0) can0 181#0102\n").is_err());
    }

    #[test]
    fn detection_ranks_the_right_protocol_first() {
        let hex = include_str!("../samples/canopen-tpdo1.hex");
        let bytes = decode_hex_file(hex.as_bytes()).unwrap();
        let ranking = detect::rank(&bytes);
        assert_eq!(ranking.first().map(|s| s.protocol), Some("canopen"));
    }

    #[test]
    fn raw_replay_passes_binary_through() {
        // The same frame as the hex sample, supplied as binary (`--replay-format raw`).
        let hex_text = include_str!("../samples/modbus-write-single-register-1.hex");
        let expected = decode_hex_file(hex_text.as_bytes()).unwrap();
        let got = replay_bytes("raw", expected.clone()).unwrap();
        assert_eq!(got, expected);
        let dp = plugins::replay_one("modbus", &got).unwrap();
        assert_eq!(dp.value, tpt_protocol_tup::TupValue::Int(0x0102));
    }

    #[test]
    fn replay_format_is_checked() {
        assert!(replay_bytes("hex", vec![0xFF, 0xFE]).is_err());
        assert!(replay_bytes("json", vec![]).is_err());
    }

    #[test]
    fn simulation_interval_is_honoured() {
        // Two points 200 ms apart should not arrive in under 200 ms in total.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut stream =
                plugins::simulate_stream("modbus", Duration::from_millis(200)).unwrap();
            let start = Instant::now();
            stream.next().await.unwrap();
            stream.next().await.unwrap();
            assert!(start.elapsed() >= Duration::from_millis(400));
        });
    }

    fn test_state() -> (Arc<ServerState>, broadcast::Sender<DataPoint>) {
        let (tx, _) = broadcast::channel::<DataPoint>(16);
        let policy = WsPolicy {
            token: None,
            allowed_origins: TAURI_ORIGINS.iter().map(|o| o.to_string()).collect(),
            check_host: true,
        };
        (Arc::new(ServerState::new(policy, tx.clone())), tx)
    }

    async fn start_server(state: Arc<ServerState>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(ws_server(listener, state));
        addr
    }

    #[tokio::test]
    async fn ws_origin_policy_end_to_end() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let (state, tx) = test_state();
        let addr = start_server(state).await;
        let url = format!("ws://{addr}/");

        // A Tauri origin is allowed: the handshake succeeds and points flow.
        let mut req = url.as_str().into_client_request().unwrap();
        req.headers_mut()
            .insert("origin", "tauri://localhost".parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("allowed origin connects");

        let dp = DataPoint::new(
            tpt_protocol_tup::SourceProtocol::Modbus,
            "dev-1".to_string(),
            "pt-1".to_string(),
            1_700_000_000_000,
            tpt_protocol_tup::TupValue::Float(1.5),
            tpt_protocol_tup::Quality::Good,
        );
        // The server subscribes after its handshake, so resend until one arrives.
        let text = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let _ = tx.send(dp.clone());
                if let Ok(Some(Ok(Message::Text(text)))) =
                    tokio::time::timeout(Duration::from_millis(50), ws.next()).await
                {
                    return text;
                }
            }
        })
        .await
        .expect("a data point arrives");
        assert!(text.starts_with('['), "{text}");
        assert!(text.contains("pt-1"), "{text}");

        // A foreign origin is refused with HTTP 403 before the upgrade.
        let mut bad = url.as_str().into_client_request().unwrap();
        bad.headers_mut()
            .insert("origin", "https://evil.example".parse().unwrap());
        match tokio_tungstenite::connect_async(bad).await {
            Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
                assert_eq!(resp.status(), StatusCode::FORBIDDEN);
            }
            other => panic!("expected HTTP 403, got {:?}", other.map(|_| ())),
        }
    }

    #[tokio::test]
    async fn healthz_reports_counters_without_data() {
        let (state, _tx) = test_state();
        let addr = start_server(Arc::clone(&state)).await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).await.unwrap();

        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(out.contains("\"status\":\"ok\""), "{out}");
        assert!(out.contains("\"rejected_handshakes\":0"), "{out}");
        assert!(out.contains("\"active\":1"), "{out}");
    }
}
