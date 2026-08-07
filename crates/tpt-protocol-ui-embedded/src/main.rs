// TPT Protocol embedded dashboard — Slint native UI.
//
// Connects to the TPT Protocol daemon's local WebSocket (bound to 127.0.0.1
// only) and renders a live, high-contrast data table of TUP data points.

use clap::Parser;
use slint::{Model, ModelRc, VecModel};

slint::include_modules!();

#[derive(Parser)]
#[command(version, about = "TPT Protocol embedded dashboard (Slint)")]
struct Cli {
    #[arg(long, default_value = "ws://127.0.0.1:9080")]
    url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let url = cli.url;

    let ui = MainWindow::new()?;

    let model = std::rc::Rc::new(VecModel::<DataRow>::from(vec![]));
    ui.set_rows(ModelRc::from(model.clone()));

    let (tx, mut rx) =
        tokio::sync::mpsc::unbounded_channel::<tpt_protocol_tup::DataPoint>();

    // WebSocket reader task.
    tokio::spawn(async move {
        loop {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    let (_, mut read) = ws_stream.split();
                    use futures_util::StreamExt;
                    while let Some(Ok(msg)) = read.next().await {
                        let text = match msg.to_text() {
                            Ok(t) => t,
                            Err(_) => continue,
                        };
                        for dp in parse_data_points(text) {
                            let _ = tx.send(dp);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("embedded UI WS connect error: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });

    slint::spawn_local(async move {
        while let Some(dp) = rx.recv().await {
            model.push(row_for(&dp));
            while model.row_count() > 200 {
                model.remove(0);
            }
        }
    })
    .unwrap();

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
