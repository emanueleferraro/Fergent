//! The interactive Ratatui shell with a placeholder panel.
//!
//! Phase 0 renders a minimal shell: a header with the live kernel health, a
//! placeholder "Workspace" panel, and a streaming event log. Later phases
//! replace the placeholder with the real workspace (`ARCHITECTURE.md` §5.2).

use std::io::{self, Stdout};

use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use fergents_api::SocketPath;
use fergents_protocol::HealthSnapshot;
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::client;

/// Maximum number of event lines retained in the log panel.
const MAX_EVENT_LINES: usize = 200;

/// Run the TUI until the user quits.
pub async fn run(socket: &SocketPath) -> anyhow::Result<()> {
    let mut client = client::connect_with_retry(socket).await?;
    let _ = client.subscribe(&[]).await;
    let health: Option<HealthSnapshot> = client.call_typed("system.health", &()).await.ok();

    let mut terminal = init_terminal()?;
    let _guard = TuiGuard;
    let mut keys = EventStream::new();

    let mut events: Vec<String> = Vec::new();
    let mut should_quit = false;

    while !should_quit {
        terminal.draw(|frame| draw(frame, health.as_ref(), &events))?;
        tokio::select! {
            key = keys.next() => {
                if let Some(Ok(CrosstermEvent::Key(key))) = key {
                    if key.kind == KeyEventKind::Press
                        && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                    {
                        should_quit = true;
                    }
                }
            }
            event = client.next_event() => {
                match event {
                    Some(event) => {
                        events.push(format!("{:<26} {:<28} {}", event.id, event.kind, event.source));
                        if events.len() > MAX_EVENT_LINES {
                            events.remove(0);
                        }
                    }
                    None => should_quit = true,
                }
            }
        }
    }
    Ok(())
}

/// Render one frame: header, body (placeholder + event log), footer.
pub fn draw(frame: &mut Frame, health: Option<&HealthSnapshot>, events: &[String]) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    let status = match health {
        Some(health) => format!(
            "status={}  storage={}  uptime={}ms  kernel={}",
            health.status, health.storage, health.uptime_ms, health.kernel_version
        ),
        None => "status=unknown".to_string(),
    };
    let header =
        Paragraph::new(status).block(Block::default().borders(Borders::ALL).title("fergentsd"));
    frame.render_widget(header, chunks[0]);

    let body = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    let placeholder = Paragraph::new(Text::raw(
        "Phase 0 placeholder panel\n\nBrains, agents, workflows, and chat\narrive in later phases.",
    ))
    .block(Block::default().borders(Borders::ALL).title("Workspace"))
    .wrap(Wrap { trim: false });
    frame.render_widget(placeholder, body[0]);

    let log = Paragraph::new(events.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Events"))
        .wrap(Wrap { trim: false });
    frame.render_widget(log, body[1]);

    let footer = Paragraph::new("q / Esc: quit").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);
}

/// Enter raw mode + the alternate screen and build the terminal.
fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;
    Ok(terminal)
}

/// Restores the terminal when dropped, even on error paths.
struct TuiGuard;

impl Drop for TuiGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
