use std::sync::Arc;
use std::io::stdout;

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

static mut CONFIG: Option<Arc<Config>> = None;
fn get_config() -> &'static Config {
    unsafe { CONFIG.as_ref().unwrap() }
}

pub fn start(config: Arc<Config>) -> anyhow::Result<()> {
    std::panic::set_hook(Box::new(|panic_info| {
		let _ = crossterm::terminal::disable_raw_mode();
		better_panic::Settings::auto().create_panic_handler()(panic_info);
        std::process::exit(1);
	}));

    unsafe { CONFIG = Some(config); }

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
    draw_communicator(rect, body_chunks[0]);
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

fn draw_communicator<B: Backend>(rect: &mut Frame<B>, chunk: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(40), Constraint::Percentage(30)].as_ref())
        .split(chunk);
    let config = get_config();

    let info_text = format!("Port: {}\nMap: {}", config.network.port.unwrap_or(48900), config.game.map);
    let info = Paragraph::new(info_text)
        .block(
            Block::default()
                .title("Server Info")
                .borders(Borders::ALL)
        );
    rect.render_widget(info, chunks[0]);

    let players_items = [ListItem::new("[0] luuk-bepis"), ListItem::new("[1] luuk-bepis2"), ListItem::new("[2] youll-never-guess")];
    let players_list = List::new(players_items)
        .block(
            Block::default()
                .title("Player List")
                .borders(Borders::ALL)
        );
    rect.render_widget(players_list, chunks[2]);
}
