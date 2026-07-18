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
};

use app::{AppMode, AppState};

// ── Cross-invocation last-selection cache ────────────────────────────────────
// Stored in $XDG_RUNTIME_DIR (a per-user tmpfs, wiped on logout) so it
// survives across cxt invocations in the same terminal session without leaving
// any permanent file anywhere on the system.
// Falls back to $TMPDIR / /tmp when XDG_RUNTIME_DIR is not set.

fn last_selection_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TMPDIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.join("cxt_last_selection")
}

pub(super) fn save_last_selection(paths: &HashSet<PathBuf>) {
    let content: String = paths
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(last_selection_path(), content);
}

pub(super) fn load_last_selection() -> Option<HashSet<PathBuf>> {
    let content = std::fs::read_to_string(last_selection_path()).ok()?;
    let paths: HashSet<PathBuf> = content
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();
    if paths.is_empty() { None } else { Some(paths) }
}

pub struct TuiOutcome {
    pub paths: Vec<String>,
    pub relative: bool,
    pub no_path: bool,
    pub aider: bool,
}

pub fn run_tui(relative: bool, no_path: bool, aider: bool) -> Result<TuiOutcome> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm_execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = tui_main(&mut terminal, relative, no_path, aider);

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
    aider: bool,
) -> Result<TuiOutcome> {
    let mut app = AppState::new(relative, no_path, aider).context("Failed to read current directory")?;
    let mut message = String::new();
    let mut needs_redraw = true;
    let mut rendered_height: u16 = 0;

    loop {
        if needs_redraw {
            // Search mode manages its own cursor scrolling; tree widget self-manages.
            if app.mode != AppMode::Normal {
                match app.mode {
                    AppMode::GitTree => {
                        app.sync_git_scroll(app.visible_height);
                    }
                    AppMode::GitStatus => {
                        app.sync_git_status_diff_scroll(app.visible_height);
                    }
                    _ => {
                        app.sync_search_scroll(app.visible_height);
                    }
                }
            }
            let file_count = app.selected_file_count();
            let loc_count = app.selected_loc();
            terminal.draw(|f| {
                rendered_height = render::draw(f, &mut app, &message, file_count, loc_count);
            })?;
            app.visible_height = rendered_height as usize;
            terminal.backend_mut().flush()?;
            needs_redraw = false;
        }

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
                        aider: app.aider,
                    });
                }
                needs_redraw = true;
            }
            Event::Mouse(mouse_event) => {
                events::handle_mouse_event(&mut app, mouse_event, &mut message);
                needs_redraw = true;
            }
            Event::Resize(_, _) => {
                needs_redraw = true;
            }
            _ => {}
        }
    }
}
