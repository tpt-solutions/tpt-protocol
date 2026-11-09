// TPT Protocol embedded dashboard — Slint native UI.
//
// Connects to the TPT Protocol daemon's local WebSocket (bound to 127.0.0.1
// only) and renders a live, high-contrast data table of TUP data points.

use std::time::Duration;

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use slint::{Model, ModelRc, VecModel};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

slint::include_modules!();

/// Rows kept in the table. Older rows are dropped.
const MAX_ROWS: usize = 200;
/// Events buffered between the socket task and the UI. When the UI falls behind,
/// the socket task waits, which pushes back on TCP instead of growing memory.
const CHANNEL_CAPACITY: usize = 256;
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

#[derive(Parser)]
#[command(version, about = "TPT Protocol embedded dashboard (Slint)")]
struct Cli {
    #[arg(long, default_value = "ws://127.0.0.1:9080")]
    url: String,
}

/// What the socket task tells the UI.
enum Event {
    Status { connected: bool, text: String },
    Data(tpt_protocol_tup::DataPoint),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let url = cli.url;

    let ui = MainWindow::new()?;

    let model = std::rc::Rc::new(VecModel::<DataRow>::from(vec![]));
    ui.set_rows(ModelRc::from(model.clone()));

    let (tx, mut rx) = mpsc::channel::<Event>(CHANNEL_CAPACITY);

    // WebSocket reader task. It reports status changes and forwards data points,
    // and it stops when the UI has gone away.
    tokio::spawn(async move {
        loop {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    if tx
                        .send(Event::Status {
                            connected: true,
                            text: format!("Connected to {url}"),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    let (mut write, mut read) = ws_stream.split();
                    while let Some(item) = read.next().await {
                        match item {
                            // Answer pings ourselves: the write half is in use here, and
                            // without a pong the daemon sees an unresponsive client.
                            Ok(Message::Ping(payload)) => {
                                if write.send(Message::Pong(payload)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(Message::Close(_)) | Err(_) => break,
                            Ok(msg) => {
                                let Ok(text) = msg.to_text() else { continue };
                                for dp in parse_data_points(text) {
                                    if tx.send(Event::Data(dp)).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    if tx
                        .send(Event::Status {
                            connected: false,
                            text: format!("Disconnected from {url}; retrying"),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(e) => {
                    if tx
                        .send(Event::Status {
                            connected: false,
                            text: format!("Cannot reach {url}: {e}"),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    });

    let ui_weak = ui.as_weak();
    slint::spawn_local(async move {
        while let Some(event) = rx.recv().await {
            match event {
                Event::Status { connected, text } => {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_connected(connected);
                        ui.set_status(text.into());
                    }
                }
                Event::Data(dp) => {
                    model.push(row_for(&dp));
                    while model.row_count() > MAX_ROWS {
                        model.remove(0);
                    }
                }
            }
        }
    })
    .map_err(|e| anyhow::anyhow!("cannot start the UI event task: {e}"))?;

    ui.run()?;
    Ok(())
}

fn parse_data_points(text: &str) -> Vec<tpt_protocol_tup::DataPoint> {
    let json: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let items = match json {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    let mut out = Vec::new();
    for v in &items {
        // The daemon's `{"type":"lagged"}` notice is not a data point.
        if v.get("type").and_then(|t| t.as_str()) == Some("lagged") {
            continue;
        }
        if let Ok(s) = serde_json::to_string(v) {
            if let Ok(dp) = tpt_protocol_tup::json::from_json_str(&s) {
                out.push(dp);
            }
        }
    }
    out
}

fn row_for(dp: &tpt_protocol_tup::DataPoint) -> DataRow {
    let total = dp.timestamp / 1000;
    let h = (total / 3600) % 24;
    let m = (total / 60) % 60;
    let s = total % 60;
    DataRow {
        time: slint::SharedString::from(format!("{h:02}:{m:02}:{s:02}")),
        protocol: slint::SharedString::from(format!("{}", dp.source_protocol)),
        device: slint::SharedString::from(dp.device_id.clone()),
        point: slint::SharedString::from(dp.point_id.clone()),
        value: slint::SharedString::from(format!("{}", dp.value)),
        quality: slint::SharedString::from(format!("{}", dp.quality)),
    }
}
