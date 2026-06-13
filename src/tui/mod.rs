mod app;
mod events;
mod render;
mod theme;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event},
    execute as crossterm_execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, io::Write, time::Duration};

use app::{AppMode, AppState};

pub struct TuiOutcome {
    pub paths: Vec<String>,
    pub relative: bool,
    pub no_path: bool,
}

pub fn run_tui(relative: bool, no_path: bool) -> Result<TuiOutcome> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm_execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = tui_main(&mut terminal, relative, no_path);

    disable_raw_mode()?;
    crossterm_execute!(
        io::stdout(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    res
}

fn tui_main(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    relative: bool,
    no_path: bool,
) -> Result<TuiOutcome> {
    let mut app = AppState::new(relative, no_path).context("Failed to read current directory")?;
    let mut message = String::new();

    loop {
        // Search mode manages its own cursor scrolling; tree widget self-manages.
        if app.mode != AppMode::Normal {
            app.sync_search_scroll(app.visible_height);
        }
        let file_count = app.selected_file_count();
        let mut rendered_height: u16 = 0;
        terminal.draw(|f| {
            rendered_height = render::draw(f, &mut app, &message, file_count);
        })?;
        app.visible_height = rendered_height as usize;
        terminal.backend_mut().flush()?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => {
                    if let Some(paths) =
                        events::handle_key_event(&mut app, key_event, &mut message)
                    {
                        return Ok(TuiOutcome {
                            paths,
                            relative: app.relative,
                            no_path: app.no_path,
                        });
                    }
                }
                Event::Mouse(mouse_event) => {
                    events::handle_mouse_event(&mut app, mouse_event, &mut message);
                }
                _ => {}
            }
        }
    }
}
