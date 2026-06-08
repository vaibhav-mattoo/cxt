use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
};
use tui_tree_widget::{Tree, TreeItem};

use crate::tui::app::{AppMode, AppState};

const HELP_ITEMS: &[(&str, &str)] = &[
    ("↑/k", "Move up"),
    ("↓/j", "Move down"),
    ("←/h", "Collapse dir"),
    ("→/l", "Expand dir"),
    ("Enter", "Toggle expand"),
    ("Backspace", "Parent dir"),
    ("Space", "Select/Unselect"),
    ("/", "Search files"),
    ("c", "Confirm"),
    ("q/Ctrl-c", "Quit"),
];

/// Render the full TUI frame and return the inner file-list height in rows.
pub fn draw(f: &mut Frame, app: &mut AppState, message: &str, token_estimate: usize) -> u16 {
    let help_lines = build_help_lines(f.area().width, app);
    let status_line_height: u16 = if !app.selected.is_empty() { 1 } else { 0 };
    let help_height = help_lines.len().max(1) as u16 + 2 + status_line_height;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(help_height),
        ])
        .split(f.area());

    let inner_list_height = chunks[1].height.saturating_sub(2);

    render_path_bar(f, app, chunks[0]);
    render_file_list(f, app, chunks[1], inner_list_height as usize);
    render_help_footer(f, message, &help_lines, chunks[2], token_estimate, app.selected.len());

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
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        (search_display, title, style)
    } else {
        let path = if app.no_path {
            "[No Path Headers]".to_string()
        } else if app.relative {
            match app
                .root_dir
                .strip_prefix(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            {
                Ok(rel) => rel.display().to_string(),
                Err(_) => app.root_dir.display().to_string(),
            }
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

    let current_dir_title = Span::styled(
        title_str,
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    );
    let path_widget = Paragraph::new(path)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_dir_title),
        )
        .style(path_style)
        .wrap(Wrap { trim: true });
    f.render_widget(path_widget, area);
}

fn render_file_list(f: &mut Frame, app: &mut AppState, area: Rect, list_height: usize) {
    if app.mode != AppMode::Normal {
        let match_style = Style::default()
            .fg(Color::Cyan)
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
                let base_style = item_style(app, &result.path, result.is_dir, false);
                let line = highlight_matches(
                    &display_text,
                    &result.match_indices,
                    base_style,
                    match_style,
                );
                ListItem::new(line).style(if is_cursor {
                    item_style(app, &result.path, result.is_dir, true)
                } else {
                    Style::default()
                })
            })
            .collect();
        let title = Span::styled(
            format!("Search Results ({} found)", app.search_results.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        let list =
            List::new(items).block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(list, area);
        return;
    }

    // Normal mode: collapsible tree view
    let open = app.tree_state.opened().clone();
    let items = build_styled_tree_items(
        &app.root_dir,
        &app.dir_cache,
        &open,
        &app.selected,
        &app.deselected,
    );

    let title = Span::styled(
        "Files (Space: select, ←/h: collapse, →/l: expand, Enter: toggle, Backspace: parent)",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let Ok(tree_widget) = Tree::new(&items) else {
        f.render_widget(Block::default().borders(Borders::ALL).title(title), area);
        return;
    };
    let tree_widget = tree_widget
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::REVERSED),
        )
        .highlight_symbol("");
    f.render_stateful_widget(tree_widget, area, &mut app.tree_state);
}

fn render_help_footer(
    f: &mut Frame,
    message: &str,
    help_lines: &[Line<'static>],
    area: Rect,
    token_estimate: usize,
    selected_count: usize,
) {
    let help_title = Span::styled(
        "Help",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    );
    let status_line: Option<Line<'static>> = if selected_count > 0 {
        Some(Line::from(vec![Span::styled(
            format!(
                " {} file{}  ~{} tokens",
                selected_count,
                if selected_count == 1 { "" } else { "s" },
                crate::token_counter::format_count(token_estimate),
            ),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]))
    } else {
        None
    };
    let footer_widget = if message.is_empty() {
        let mut content: Vec<Line<'static>> = Vec::new();
        if let Some(status) = status_line {
            content.push(status);
        }
        content.extend_from_slice(help_lines);
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(help_title))
            .wrap(Wrap { trim: true })
    } else {
        let mut content: Vec<Line<'static>> = Vec::new();
        if let Some(status) = status_line {
            content.push(status);
        }
        content.push(Line::from(vec![Span::styled(
            message.to_string(),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )]));
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true })
    };
    f.render_widget(footer_widget, area);
}

fn item_style(app: &AppState, path: &std::path::Path, is_dir: bool, is_cursor: bool) -> Style {
    let is_selected = (app.selected.contains(path) && !app.deselected.contains(path))
        || app.is_implicitly_selected(path);

    let mut style = if is_dir {
        Style::default().fg(Color::Blue)
    } else {
        Style::default()
    };

    if is_selected {
        style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
    }

    if is_cursor {
        if is_selected && is_dir {
            style = style
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        } else {
            style = style.fg(Color::Yellow).add_modifier(Modifier::REVERSED);
        }
    }

    style
}

fn build_help_lines(width: u16, app: &AppState) -> Vec<Line<'static>> {
    let max_width = width.saturating_sub(6) as usize;

    let extra_help: &[(&str, &str)] = if app.mode != AppMode::Normal {
        if app.mode == AppMode::SearchFocused {
            &[("Esc", "Leave search"), ("Enter/↑/↓", "Search")]
        } else {
            &[
                ("/", "Continue searching"),
                ("Esc", "Leave search"),
                ("↑/k", "Move up"),
                ("↓/j", "Move down"),
                ("←/h/Backspace", "Parent dir"),
                ("→/l/Enter", "Open dir"),
                ("Space", "Select/Unselect"),
                ("c", "Confirm"),
                ("q/Ctrl-c", "Quit"),
            ]
        }
    } else {
        &[
            ("r", "Toggle relative path"),
            ("n", "Toggle no path headers"),
        ]
    };

    let combined: Vec<(&str, &str)> = if app.mode != AppMode::Normal {
        extra_help.to_vec()
    } else {
        HELP_ITEMS
            .iter()
            .copied()
            .chain(extra_help.iter().copied())
            .collect()
    };

    let mut help_lines: Vec<Line<'static>> = vec![];
    let mut spans: Vec<Span<'static>> = vec![];
    let mut current_width = 0usize;

    for (i, (key, desc)) in combined.iter().enumerate() {
        let key_str = key.to_string();
        let desc_str = format!(": {desc}");
        let item_width = key_str.len() + desc_str.len() + if i > 0 { 2 } else { 0 };
        if i > 0 {
            if current_width + item_width > max_width {
                help_lines.push(Line::from(std::mem::take(&mut spans)));
                current_width = 0;
            } else {
                spans.push(Span::raw("  "));
                current_width += 2;
            }
        }
        spans.push(Span::styled(
            key_str.clone(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(": ", Style::default().fg(Color::White)));
        spans.push(Span::styled(
            desc.to_string(),
            Style::default().fg(Color::Green),
        ));
        current_width += key_str.len() + 2 + desc_str.len();
    }
    if !spans.is_empty() {
        help_lines.push(Line::from(spans));
    }

    help_lines
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
                if seg_is_match { match_style } else { base_style },
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
    dir_cache: &HashMap<PathBuf, Vec<fs::DirEntry>>,
    open: &HashSet<Vec<PathBuf>>,
    selected: &HashSet<PathBuf>,
    deselected: &HashSet<PathBuf>,
) -> Vec<TreeItem<'static, PathBuf>> {
    let entries = match dir_cache.get(dir) {
        Some(e) => e,
        None => return vec![],
    };

    entries
        .iter()
        .filter_map(|entry| {
            let path = entry.path();
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            let raw_name = entry.file_name().to_string_lossy().to_string();
            let display_name = if is_dir {
                format!("{}/", raw_name)
            } else {
                raw_name
            };

            let is_directly_selected =
                selected.contains(&path) && !deselected.contains(&path);
            let is_implicit = selected.iter().any(|sel| {
                sel.is_dir()
                    && path.starts_with(sel)
                    && path != *sel
                    && !deselected.contains(&path)
            });
            let is_selected = is_directly_selected || is_implicit;

            let base_style = if is_dir {
                Style::default().fg(Color::Blue)
            } else {
                Style::default()
            };
            let style = if is_selected {
                base_style.bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                base_style
            };

            let text = Text::styled(display_name, style);

            if is_dir {
                let is_open = open.iter().any(|kp| kp.last() == Some(&path));
                let children = if is_open {
                    build_styled_tree_items(&path, dir_cache, open, selected, deselected)
                } else {
                    match dir_cache.get(&path) {
                        Some(sub_entries) if !sub_entries.is_empty() => sub_entries
                            .iter()
                            .filter_map(|e| {
                                Some(TreeItem::new_leaf(
                                    e.path(),
                                    e.file_name().to_string_lossy().to_string(),
                                ))
                            })
                            .collect(),
                        // Not yet cached — dummy child so the ▶ indicator always shows.
                        None => vec![TreeItem::new_leaf(path.join("\0"), String::new())],
                        _ => vec![], // confirmed empty directory
                    }
                };
                TreeItem::new(path, text, children).ok()
            } else {
                Some(TreeItem::new_leaf(path, text))
            }
        })
        .collect()
}
