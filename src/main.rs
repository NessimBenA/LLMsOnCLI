use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::prelude::*;
use std::io;

mod api;
mod app;
mod commands;
mod config;
mod history;
mod ui;

use crate::app::App;
use crate::config::Config;
use crate::history::History;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let history = History::open_default()?;
    let mut app = App::new(config, history);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut events = EventStream::new();
    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            event = events.next() => match event {
                Some(event) => app.handle_input(event?),
                None => break,
            },
            event = app.next_stream_event() => app.handle_stream_event(event),
        }
    }
    Ok(())
}
