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
use std::{
    collections::HashSet,
    io, io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use app::{AppMode, AppState};

// ── Session-scoped last-selection cache ──────────────────────────────────────
// No disk I/O; lives only as long as the process. A second TUI invocation in
// the same session (e.g. after --tui is called again from the same shell) can
// restore the previous pick.

static LAST_SELECTION: OnceLock<Mutex<Option<HashSet<PathBuf>>>> = OnceLock::new();

fn last_selection_store() -> &'static Mutex<Option<HashSet<PathBuf>>> {
    LAST_SELECTION.get_or_init(|| Mutex::new(None))
}

pub(super) fn save_last_selection(paths: &HashSet<PathBuf>) {
    if let Ok(mut guard) = last_selection_store().lock() {
        *guard = Some(paths.clone());
    }
}

pub(super) fn load_last_selection() -> Option<HashSet<PathBuf>> {
    last_selection_store()
        .lock()
        .ok()
        .and_then(|g| g.clone())
}

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
            if app.mode == AppMode::GitTree {
                app.sync_git_scroll(app.visible_height);
            } else {
                app.sync_search_scroll(app.visible_height);
            }
        }
        let file_count = app.selected_file_count();
        let loc_count = app.selected_loc();
        let mut rendered_height: u16 = 0;
        terminal.draw(|f| {
            rendered_height = render::draw(f, &mut app, &message, file_count, loc_count);
        })?;
        app.visible_height = rendered_height as usize;
        terminal.backend_mut().flush()?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => {
                    if let Some(paths) =
                        events::handle_key_event(&mut app, key_event, &mut message)
                    {
                        // Persist non-empty selections for this session so the
                        // user can restore them with `p` in the next invocation.
                        if !app.selected.is_empty() {
                            save_last_selection(&app.selected);
                        }
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