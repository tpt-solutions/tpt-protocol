//! Bytes piped into the daemon on standard input, for example:
//!
//! ```text
//! cat capture.bin | tpt-protocol-daemon --stdin --simulate modbus --replay-format raw
//! ```
//!
//! No hardware or network is needed, which makes this the simplest way to feed
//! a capture through the live pipeline.

use anyhow::Result;
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

use crate::transport::{supervise, ReaderLink, Transport};

pub async fn run_stdin(protocol: String, tx: broadcast::Sender<DataPoint>) -> Result<()> {
    supervise(StdinSource { done: false }, protocol, tx).await
}

/// Standard input is one link, and it ends at end of file.
struct StdinSource {
    done: bool,
}

impl Transport for StdinSource {
    type Link = ReaderLink<tokio::io::Stdin>;
    const CONCURRENT: bool = false;

    fn describe(&self) -> String {
        "stdin".into()
    }

    async fn next_link(&mut self) -> Result<Option<Self::Link>> {
        if std::mem::replace(&mut self.done, true) {
            return Ok(None);
        }
        Ok(Some(ReaderLink::new(tokio::io::stdin())))
    }
}
