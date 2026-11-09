//! Records the TUP stream to a file as NDJSON, one data point per line. Each
//! line is the same JSON the WebSocket sends, so a recording can be read with
//! `jq`, pandas or any JSON-lines reader, and replayed by a script.

use std::path::Path;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tpt_protocol_tup::{json, DataPoint};

/// Appends every data point from `tx` to `path` until the channel closes.
/// A slow disk shows up as lagged messages, which are logged, not hidden.
pub async fn run(path: &Path, tx: broadcast::Sender<DataPoint>) -> Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("cannot open recording file {}", path.display()))?;
    let mut rx = tx.subscribe();
    tracing::info!("recording data points to {}", path.display());
    loop {
        match rx.recv().await {
            Ok(dp) => {
                let mut line = json::to_json_string(&dp)?;
                line.push('\n');
                file.write_all(line.as_bytes())
                    .await
                    .with_context(|| format!("cannot write {}", path.display()))?;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("recording fell behind and missed {n} points");
            }
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}
