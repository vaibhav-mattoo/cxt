use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use std::path::PathBuf;

use crate::tui::app::{AppMode, AppState, GitStatusSection};

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
        AppMode::GitStatus => handle_git_status(app, key_event, message),
    }
}

fn handle_git_status(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    if let Some(stash_ref) = app.pending_stash_pop.clone() {
        match key_event.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = std::process::Command::new("git")
                    .args(["stash", "pop", &stash_ref])
                    .output();
                app.pending_stash_pop = None;
                let old_cursor = app.git_status_cursor;
                app.enter_git_status_mode();
                app.git_status_cursor = old_cursor.min(app.git_status_total_len().saturating_sub(1));
                app.fetch_git_status_diff();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.pending_stash_pop = None;
            }
            _ => {}
        }
        return None;
    }
    match key_event.code {
        KeyCode::Tab => {
            app.git_status_diff_focused = !app.git_status_diff_focused;
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.git_status_diff_focused = false;
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.git_status_diff_focused = true;
        }
        KeyCode::Char('1') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        KeyCode::Char('2') => {
            app.enter_git_tree_mode();
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
        KeyCode::Char('m') => {
            app.aider = !app.aider;
            message.clear();
        }
        KeyCode::Char('s') => {
            if let Some(item) = app.git_status_items.get(app.git_status_cursor).cloned() {
                let path = &item.path;
                match item.section {
                    GitStatusSection::Staged => {
                        let _ = std::process::Command::new("git")
                            .args(["restore", "--staged", path])
                            .output();
                    }
                    GitStatusSection::Unstaged | GitStatusSection::Untracked => {
                        let _ = std::process::Command::new("git")
                            .args(["add", path])
                            .output();
                    }
                }
                let old_cursor = app.git_status_cursor;
                app.enter_git_status_mode();
                app.git_status_cursor = old_cursor.min(app.git_status_items.len().saturating_sub(1));
                app.fetch_git_status_diff();
            }
        }
        KeyCode::Char('z') => {
            let _ = std::process::Command::new("git")
                .args(["stash", "push"])
                .output();
            let old_cursor = app.git_status_cursor;
            app.enter_git_status_mode();
            app.git_status_cursor = old_cursor.min(app.git_status_total_len().saturating_sub(1));
            app.fetch_git_status_diff();
        }
        KeyCode::Enter => {
            let items_len = app.git_status_items.len();
            if app.git_status_cursor >= items_len {
                if let Some(stash) = app
                    .git_stash_items
                    .get(app.git_status_cursor - items_len)
                    .cloned()
                {
                    app.pending_stash_pop = Some(stash.stash_ref.clone());
                }
            }
        }
        KeyCode::Char(' ') => {
            if let Some(item) = app.git_status_items.get(app.git_status_cursor).cloned() {
                let path = app.git_file_abs_path(&item.path);
                app.invalidate_caches();
                if app.selected.remove(&path) {
                    app.git_base_selected.remove(&path);
                } else {
                    app.selected.insert(path.clone());
                    app.git_base_selected.insert(path);
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.git_status_diff_focused {
                if app.git_status_diff_cursor > 0 {
                    app.git_status_diff_cursor -= 1;
                }
            } else if app.git_status_cursor > 0 {
                app.git_status_cursor -= 1;
                app.fetch_git_status_diff();
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.git_status_diff_focused {
                let len = app.git_status_diff_content.lines().count();
                if app.git_status_diff_cursor + 1 < len {
                    app.git_status_diff_cursor += 1;
                }
            } else if app.git_status_cursor + 1 < app.git_status_total_len() {
                app.git_status_cursor += 1;
                app.fetch_git_status_diff();
            }
        }
        _ => {}
    }
    None
}

fn handle_git_tree(
    app: &mut AppState,
    key_event: KeyEvent,
    message: &mut String,
) -> Option<Vec<String>> {
    match key_event.code {
        KeyCode::Tab | KeyCode::Char('2') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        KeyCode::Char('1') => {
            app.enter_git_status_mode();
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
        KeyCode::Char('m') => {
            app.aider = !app.aider;
            message.clear();
        }
        KeyCode::Char('p') => {
            let added = app.restore_last_selection();
            *message = if added > 0 {
                format!("Restored last selection (+{added} file{}).", if added == 1 { "" } else { "s" })
            } else {
                "No previous selection in this session.".to_string()
            };
        }
        KeyCode::Char(' ') => {
            if app.git_panel_focused {
                app.toggle_git_commit_mark();
            } else {
                app.toggle_git_file_selection();
            }
        }
        KeyCode::Char('d') => {
            app.show_git_diff = !app.show_git_diff;
            if app.show_git_diff {
                app.fetch_git_diff();
                app.git_diff_scroll_offset = 0;
                app.git_diff_cursor = 0;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.git_panel_focused {
                if app.git_commit_cursor > 0 {
                    app.git_commit_cursor -= 1;
                    app.fetch_git_files();
                    if app.show_git_diff {
                        app.fetch_git_diff();
                        app.git_diff_scroll_offset = 0;
                        app.git_diff_cursor = 0;
                    }
                }
            } else if app.show_git_diff {
                if app.git_diff_cursor > 0 {
                    app.git_diff_cursor -= 1;
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
                    if app.show_git_diff {
                        app.fetch_git_diff();
                        app.git_diff_scroll_offset = 0;
                        app.git_diff_cursor = 0;
                    }
                }
            } else if app.show_git_diff {
                let len = app.git_diff_content.lines().count();
                if app.git_diff_cursor + 1 < len {
                    app.git_diff_cursor += 1;
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
        KeyCode::Char('m') => {
            app.aider = !app.aider;
            message.clear();
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
        KeyCode::Char('p') => {
            let added = app.restore_last_selection();
            *message = if added > 0 {
                format!("Restored last selection (+{added} file{}).", if added == 1 { "" } else { "s" })
            } else {
                "No previous selection in this session.".to_string()
            };
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
            } else if app.mode == AppMode::GitStatus {
                if app.git_status_diff_focused {
                    let len = app.git_status_diff_content.lines().count();
                    if app.git_status_diff_cursor + 1 < len {
                        app.git_status_diff_cursor += 1;
                    }
                } else if app.git_status_cursor + 1 < app.git_status_total_len() {
                    app.git_status_cursor += 1;
                    app.fetch_git_status_diff();
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
            } else if app.mode == AppMode::GitStatus {
                if app.git_status_diff_focused {
                    if app.git_status_diff_cursor > 0 {
                        app.git_status_diff_cursor -= 1;
                    }
                } else if app.git_status_cursor > 0 {
                    app.git_status_cursor -= 1;
                    app.fetch_git_status_diff();
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
        KeyCode::Char('m') => {
            app.aider = !app.aider;
            message.clear();
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
        KeyCode::Char('p') => {
            let added = app.restore_last_selection();
            *message = if added > 0 {
                format!("Restored last selection (+{added} file{}).", if added == 1 { "" } else { "s" })
            } else {
                "No previous selection in this session.".to_string()
            };
        }
       KeyCode::Tab | KeyCode::Char('2') => {
            app.enter_git_tree_mode();
        }
        KeyCode::Char('1') => {
            app.enter_git_status_mode();
        }
        _ => {}
    }
    None
}
