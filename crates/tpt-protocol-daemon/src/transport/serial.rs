//! Serial ports. Needs the `serial` feature; without it the endpoint fails with
//! an explanation.

use anyhow::Result;
use tokio::sync::broadcast;
use tpt_protocol_tup::DataPoint;

#[cfg(feature = "serial")]
pub async fn run_serial(
    path: String,
    baud: u32,
    protocol: String,
    tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    crate::transport::supervise(SerialPort { path, baud }, protocol, tx).await
}

/// Reopens the port whenever it fails or closes.
#[cfg(feature = "serial")]
struct SerialPort {
    path: String,
    baud: u32,
}

#[cfg(feature = "serial")]
impl crate::transport::Transport for SerialPort {
    type Link = crate::transport::ReaderLink<tokio_serial::SerialStream>;
    const CONCURRENT: bool = false;

    fn describe(&self) -> String {
        format!("{} at {} baud", self.path, self.baud)
    }

    async fn next_link(&mut self) -> Result<Option<Self::Link>> {
        use anyhow::Context;
        use tokio_serial::SerialPortBuilderExt;
        let port = tokio_serial::new(&self.path, self.baud)
            .open_native_async()
            .with_context(|| format!("open {} failed", self.path))?;
        Ok(Some(crate::transport::ReaderLink::new(port)))
    }
}

#[cfg(not(feature = "serial"))]
pub async fn run_serial(
    _path: String,
    _baud: u32,
    _protocol: String,
    _tx: broadcast::Sender<DataPoint>,
) -> Result<()> {
    anyhow::bail!("serial ports need the daemon to be built with the `serial` feature")
}
