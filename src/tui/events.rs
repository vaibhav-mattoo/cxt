use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::tui::app::{AppMode, AppState};

pub fn handle_key_event(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    if key_event.kind != KeyEventKind::Press {
        return None;
    }
    match app.mode {
        AppMode::SearchFocused => handle_search_focused(app, key_event),
        AppMode::SearchNavigating => handle_search_navigating(app, key_event, message),
        AppMode::Normal => handle_normal(app, key_event, message),
    }
}

fn handle_search_focused(app: &mut AppState, key_event: KeyEvent) -> Option<Vec<String>> {
    match key_event.code {
        KeyCode::Esc => {
            app.exit_search();
        }
        KeyCode::Backspace => {
            app.pop_search_char();
        }
        KeyCode::Enter | KeyCode::Down | KeyCode::Up => {
            app.mode = AppMode::SearchNavigating;
        }
        KeyCode::Char(c) => {
            app.push_search_char(c);
        }
        _ => {}
    }
    None
}

fn handle_search_navigating(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    match key_event.code {
        KeyCode::Char('q') => return Some(vec![]),
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(vec![])
        }
        KeyCode::Char('c') => {
            if app.selected.is_empty() {
                *message = "No files or directories selected!".to_string();
            } else {
                return Some(app.collect_selected_paths());
            }
        }
        KeyCode::Char('/') => {
            app.mode = AppMode::SearchFocused;
        }
        KeyCode::Esc => {
            app.exit_search();
        }
        KeyCode::Enter => {
            if let Some(result) = app.search_results.get(app.search_cursor) {
                if result.is_dir {
                    let path = result.path.clone();
                    app.navigate_to_dir(path);
                } else {
                    let path = result.path.clone();
                    let is_dir = result.is_dir;
                    app.toggle_selection(path, is_dir);
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') if app.search_cursor > 0 => {
            app.search_cursor -= 1;
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.search_cursor + 1 < app.search_results.len() =>
        {
            app.search_cursor += 1;
        }
        KeyCode::Char(' ') => {
            if let Some(result) = app.search_results.get(app.search_cursor) {
                let path = result.path.clone();
                let is_dir = result.is_dir;
                app.toggle_selection(path, is_dir);
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(result) = app.search_results.get(app.search_cursor) {
                if result.is_dir {
                    let path = result.path.clone();
                    app.navigate_to_dir(path);
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
            app.go_up_root();
        }
        _ => {}
    }
    None
}

fn handle_normal(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    match key_event.code {
        KeyCode::Char('q') => return Some(vec![]),
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(vec![])
        }
        KeyCode::Char('c') => {
            if app.selected.is_empty() {
                *message = "No files or directories selected!".to_string();
            } else {
                return Some(app.collect_selected_paths());
            }
        }
        KeyCode::Char('/') => {
            app.enter_search();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.tree_state.key_up();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.tree_state.key_down();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(path) = app.highlighted_path() {
                if path.is_dir() {
                    app.ensure_dir_loaded(&path);
                }
            }
            app.tree_state.key_right();
        }
        KeyCode::Enter => {
            if let Some(path) = app.highlighted_path() {
                if path.is_dir() {
                    app.ensure_dir_loaded(&path);
                }
            }
            app.tree_state.toggle_selected();
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.tree_state.key_left();
        }
        KeyCode::Backspace => {
            app.go_up_root();
        }
        KeyCode::Char(' ') => {
            if let Some(path) = app.highlighted_path() {
                let is_dir = path.is_dir();
                app.toggle_selection(path, is_dir);
            }
        }
        KeyCode::Char('r') if !app.no_path => {
            app.relative = !app.relative;
        }
        KeyCode::Char('n') => {
            app.no_path = !app.no_path;
            if app.no_path {
                app.relative = false;
            }
        }
        _ => {}
    }
    None
}
