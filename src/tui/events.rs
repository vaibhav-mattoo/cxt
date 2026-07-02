use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use std::path::PathBuf;

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
        AppMode::GitTree => handle_git_tree(app, key_event, message),
    }
}

fn handle_git_tree(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    match key_event.code {
        KeyCode::Tab => {
            app.git_panel_focused = !app.git_panel_focused;
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
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
        KeyCode::Char(' ') => {
            if app.git_panel_focused {
                app.toggle_git_commit_selection();
            } else {
                app.toggle_git_file_selection();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.git_panel_focused {
                if app.git_commit_cursor > 0 {
                    app.git_commit_cursor -= 1;
                    app.fetch_git_files();
                }
            } else if app.git_files_cursor > 0 {
                app.git_files_cursor -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.git_panel_focused {
                if app.git_commit_cursor + 1 < app.git_commits.len() {
                    app.git_commit_cursor += 1;
                    app.fetch_git_files();
                }
            } else if app.git_files_cursor + 1 < app.git_files.len() {
                app.git_files_cursor += 1;
            }
        }
        _ => {}
    }
    None
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
        KeyCode::Char('f') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
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

pub fn handle_mouse_event(app: &mut AppState, mouse: MouseEvent, _message: &mut String) {
    if app.show_help {
        return;
    }
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if app.mode == AppMode::Normal {
                let pos = Position::new(mouse.column, mouse.row);
                let clicked: Option<Vec<PathBuf>> =
                    app.tree_state.rendered_at(pos).map(|id| id.to_vec());
                if let Some(id) = clicked {
                    if let Some(path) = id.last().cloned() {
                        app.tree_state.select(id);
                        let is_dir = path.is_dir();
                        app.toggle_selection(path, is_dir);
                    }
                }
            } else if app.mode == AppMode::SearchFocused || app.mode == AppMode::SearchNavigating {
                if let Some(area) = app.list_area {
                    let inner_top = area.y + 1;
                    if mouse.row >= inner_top {
                        let idx = app.search_scroll_offset + (mouse.row - inner_top) as usize;
                        if idx < app.search_results.len() {
                            app.search_cursor = idx;
                            if let Some(r) = app.search_results.get(idx) {
                                let path = r.path.clone();
                                let is_dir = r.is_dir;
                                app.toggle_selection(path, is_dir);
                            }
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollDown => {
            if app.mode == AppMode::Normal {
                app.tree_state.scroll_down(1);
            } else if app.mode == AppMode::GitTree {
                if app.git_panel_focused {
                    if app.git_commit_cursor + 1 < app.git_commits.len() {
                        app.git_commit_cursor += 1;
                        app.fetch_git_files();
                    }
                } else if app.git_files_cursor + 1 < app.git_files.len() {
                    app.git_files_cursor += 1;
                }
            } else if app.search_cursor + 1 < app.search_results.len() {
                app.search_cursor += 1;
                app.sync_search_scroll(app.visible_height);
            }
        }
        MouseEventKind::ScrollUp => {
            if app.mode == AppMode::Normal {
                app.tree_state.scroll_up(1);
            } else if app.mode == AppMode::GitTree {
                if app.git_panel_focused {
                    if app.git_commit_cursor > 0 {
                        app.git_commit_cursor -= 1;
                        app.fetch_git_files();
                    }
                } else if app.git_files_cursor > 0 {
                    app.git_files_cursor -= 1;
                }
            } else if app.search_cursor > 0 {
                app.search_cursor -= 1;
                app.sync_search_scroll(app.visible_height);
            }
        }
        _ => {}
    }
}

fn handle_normal(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    if app.show_help {
        match key_event.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                app.show_help = false;
            }
            _ => {}
        }
        return None;
    }
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
        KeyCode::Char('f') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
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
        KeyCode::Char('?') => {
            app.show_help = true;
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
        KeyCode::Tab => {
            app.enter_git_tree_mode();
        }
        _ => {}
    }
    None
}