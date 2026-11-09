mod plugins;
mod transport;

use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use futures_util::stream::{Stream, StreamExt};
use futures_util::SinkExt;
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, OwnedSemaphorePermit, Semaphore};
use tokio_tungstenite::accept_hdr_async_with_config;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

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
    /// How `--replay` interprets the file: `hex` (ASCII hex text) or `raw` (binary).
    #[arg(long, default_value = "hex", value_parser = ["hex", "raw"])]
    replay_format: String,
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
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Config {
    simulate: Option<String>,
    port: Option<u16>,
    bind: Option<String>,
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
            eprintln!("rejected WebSocket handshake ({status}): {reason}");
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

async fn handle_connection(
    stream: TcpStream,
    tx: broadcast::Sender<DataPoint>,
    policy: Arc<WsPolicy>,
    _permit: OwnedSemaphorePermit,
) {
    #[allow(clippy::result_large_err)] // same tungstenite-imposed type as `on_handshake`
    let callback = move |req: &Request, resp: Response| on_handshake(req, resp, &policy);

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
            eprintln!("WebSocket handshake error: {e}");
            return;
        }
        Err(_) => {
            eprintln!("WebSocket handshake timed out");
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();
    let mut rx = tx.subscribe();

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
                    eprintln!("WebSocket read error: {e}");
                    break;
                }
            },
            outgoing = rx.recv() => match outgoing {
                Ok(dp) => {
                    let json = match json::to_json_string(&dp) {
                        Ok(j) => j,
                        Err(e) => {
                            eprintln!("JSON serialization error: {e}");
                            continue;
                        }
                    };
                    if write.send(Message::Text(format!("[{json}]"))).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(dropped)) => {
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

async fn ws_server(
    bind: &str,
    port: u16,
    tx: broadcast::Sender<DataPoint>,
    policy: Arc<WsPolicy>,
) -> Result<()> {
    let listener = TcpListener::bind((bind, port)).await?;
    let slots = Arc::new(Semaphore::new(MAX_CLIENTS));

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                eprintln!("accept error: {e}");
                tokio::time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };

        let permit = match Arc::clone(&slots).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                eprintln!("rejecting {peer}: connection limit ({MAX_CLIENTS}) reached");
                continue;
            }
        };

        let tx = tx.clone();
        let policy = Arc::clone(&policy);
        tokio::spawn(async move {
            handle_connection(stream, tx, policy, permit).await;
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

    let file_config = match &args.config {
        Some(path) => toml::from_str::<Config>(&tokio::fs::read_to_string(path).await?)?,
        None => Config::default(),
    };
    let settings = resolve_settings(args.simulate, args.port, args.bind, file_config);

    if let Some(ref replay_path) = args.replay {
        let protocol = settings.simulate.as_deref().ok_or_else(|| {
            anyhow!(
                "--replay needs --simulate <protocol> to choose the parser (see --list-protocols)"
            )
        })?;
        let raw = tokio::fs::read(replay_path).await?;
        let bytes = match args.replay_format.as_str() {
            "hex" => decode_hex_file(&raw)?,
            "raw" => raw,
            other => bail!("unknown --replay-format '{other}' (expected hex or raw)"),
        };
        let dp = plugins::replay_one(protocol, &bytes)
            .map_err(|e| anyhow!("replay parse failed: {e}"))?;
        println!("{}", json::to_json_string(&dp)?);
        return Ok(());
    }

    let protocol = match settings.simulate.clone() {
        Some(p) => p,
        None => {
            eprintln!("no --simulate given; defaulting to modbus");
            "modbus".to_string()
        }
    };
    let plugin = plugins::build_plugin(&protocol).map_err(|e| anyhow!("{e}"))?;
    let transport_endpoint = transport::Endpoint::from_flags(
        args.tcp_connect.clone(),
        args.tcp_listen.clone(),
        args.udp_bind.clone(),
        args.serial.clone(),
        args.baud,
        args.can.clone(),
    )?;

    if let Some(ref token) = args.token {
        if token.is_empty() {
            bail!("--token must not be empty");
        }
    }

    let bind = settings.bind;
    let port = settings.port;
    let loopback = is_loopback_bind(&bind);

    println!("TPT Protocol Daemon v0.1.0");
    println!("Simulating: {protocol}");

    if !loopback {
        eprintln!(
            "WARNING: binding to non-loopback address '{bind}'. Host-header checks are \
             disabled; expose the WebSocket API only on a trusted network and use --token."
        );
    }

    let mut allowed_origins: Vec<String> = TAURI_ORIGINS.iter().map(|o| o.to_string()).collect();
    allowed_origins.extend(args.allow_origin.iter().map(|o| normalize_origin(o)));
    let policy = Arc::new(WsPolicy {
        token: args.token,
        allowed_origins,
        check_host: loopback,
    });

    let (tx, _) = broadcast::channel::<DataPoint>(1024);

    match transport_endpoint {
        Some(endpoint) => {
            // Real bytes from a device replace the simulation.
            let real_tx = tx.clone();
            let real_protocol = protocol.clone();
            tokio::spawn(async move {
                if let Err(e) = transport::run(endpoint, real_protocol.clone(), real_tx).await {
                    eprintln!("{real_protocol}: transport stopped: {e}");
                }
            });
        }
        None => {
            let stream: Pin<Box<dyn Stream<Item = DataPoint> + Send>> = plugin.simulate();
            let sim_tx = tx.clone();
            let sim_name = protocol.clone();
            tokio::spawn(async move {
                let mut stream = stream;
                while let Some(dp) = stream.next().await {
                    let _ = sim_tx.send(dp);
                }
                eprintln!("simulation stream for {sim_name} ended");
            });
        }
    }

    let ws_tx = tx.clone();
    let bind_clone = bind.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_server(&bind_clone, port, ws_tx, policy).await {
            eprintln!("WebSocket server error: {e}");
        }
    });

    println!("WebSocket server listening on ws://{bind}:{port}");

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
        assert_eq!(plugins::list_protocols().len(), 19);
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
}
