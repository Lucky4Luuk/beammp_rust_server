use std::sync::Arc;
use std::io::stdout;

use tokio::task::JoinHandle;

use tui::{Terminal, Frame};
use tui::backend::{Backend, CrosstermBackend};
use tui::layout::*;
use tui::style::*;
use tui::widgets::*;

use crate::config::Config;

mod input;
mod communicator;

use input::*;
use communicator::*;

pub fn start(config: Arc<Config>) -> anyhow::Result<()> {
    std::panic::set_hook(Box::new(|panic_info| {
		let _ = crossterm::terminal::disable_raw_mode();
		better_panic::Settings::auto().create_panic_handler()(panic_info);
        std::process::exit(1);
	}));

    let stdout = stdout();
    crossterm::terminal::enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    terminal.hide_cursor()?;

    let mut communicator = Communicator::new();

    let tick_rate = std::time::Duration::from_millis(200);
    let events = Events::new(tick_rate);

    loop {
        terminal.draw(|rect| draw(rect))?;

        // Handle inputs
        let result = match events.next()? {
            InputEvent::Input(key) => communicator.handle_input(key),
            InputEvent::Tick => communicator.tick(),
        };
        if result == UpdateResult::Exit {
            break;
        }
    }

    terminal.clear()?;
    terminal.show_cursor()?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

fn draw<B: Backend>(rect: &mut Frame<B>) {
    let size = rect.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10)].as_ref())
        .split(size);

    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(chunks[1]);

    let title = Paragraph::new("BeamMP rust server v0.1")
        .style(Style::default().fg(Color::LightCyan))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .border_type(BorderType::Plain),
        );
    rect.render_widget(title, chunks[0]);
    draw_logger(rect, body_chunks[1]);
}

fn draw_logger<B: Backend>(rect: &mut Frame<B>, chunk: Rect) {
    let logger_widget = tui_logger::TuiLoggerWidget::default()
        .block(
            Block::default()
                .title("Logger")
                .borders(Borders::ALL)
        )
        .output_separator('|')
        .output_timestamp(Some("%F %H:%M:%S%.3f".to_string()))
        .style(Style::default().fg(Color::White).bg(Color::Black));
    rect.render_widget(logger_widget, chunk);
}
