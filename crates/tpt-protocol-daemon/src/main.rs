mod plugins;

use std::pin::Pin;

use anyhow::Result;
use clap::Parser;
use futures_util::stream::{Stream, StreamExt};
use futures_util::SinkExt;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use tpt_protocol_tup::{DataPoint, TupValue};

#[derive(Parser)]
#[command(name = "tpt-protocol-daemon", version = "0.1.0")]
struct Args {
    /// Simulate this protocol instead of a real device. See `--list-protocols`.
    #[arg(long)]
    simulate: Option<String>,
    #[arg(long)]
    config: Option<String>,
    #[arg(long, default_value = "9080")]
    port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
    /// Print the list of supported protocols and exit.
    #[arg(long)]
    list_protocols: bool,
    /// Replay raw bytes from a file through the chosen protocol's parser instead
    /// of running the synthetic simulation. Hex strings are decoded; otherwise
    /// the file is read as raw bytes.
    #[arg(long)]
    replay: Option<String>,
}

#[derive(Deserialize)]
struct Config {
    simulate: Option<String>,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_bind")]
    bind: String,
}

fn default_port() -> u16 {
    9080
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn datapoint_to_json(dp: &DataPoint) -> Result<String, serde_json::Error> {
    let value = match &dp.value {
        TupValue::Int(v) => serde_json::json!(v),
        TupValue::Float(v) => serde_json::json!(v),
        TupValue::Bool(v) => serde_json::json!(v),
        TupValue::Text(v) => serde_json::json!(v),
        TupValue::Bytes(v) => serde_json::Value::String(format!("<{} bytes>", v.len())),
    };

    let mut obj = serde_json::json!({
        "schema_version": dp.schema_version(),
        "source_protocol": dp.source_protocol.as_str(),
        "device_id": dp.device_id,
        "point_id": dp.point_id,
        "timestamp": dp.timestamp,
        "value": value,
        "quality": dp.quality.as_str(),
    });

    if let Some(ref unit) = dp.unit {
        obj["unit"] = serde_json::json!(unit);
    }

    serde_json::to_string(&obj)
}

async fn handle_connection(stream: tokio::net::TcpStream, tx: broadcast::Sender<DataPoint>) {
    let ws_stream = match accept_async(stream).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("WebSocket handshake error: {e}");
            return;
        }
    };

    let (mut write, _read) = ws_stream.split();
    let mut rx = tx.subscribe();

    loop {
        match rx.recv().await {
            Ok(dp) => {
                let json = match datapoint_to_json(&dp) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("JSON serialization error: {e}");
                        continue;
                    }
                };
                let msg = format!("[{}]", json);
                if write.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("client lagged by {n} messages");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn ws_server(bind: &str, port: u16, tx: broadcast::Sender<DataPoint>) -> Result<()> {
    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr).await?;

    while let Ok((stream, _peer)) = listener.accept().await {
        let tx = tx.clone();
        tokio::spawn(async move {
            handle_connection(stream, tx).await;
        });
    }

    Ok(())
}

/// Returns true if `bind` resolves to a loopback address. We warn (rather than
/// refuse) when the daemon is asked to bind to a non-loopback address, since
/// the WebSocket API currently has no authentication.
fn is_loopback_bind(bind: &str) -> bool {
    if bind == "localhost" || bind == "::1" || bind == "0:0:0:0:0:0:0:1" {
        return true;
    }
    // Strip a bracketed IPv6 literal like [::1].
    let candidate = bind.trim_matches(|c| c == '[' || c == ']');
    if let Ok(ip) = candidate.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    false
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

    let (simulate, port, bind) = if let Some(ref config_path) = args.config {
        let config_str = tokio::fs::read_to_string(config_path).await?;
        let config: Config = toml::from_str(&config_str)?;
        (config.simulate.or(args.simulate), config.port, config.bind)
    } else {
        (args.simulate, args.port, args.bind)
    };

    let protocol = simulate.as_deref().unwrap_or("modbus");

    if let Some(ref replay_path) = args.replay {
        let raw = tokio::fs::read_to_string(replay_path).await?;
        let bytes = if let Some(hex_str) = raw.trim().strip_prefix("0x").or_else(|| raw.trim().strip_prefix("0X")) {
            hex::decode(hex_str.trim()).map_err(|e| anyhow::anyhow!("invalid hex in replay file: {e}"))?
        } else if raw.trim().chars().all(|c| c.is_ascii_hexdigit()) && raw.trim().len() % 2 == 0 {
            hex::decode(raw.trim()).map_err(|e| anyhow::anyhow!("invalid hex in replay file: {e}"))?
        } else {
            tokio::fs::read(replay_path).await?
        };
        let dp = plugins::replay_one(protocol, &bytes)
            .map_err(|e| anyhow::anyhow!("replay parse failed: {e}"))?;
        println!("{}", datapoint_to_json(&dp)?);
        return Ok(());
    }

    println!("TPT Protocol Daemon v0.1.0");
    println!("Simulating: {protocol}");

    if !is_loopback_bind(&bind) {
        eprintln!(
            "WARNING: binding to non-loopback address '{bind}'. The daemon's WebSocket \
             API has no authentication; expose it only on a trusted network."
        );
    }

    let (tx, _) = broadcast::channel::<DataPoint>(1024);

    let plugin = plugins::build_plugin(protocol)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let stream: Pin<Box<dyn Stream<Item = DataPoint> + Send>> = plugin.simulate();
    let sim_tx = tx.clone();
    tokio::spawn(async move {
        let mut stream = stream;
        while let Some(dp) = stream.next().await {
            let _ = sim_tx.send(dp);
        }
    });

    let ws_tx = tx.clone();
    let bind_clone = bind.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_server(&bind_clone, port, ws_tx).await {
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
            assert!(plugins::build_plugin(name).is_ok(), "failed to build {name}");
        }
        assert!(plugins::build_plugin("not-a-protocol").is_err());
    }
}
