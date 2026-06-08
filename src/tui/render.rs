use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::{env, path::PathBuf};

use crate::tui::app::{AppMode, AppState};

const HELP_ITEMS: &[(&str, &str)] = &[
    ("↑/k", "Move up"),
    ("↓/j", "Move down"),
    ("←/h/Backspace", "Up dir"),
    ("→/l/Enter", "Open dir"),
    ("Space", "Select/Unselect"),
    ("/", "Search files"),
    ("c", "Confirm"),
    ("q/Ctrl-c", "Quit"),
];

/// Render the full TUI frame and return the inner file-list height in rows.
pub fn draw(f: &mut Frame, app: &AppState, message: &str, token_estimate: usize) -> u16 {
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
                .current_dir
                .strip_prefix(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            {
                Ok(rel) => rel.display().to_string(),
                Err(_) => app.current_dir.display().to_string(),
            }
        } else {
            app.current_dir.display().to_string()
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

fn render_file_list(f: &mut Frame, app: &AppState, area: Rect, list_height: usize) {
    let (visible_items, files_title) = if app.mode != AppMode::Normal {
        let items: Vec<ListItem> = app
            .search_results
            .iter()
            .enumerate()
            .skip(app.scroll_offset)
            .take(list_height)
            .map(|(i, result)| {
                let mut text = result.display_name.clone();
                if result.is_dir && !text.ends_with('/') {
                    text.push('/');
                }
                let style = item_style(app, &result.path, result.is_dir, i == app.cursor);
                ListItem::new(text).style(style)
            })
            .collect();
        let title = Span::styled(
            format!("Search Results ({} found)", app.search_results.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        (items, title)
    } else {
        let items: Vec<ListItem> = app
            .entries
            .iter()
            .enumerate()
            .skip(app.scroll_offset)
            .take(list_height)
            .map(|(i, entry)| {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.metadata().ok().map(|m| m.is_dir()).unwrap_or(false);
                let path = entry.path();
                let mut text = file_name;
                if is_dir {
                    text.push('/');
                }
                let style = item_style(app, &path, is_dir, i == app.cursor);
                ListItem::new(text).style(style)
            })
            .collect();
        let title = Span::styled(
            "Files (Space: select, Enter/l/→: open, Backspace/h/←: up)",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        (items, title)
    };

    let list =
        List::new(visible_items).block(Block::default().borders(Borders::ALL).title(files_title));
    f.render_widget(list, area);
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
