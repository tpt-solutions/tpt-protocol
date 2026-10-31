//! Tauri application library for the TPT Protocol fleet dashboard.
//!
//! The frontend (SvelteKit) connects to the TPT Protocol daemon's WebSocket
//! directly from the renderer via a small relay. This library sets up the
//! Tauri window and exposes a command to fetch the configured daemon URL.

use serde::Serialize;

/// Configuration the webview reads at startup.
#[derive(Serialize)]
pub struct DaemonConfig {
    /// WebSocket URL the daemon is listening on (loopback by default).
    pub url: String,
}

/// Returns the daemon WebSocket URL the dashboard should connect to.
#[tauri::command]
fn daemon_url() -> DaemonConfig {
    DaemonConfig {
        url: std::env::var("TPT_DAEMON_URL").unwrap_or_else(|_| "ws://127.0.0.1:9080".to_string()),
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![daemon_url])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
