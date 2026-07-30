use std::pin::Pin;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures_util::stream::{Stream, StreamExt};
use futures_util::SinkExt;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

trait ProtocolPlugin: Send + 'static {
    fn simulate(self: Box<Self>) -> Pin<Box<dyn Stream<Item = DataPoint> + Send>>;
}

struct ModbusPlugin;

impl ProtocolPlugin for ModbusPlugin {
    fn simulate(self: Box<Self>) -> Pin<Box<dyn Stream<Item = DataPoint> + Send>> {
        Box::pin(futures_util::stream::unfold(0u64, |tick| async move {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let dp = match tick % 3 {
                0 => DataPoint::new(
                    SourceProtocol::Modbus,
                    "modbus-sim-01",
                    "holding-register:40001",
                    ts,
                    TupValue::Float(70.0 + ((tick as f64 * 0.2).sin() * 10.0)),
                    Quality::Good,
                )
                .with_unit("degF"),
                1 => DataPoint::new(
                    SourceProtocol::Modbus,
                    "modbus-sim-01",
                    "holding-register:40002",
                    ts,
                    TupValue::Float(100.0 + (tick as f64 % 100.0)),
                    Quality::Good,
                )
                .with_unit("kPa"),
                _ => DataPoint::new(
                    SourceProtocol::Modbus,
                    "modbus-sim-01",
                    "coil:00001",
                    ts,
                    TupValue::Bool(tick % 2 == 0),
                    Quality::Good,
                ),
            };

            Some((dp, tick + 1))
        }))
    }
}

#[derive(Parser)]
#[command(name = "tpt-protocol-daemon", version = "0.1.0")]
struct Args {
    #[arg(long)]
    simulate: Option<String>,
    #[arg(long)]
    config: Option<String>,
    #[arg(long, default_value = "9080")]
    port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (simulate, port, bind) = if let Some(ref config_path) = args.config {
        let config_str = tokio::fs::read_to_string(config_path).await?;
        let config: Config = toml::from_str(&config_str)?;
        (config.simulate.or(args.simulate), config.port, config.bind)
    } else {
        (args.simulate, args.port, args.bind)
    };

    let protocol = simulate.as_deref().unwrap_or("modbus");

    println!("TPT Protocol Daemon v0.1.0");
    println!("Simulating: {protocol}");

    let (tx, _) = broadcast::channel::<DataPoint>(1024);

    match protocol {
        "modbus" => {
            let plugin = ModbusPlugin;
            let stream = Box::new(plugin).simulate();
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut stream = stream;
                while let Some(dp) = stream.next().await {
                    let _ = tx.send(dp);
                }
            });
        }
        other => anyhow::bail!("unknown protocol: {other}"),
    }

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
