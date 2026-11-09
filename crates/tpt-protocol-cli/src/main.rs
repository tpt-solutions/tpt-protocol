use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tpt_protocol_tup::{json, DataPoint, Quality};

const MAX_DATA_POINTS: usize = 100;
/// Messages buffered between the socket task and the UI. When the UI falls behind,
/// the socket task waits, which pushes back on TCP instead of growing memory.
const CHANNEL_CAPACITY: usize = 256;
const TICK_RATE: Duration = Duration::from_millis(100);

/// Truncate `s` to at most `max` bytes, never splitting a UTF-8 character, and
/// append `..` when truncation occurs. Avoids the panic that `&s[..max]` would
/// cause on a multibyte character sitting on the boundary.
fn truncate_to(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}..", &s[..end])
}

#[derive(Parser)]
#[command(version, about = "TPT Protocol TUI Dashboard")]
struct Cli {
    #[arg(long, default_value = "ws://127.0.0.1:9080")]
    url: String,
    #[arg(long)]
    daemon: Option<String>,
}

enum WsMessage {
    Data(Vec<DataPoint>),
    Connected,
    Disconnected,
    Error(String),
}

struct App {
    data_points: VecDeque<DataPoint>,
    connected: bool,
    data_count: u64,
    error: Option<String>,
    connected_since: Option<Instant>,
    scroll_offset: usize,
}

impl App {
    fn new() -> Self {
        Self {
            data_points: VecDeque::with_capacity(MAX_DATA_POINTS),
            connected: false,
            data_count: 0,
            error: None,
            connected_since: None,
            scroll_offset: 0,
        }
    }

    fn add_data(&mut self, points: Vec<DataPoint>) {
        for dp in points {
            self.data_points.push_back(dp);
            self.data_count += 1;
        }
        while self.data_points.len() > MAX_DATA_POINTS {
            self.data_points.pop_front();
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }
}

fn format_timestamp(ts: u64) -> String {
    let total_secs = ts / 1000;
    let millis = ts % 1000;
    let h = (total_secs / 3600) % 24;
    let m = (total_secs / 60) % 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
}

fn format_duration(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total / 60) % 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn quality_color(q: &Quality) -> Color {
    match q {
        Quality::Good => Color::Green,
        Quality::Bad => Color::Red,
        Quality::Uncertain => Color::Yellow,
        Quality::Stale => Color::DarkGray,
        // `Quality` is non-exhaustive: an unknown future quality stays visible.
        _ => Color::White,
    }
}

fn parse_data_points(text: &str) -> Result<Vec<DataPoint>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("JSON error: {e}"))?;
    match json {
        serde_json::Value::Array(arr) => {
            let mut points = Vec::with_capacity(arr.len());
            for v in &arr {
                let s = serde_json::to_string(v).map_err(|e| format!("serialize error: {e}"))?;
                points.push(json::from_json_str(&s)?);
            }
            Ok(points)
        }
        serde_json::Value::Object(ref obj)
            if obj.get("type").and_then(|t| t.as_str()) == Some("lagged") =>
        {
            let dropped = obj.get("dropped").and_then(|d| d.as_u64()).unwrap_or(0);
            Err(format!(
                "daemon dropped {dropped} messages (client too slow)"
            ))
        }
        _ => {
            let dp = json::from_json_str(text)?;
            Ok(vec![dp])
        }
    }
}

async fn ws_task(url: String, tx: mpsc::Sender<WsMessage>) {
    loop {
        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                let _ = tx.send(WsMessage::Connected).await;
                let (_, mut read) = ws_stream.split();
                loop {
                    match read.next().await {
                        Some(Ok(msg)) => {
                            if let Ok(text) = msg.to_text() {
                                match parse_data_points(text) {
                                    Ok(points) => {
                                        let _ = tx.send(WsMessage::Data(points)).await;
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsMessage::Error(e)).await;
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            let _ = tx.send(WsMessage::Error(e.to_string())).await;
                            break;
                        }
                        None => break,
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(WsMessage::Error(e.to_string())).await;
            }
        }
        let _ = tx.send(WsMessage::Disconnected).await;
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let title_area = chunks[0];
    let stats_area = chunks[1];
    let table_area = chunks[2];
    let status_area = chunks[3];

    let status_color = if app.connected {
        Color::Green
    } else {
        Color::Red
    };
    let status_text = if app.connected {
        " CONNECTED "
    } else {
        " DISCONNECTED "
    };
    let title = Line::from(vec![
        Span::styled(" TPT Protocol CLI ", Style::new().bold().white()),
        Span::styled(status_text, Style::new().bg(status_color).black().bold()),
    ]);
    frame.render_widget(Paragraph::new(title), title_area);

    let connected_time = app
        .connected_since
        .map(|t| format_duration(t.elapsed()))
        .unwrap_or_default();
    let stats = Line::from(vec![
        Span::raw(format!(" Messages: {}", app.data_count)),
        Span::raw(" | "),
        Span::raw(format!("Uptime: {}", connected_time)),
    ]);
    frame.render_widget(Paragraph::new(stats).style(Style::new().dim()), stats_area);

    let header_cells = ["Time", "Protocol", "Device", "Point", "Value", "Quality"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::new().bold().white().bg(Color::DarkGray)));
    let header = Row::new(header_cells);

    let max_visible = table_area.height.saturating_sub(3).max(1) as usize;
    if app.scroll_offset + max_visible > app.data_points.len()
        && app.data_points.len() > max_visible
    {
        app.scroll_offset = app.data_points.len().saturating_sub(max_visible);
    }

    let rows: Vec<Row> = app
        .data_points
        .iter()
        .map(|dp| {
            let q_style = Style::new().fg(quality_color(&dp.quality));
            let device = if dp.device_id.len() > 18 {
                truncate_to(&dp.device_id, 18)
            } else {
                dp.device_id.clone()
            };
            let point = if dp.point_id.len() > 18 {
                truncate_to(&dp.point_id, 18)
            } else {
                dp.point_id.clone()
            };
            Row::new(vec![
                Cell::from(format_timestamp(dp.timestamp)),
                Cell::from(format!("{}", dp.source_protocol)),
                Cell::from(device),
                Cell::from(point),
                Cell::from(format!("{}", dp.value)),
                Cell::from(format!("{}", dp.quality)).style(q_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(15),
        Constraint::Length(12),
        Constraint::Length(20),
        Constraint::Length(20),
        Constraint::Length(15),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::new().reversed());

    let mut table_state = TableState::new().with_offset(app.scroll_offset);
    frame.render_stateful_widget(table, table_area, &mut table_state);

    let status = Line::from(vec![
        Span::styled(" [Q]", Style::new().bold()),
        Span::raw("uit "),
        Span::styled("[↑↓]", Style::new().bold()),
        Span::raw(" Scroll "),
        Span::styled("[Esc]", Style::new().bold()),
        Span::raw(" Quit"),
        if let Some(ref err) = app.error {
            Span::styled(format!("  Error: {err}"), Style::new().fg(Color::Red))
        } else {
            Span::raw("")
        },
    ]);
    frame.render_widget(
        Paragraph::new(status).style(Style::new().dim()),
        status_area,
    );
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let url = match cli.daemon {
        Some(ref d) => format!("ws://{d}"),
        None => cli.url,
    };

    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.spawn(ws_task(url, tx));

    let mut terminal = ratatui::init();
    // Restore the terminal on every path, including errors from the loop.
    let result = run_ui(&mut terminal, &mut rx);
    ratatui::restore();
    result
}

fn run_ui(
    terminal: &mut ratatui::DefaultTerminal,
    rx: &mut mpsc::Receiver<WsMessage>,
) -> Result<()> {
    let mut app = App::new();
    let mut last_tick = Instant::now();

    loop {
        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
        if event::poll(timeout).context("failed to poll events")? {
            if let Event::Key(key) = event::read().context("failed to read event")? {
                // Windows reports key release and repeat as well as press. Acting on
                // all of them scrolls twice per keystroke.
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => break,
                        KeyCode::Up => app.scroll_up(),
                        KeyCode::Down => app.scroll_down(),
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= TICK_RATE {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WsMessage::Data(points) => app.add_data(points),
                    WsMessage::Connected => {
                        app.connected = true;
                        app.connected_since = Some(Instant::now());
                        app.error = None;
                    }
                    WsMessage::Disconnected => {
                        app.connected = false;
                        app.connected_since = None;
                    }
                    WsMessage::Error(e) => app.error = Some(e),
                }
            }

            terminal
                .draw(|f| ui(f, &mut app))
                .context("failed to draw")?;
            last_tick = Instant::now();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundary() {
        // A multibyte char ("é", 2 bytes) straddling the cut must not panic.
        let s = "abcéfghijklmnopqrstuvwxyz0123456789";
        let t = truncate_to(s, 4);
        assert!(s.starts_with(t.trim_end_matches("..")));
        assert!(t.ends_with(".."));

        // A long ASCII string truncates and appends the marker.
        let long = "x".repeat(40);
        let t2 = truncate_to(&long, 18);
        assert!(t2.ends_with(".."));
        assert_eq!(t2.len(), 20);
    }

    #[test]
    fn truncate_short_is_untouched() {
        assert_eq!(truncate_to("abc", 18), "abc");
    }

    #[test]
    fn lagged_notice_is_reported_not_parsed_as_points() {
        let err = parse_data_points(r#"{"type":"lagged","dropped":7}"#).unwrap_err();
        assert!(err.contains("dropped 7"), "got: {err}");
    }
}
