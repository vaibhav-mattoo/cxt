use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
};
use pathdiff::diff_paths;
use tui_tree_widget::{Tree, TreeItem};

use super::theme;
use crate::tui::app::{AppMode, AppState, DirItem};

fn panel(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            theme::BORDER_FOCUS
        } else {
            theme::BORDER
        }))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::BOLD),
        ))
}

/// Render the full TUI frame and return the inner file-list height in rows.
pub fn draw(f: &mut Frame, app: &mut AppState, message: &str, file_count: usize, loc_count: u64) -> u16 {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());
    let inner_list_height = chunks[1].height.saturating_sub(2);
    app.list_area = Some(chunks[1]);
    render_path_bar(f, app, chunks[0]);
    if app.mode == AppMode::GitTree {
        render_git_tree(f, app, chunks[1], inner_list_height as usize);
    } else {
        render_file_list(f, app, chunks[1], inner_list_height as usize);
    }
    render_status_bar(f, chunks[2], message, file_count, loc_count);
    if app.show_help {
        render_help_overlay(f, f.area());
    }
    inner_list_height
}

fn render_path_bar(f: &mut Frame, app: &AppState, area: Rect) {
    let (path, title_str, path_style) = if app.mode != AppMode::Normal {
        let search_display = format!("Search: {}", app.search_query);
        let title = if app.mode == AppMode::SearchFocused {
            "Enter to search, Esc to leave search".to_string()
        } else {
            "Esc to leave search".to_string()
        };
        let style = if app.mode == AppMode::SearchFocused {
            Style::default()
                .fg(theme::MATCH)
                .bg(theme::CURSOR_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        (search_display, title, style)
    } else {
        let path = if app.no_path {
            "[No Path Headers]".to_string()
        } else if app.relative {
            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            diff_paths(&app.root_dir, &cwd)
                .unwrap_or_else(|| app.root_dir.clone())
                .display()
                .to_string()
        } else {
            app.root_dir.display().to_string()
        };
        let mut title_str = "Current Directory".to_string();
        if app.no_path {
            title_str.push_str(" [n: no path]");
        } else if app.relative {
            title_str.push_str(" [r: relative]");
        }
        (path, title_str, Style::default())
    };

    let path_widget = Paragraph::new(path)
        .block(panel(&title_str, app.mode != AppMode::Normal))
        .style(path_style)
        .wrap(Wrap { trim: true });
    f.render_widget(path_widget, area);
}

fn render_git_tree(f: &mut Frame, app: &mut AppState, area: Rect, list_height: usize) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let commit_items: Vec<ListItem> = app
        .git_commits
        .iter()
        .enumerate()
        .skip(app.git_commit_scroll_offset)
        .take(list_height)
        .map(|(i, commit)| {
            let style = if i == app.git_commit_cursor {
                Style::default().bg(theme::CURSOR_BG).fg(theme::FG)
            } else {
                Style::default().fg(theme::FG)
            };
            ListItem::new(Line::from(vec![Span::styled(
                commit.display.clone(),
                style,
            )]))
        })
        .collect();

    let commit_list = List::new(commit_items).block(panel("Commits", app.git_panel_focused));
    f.render_widget(commit_list, chunks[0]);

    let file_items: Vec<ListItem> = app
        .git_files
        .iter()
        .enumerate()
        .skip(app.git_files_scroll_offset)
        .take(list_height)
        .map(|(i, file)| {
            let style = if i == app.git_files_cursor {
                Style::default().bg(theme::CURSOR_BG).fg(theme::FG)
            } else {
                Style::default().fg(theme::FG)
            };
            ListItem::new(Line::from(vec![Span::styled(
                file.clone(),
                style,
            )]))
        })
        .collect();

    let file_list = List::new(file_items).block(panel("Files", !app.git_panel_focused));
    f.render_widget(file_list, chunks[1]);
}

fn render_file_list(f: &mut Frame, app: &mut AppState, area: Rect, list_height: usize) {
    if app.mode != AppMode::Normal {
        let match_style = Style::default()
            .fg(theme::MATCH)
            .add_modifier(Modifier::BOLD);
        let items: Vec<ListItem> = app
            .search_results
            .iter()
            .enumerate()
            .skip(app.search_scroll_offset)
            .take(list_height)
            .map(|(i, result)| {
                let mut display_text = result.display_name.clone();
                if result.is_dir && !display_text.ends_with('/') {
                    display_text.push('/');
                }
                let is_cursor = i == app.search_cursor;
                let is_selected = app.is_selected(&result.path, result.is_dir);

                let marker = if is_selected { "✓ " } else { "  " };
                let base_style = if result.is_dir {
                    Style::default().fg(theme::DIR)
                } else {
                    Style::default().fg(theme::FG)
                };

                let mut line = highlight_matches(
                    &display_text,
                    &result.match_indices,
                    base_style,
                    match_style,
                );
                line.spans.insert(
                    0,
                    Span::styled(
                        marker.to_string(),
                        Style::default()
                            .fg(theme::SELECTED)
                            .add_modifier(Modifier::BOLD),
                    ),
                );

                let cursor_style = if is_cursor {
                    Style::default()
                        .bg(theme::CURSOR_BG)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(line).style(cursor_style)
            })
            .collect();

        let list = List::new(items).block(panel("Files", app.mode != AppMode::Normal));
        f.render_widget(list, area);

        let mut sb_state =
            ScrollbarState::new(app.search_results.len()).position(app.search_cursor);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
        return;
    }

    // Normal mode: collapsible tree view.
    // Pre-pass: compute which visible directories are fully selected so we can
    // pass an immutable HashSet into the recursive tree builder (avoids borrow
    // conflicts with the mutable tree_state render below).
    let open = app.tree_state.opened().clone();
    let visible_dirs = collect_visible_dirs(&app.root_dir, &app.dir_cache, &open);
    let fully_selected_dirs: HashSet<PathBuf> = visible_dirs
        .into_iter()
        .filter(|d| app.dir_fully_selected(d))
        .collect();

    let items = build_styled_tree_items(
        &app.root_dir,
        &app.dir_cache,
        &open,
        &app.selected,
        &fully_selected_dirs,
    );

    let Ok(tree_widget) = Tree::new(&items) else {
        f.render_widget(panel("Files", true), area);
        return;
    };
    let tree_widget = tree_widget
        .block(panel("Files", app.mode == AppMode::Normal))
        .highlight_style(
            Style::default()
                .bg(theme::CURSOR_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▎ ");
    f.render_stateful_widget(tree_widget, area, &mut app.tree_state);
}

fn render_status_bar(f: &mut Frame, area: Rect, message: &str, file_count: usize, loc_count: u64) {
    let hint_str = "space select   c copy   ? help   q quit ";
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(hint_str.len() as u16),
        ])
        .split(area);
    if message.is_empty() {
        let left = Line::from(vec![Span::styled(
            format!(
                " {file_count} file{} selected | {loc_count} LOC",
                if file_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(theme::SELECTED),
        )]);
        f.render_widget(Paragraph::new(left), chunks[0]);
    } else {
        let error = Line::from(vec![Span::styled(
            format!(" {message}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]);
        f.render_widget(Paragraph::new(error), chunks[0]);
    }

    let hint = Line::from(vec![Span::styled(
        hint_str,
        Style::default().fg(theme::MUTED),
    )]);
    f.render_widget(Paragraph::new(hint), chunks[1]);
}

fn render_help_overlay(f: &mut Frame, area: Rect) {
    let modal = centered_rect(60, 85, area);
    f.render_widget(Clear, modal);

    let block = panel("Keybindings", true);
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    // Reserve the last inner row for the close hint.
    let content_area = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let hint_area = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: inner.height.min(1),
        ..inner
    };

    let help_lines = build_help_lines();
    f.render_widget(Paragraph::new(help_lines), content_area);

    let close_hint = Line::from(vec![Span::styled(
        "? / Esc  close ",
        Style::default().fg(theme::MUTED),
    )])
    .right_aligned();
    f.render_widget(Paragraph::new(close_hint), hint_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// One keybinding per line, key padded to the width of the longest key.
fn build_help_lines() -> Vec<Line<'static>> {
    const ALL: &[(&str, &str)] = &[
        ("↑/k", "Move up"),
        ("↓/j", "Move down"),
        ("←/h", "Collapse dir"),
        ("→/l", "Expand dir"),
        ("Enter", "Toggle expand"),
        ("Backspace", "Parent dir"),
        ("Space", "Select/Unselect"),
        ("/ or Ctrl-f", "Search files"),
        ("?", "Toggle help"),
        ("c", "Confirm"),
        ("q/Ctrl-c", "Quit"),
        ("r", "Toggle relative path"),
        ("n", "Toggle no path headers"),
    ];

    let key_width = ALL.iter().map(|(k, _)| k.len()).max().unwrap_or(0);

    ALL.iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{:<key_width$}", key),
                    Style::default()
                        .fg(theme::BORDER_FOCUS)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  —  ", Style::default().fg(theme::MUTED)),
                Span::styled(desc.to_string(), Style::default().fg(theme::FG)),
            ])
        })
        .collect()
}

/// Collect all directory paths visible in the current tree view
/// (top-level entries plus all recursively opened subdirectories).
fn collect_visible_dirs(
    dir: &PathBuf,
    dir_cache: &HashMap<PathBuf, Vec<DirItem>>,
    open: &HashSet<Vec<PathBuf>>,
) -> Vec<PathBuf> {
    let Some(entries) = dir_cache.get(dir) else {
        return vec![];
    };
    let mut result = Vec::new();
    for entry in entries {
        let path = entry.path();
        if entry.is_dir() {
            result.push(path.clone());
            let is_open = open.iter().any(|kp| kp.last() == Some(&path));
            if is_open {
                result.extend(collect_visible_dirs(&path, dir_cache, open));
            }
        }
    }
    result
}

/// Build a `Line` from `text` where characters at `indices` use `match_style`
/// and all others use `base_style`.
fn highlight_matches(
    text: &str,
    indices: &[usize],
    base_style: Style,
    match_style: Style,
) -> Line<'static> {
    if indices.is_empty() {
        return Line::from(Span::styled(text.to_string(), base_style));
    }

    let matched: std::collections::HashSet<usize> = indices.iter().copied().collect();
    let chars: Vec<char> = text.chars().collect();

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut seg_start = 0;
    let mut seg_is_match = matched.contains(&0);

    for i in 1..=chars.len() {
        let cur_is_match = i < chars.len() && matched.contains(&i);
        if i == chars.len() || cur_is_match != seg_is_match {
            let segment: String = chars[seg_start..i].iter().collect();
            spans.push(Span::styled(
                segment,
                if seg_is_match {
                    match_style
                } else {
                    base_style
                },
            ));
            seg_start = i;
            seg_is_match = cur_is_match;
        }
    }

    Line::from(spans)
}

/// Build styled TreeItems for a directory from the cache.
/// Only recurses into directories that are in `open`.
/// Closed directories with cached entries include flat stubs so the ▶ symbol shows.
fn build_styled_tree_items(
    dir: &PathBuf,
    dir_cache: &HashMap<PathBuf, Vec<DirItem>>,
    open: &HashSet<Vec<PathBuf>>,
    selected: &HashSet<PathBuf>,
    fully_selected_dirs: &HashSet<PathBuf>,
) -> Vec<TreeItem<'static, PathBuf>> {
    let entries = match dir_cache.get(dir) {
        Some(e) => e,
        None => return vec![],
    };

    entries
        .iter()
        .filter_map(|entry| {
            let path = entry.path();
            let is_dir = entry.is_dir();
            let raw_name = entry.file_name().to_string_lossy().to_string();
            let display_name = if is_dir {
                format!("{}/", raw_name)
            } else {
                raw_name
            };

            let is_selected = if is_dir {
                fully_selected_dirs.contains(&path)
            } else {
                selected.contains(&path)
            };

            let marker = if is_selected { "✓ " } else { "  " };
            let name_style = if is_dir {
                Style::default().fg(theme::DIR)
            } else {
                Style::default().fg(theme::FG)
            };
            let text = Line::from(vec![
                Span::styled(
                    marker,
                    Style::default()
                        .fg(theme::SELECTED)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(display_name, name_style),
            ]);

            if is_dir {
                let is_open = open.iter().any(|kp| kp.last() == Some(&path));
                let children = if is_open {
                    build_styled_tree_items(&path, dir_cache, open, selected, fully_selected_dirs)
                } else {
                    match dir_cache.get(&path) {
                        Some(sub_entries) if !sub_entries.is_empty() => sub_entries
                            .iter()
                            .map(|e| {
                                TreeItem::new_leaf(
                                    e.path(),
                                    e.file_name().to_string_lossy().to_string(),
                                )
                            })
                            .collect(),
                        // Not yet cached — dummy child so the ▶ indicator always shows.
                        None => vec![TreeItem::new_leaf(path.join("\0"), String::new())],
                        _ => vec![],
                    }
                };
                TreeItem::new(path, text, children).ok()
            } else {
                Some(TreeItem::new_leaf(path, text))
            }
        })
        .collect()
}