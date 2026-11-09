//! MCAP recording (`--record-mcap FILE`, feature `mcap-record`).
//!
//! Each series (protocol, device, point) becomes one MCAP channel, named
//! `<protocol>/<device>/<point>`. Messages are the point as JSON (the same text the
//! WebSocket sends), with the point's timestamp as the MCAP log time in nanoseconds.
//! Files open in Foxglove and the `mcap` command-line tool.
//!
//! The file is written as points arrive, on a blocking thread. When the source
//! closes, the writer finishes the file (summary and index). A daemon stopped with
//! Ctrl-C does not reach that point, so the file is left without its summary; the
//! `mcap recover` command rebuilds it.
//!
//! Without the `mcap-record` feature, `--record-mcap` is refused with a clear error.

use std::path::Path;

use anyhow::Result;
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

/// Opens `path` for writing and records every point from `rx` until it closes.
/// The file is created here, so a bad path is a startup error.
#[cfg(feature = "mcap-record")]
pub fn spawn_mcap(path: &Path, rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    use anyhow::anyhow;
    let file = std::fs::File::create(path)
        .map_err(|e| anyhow!("cannot create {}: {e}", path.display()))?;
    let writer = mcap::Writer::with_options(file, write_options())
        .map_err(|e| anyhow!("cannot start an MCAP file: {e}"))?;
    tracing::info!("recording MCAP to {}", path.display());
    tokio::task::spawn_blocking(move || {
        if let Err(e) = write_points(writer, rx) {
            tracing::error!("MCAP recording stopped: {e:#}");
        }
    });
    Ok(())
}

/// Without the `mcap-record` feature there is no writer to start.
#[cfg(not(feature = "mcap-record"))]
pub fn spawn_mcap(_path: &Path, _rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    anyhow::bail!("--record-mcap needs a daemon built with the `mcap-record` feature")
}

/// The MCAP channel name for a point.
#[cfg(feature = "mcap-record")]
fn channel_name(dp: &DataPoint) -> String {
    format!(
        "{}/{}/{}",
        dp.source_protocol.as_str(),
        dp.device_id,
        dp.point_id
    )
}

/// Unchunked, uncompressed output. The default groups messages into chunks held in
/// memory until they fill (about 768 KB), so a daemon that stops would lose them.
/// Here each message is written as it arrives, and an interrupted file keeps every
/// message; only the summary is missing.
#[cfg(feature = "mcap-record")]
fn write_options() -> mcap::WriteOptions {
    mcap::WriteOptions::default().use_chunks(false)
}

/// Writes points until the channel closes, then finishes the file.
#[cfg(feature = "mcap-record")]
fn write_points<W: std::io::Write + std::io::Seek>(
    mut writer: mcap::Writer<W>,
    mut rx: broadcast::Receiver<DataPoint>,
) -> Result<()> {
    use std::collections::{BTreeMap, HashMap};
    use tokio::sync::broadcast::error::RecvError;

    let mut channels: HashMap<String, u16> = HashMap::new();
    loop {
        match rx.blocking_recv() {
            Ok(dp) => {
                let name = channel_name(&dp);
                let channel = match channels.get(&name) {
                    Some(id) => *id,
                    None => {
                        let id = writer.add_channel(0, &name, "json", &BTreeMap::new())?;
                        channels.insert(name, id);
                        id
                    }
                };
                let text = tpt_protocol_tup::json::to_json_string(&dp)?;
                let log_time = dp.timestamp.saturating_mul(1_000_000);
                writer.write_to_known_channel(
                    &mcap::records::MessageHeader {
                        channel_id: channel,
                        sequence: 0,
                        log_time,
                        publish_time: log_time,
                    },
                    text.as_bytes(),
                )?;
            }
            Err(RecvError::Lagged(n)) => {
                tracing::warn!("MCAP recording fell behind and skipped {n} points");
            }
            Err(RecvError::Closed) => break,
        }
    }
    writer.finish()?;
    Ok(())
}

#[cfg(all(test, feature = "mcap-record"))]
mod tests {
    use super::*;
    use tpt_protocol_tup::{Quality, SourceProtocol, TupValue};

    fn point(device: &str, ts: u64, value: i64) -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            device,
            "holding_register[1]",
            ts,
            TupValue::Int(value),
            Quality::Good,
        )
    }

    #[test]
    fn recorded_points_read_back_with_topics_and_log_times() {
        let (tx, rx) = broadcast::channel(16);
        let dir = std::env::temp_dir().join(format!("tpt-mcap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("points.mcap");
        let file = std::fs::File::create(&path).unwrap();
        let writer = mcap::Writer::with_options(file, write_options()).unwrap();

        tx.send(point("plc-1", 1_000, 10)).unwrap();
        tx.send(point("plc-2", 1_001, 20)).unwrap();
        tx.send(point("plc-1", 1_002, 30)).unwrap();
        drop(tx);
        write_points(writer, rx).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let messages: Vec<_> = mcap::MessageStream::new(&bytes)
            .unwrap()
            .map(|m| m.unwrap())
            .collect();
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages[0].channel.topic,
            "modbus/plc-1/holding_register[1]"
        );
        assert_eq!(
            messages[1].channel.topic,
            "modbus/plc-2/holding_register[1]"
        );
        assert_eq!(messages[0].log_time, 1_000 * 1_000_000);
        let body = String::from_utf8(messages[2].data.to_vec()).unwrap();
        assert!(body.contains("\"value\":30"), "{body}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn channel_names_follow_the_series() {
        assert_eq!(
            channel_name(&point("plc-1", 0, 0)),
            "modbus/plc-1/holding_register[1]"
        );
    }
}
