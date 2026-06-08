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
            if let Some(result) = app.search_results.get(app.cursor) {
                if result.is_dir {
                    let new_path = result.path.clone();
                    app.navigate_into(new_path);
                } else {
                    let path = result.path.clone();
                    let is_dir = result.is_dir;
                    app.toggle_selection(path, is_dir);
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') if app.cursor > 0 => {
            app.cursor -= 1;
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.cursor + 1 < app.search_results.len() =>
        {
            app.cursor += 1;
        }
        KeyCode::Char(' ') => {
            if let Some(result) = app.search_results.get(app.cursor) {
                let path = result.path.clone();
                let is_dir = result.is_dir;
                app.toggle_selection(path, is_dir);
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(result) = app.search_results.get(app.cursor) {
                if result.is_dir {
                    let new_path = result.path.clone();
                    app.navigate_into(new_path);
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.navigate_to_parent();
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
        KeyCode::Up | KeyCode::Char('k') => app.move_cursor_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor_down(),
        KeyCode::Char(' ') => {
            if let Some(entry) = app.entries.get(app.cursor) {
                let path = entry.path();
                let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
                app.toggle_selection(path, is_dir);
            }
        }
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
            if let Some(entry) = app.entries.get(app.cursor) {
                if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                    let new_path = entry.path();
                    app.navigate_into(new_path);
                }
            }
        }
        KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
            app.navigate_to_parent();
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
