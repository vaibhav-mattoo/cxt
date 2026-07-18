use super::theme;
use crate::tui::app::{AppMode, AppState, DirItem, GitStatusItem, GitStatusSection};
use pathdiff::diff_paths;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Position, Rect},
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
use tui_tree_widget::{Tree, TreeItem};
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
pub fn draw(
    f: &mut Frame,
    app: &mut AppState,
    message: &str,
    file_count: usize,
    loc_count: u64,
) -> u16 {
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
    } else if app.mode == AppMode::GitStatus {
        render_git_status(f, app, chunks[1], inner_list_height as usize);
    } else if app.mode == AppMode::RgFocused || app.mode == AppMode::RgNavigating {
        render_rg(f, app, chunks[1], inner_list_height as usize);
    } else {
        render_file_list(f, app, chunks[1], inner_list_height as usize);
    }
    let is_git_mode = app.mode == AppMode::GitTree || app.mode == AppMode::GitStatus;
    render_status_bar(
        f, chunks[2], message, file_count, loc_count, app.mode, app.aider,
    );
    if app.show_help {
        render_help_overlay(f, f.area());
    }
    if let Some(stash_ref) = app.pending_stash_pop.clone() {
        let stash_message = app
            .git_stash_items
            .iter()
            .find(|s| s.stash_ref == stash_ref)
            .map(|s| s.message.clone())
            .unwrap_or_default();
        render_confirm_overlay(f, f.area(), &stash_ref, &stash_message);
    }
    if let Some(branch) = app.pending_branch_switch.clone() {
        render_branch_switch_overlay(f, f.area(), &branch);
    }
    inner_list_height
}
/// Small centered confirmation modal (e.g. "pop this stash?").
fn render_confirm_overlay(f: &mut Frame, area: Rect, stash_ref: &str, stash_message: &str) {
    let modal = centered_rect(46, 24, area);
    f.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            " Confirm Stash Pop ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    f.render_widget(block, modal);
    let lines = vec![
        Line::from(Span::styled(
            format!("Pop {stash_ref}?"),
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            stash_message.to_string(),
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y",
                Style::default()
                    .fg(theme::SELECTED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm    ", Style::default().fg(theme::MUTED)),
            Span::styled(
                "n / Esc",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(theme::MUTED)),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}
/// Centered confirmation modal for branch switching.
fn render_branch_switch_overlay(f: &mut Frame, area: Rect, branch: &str) {
    let modal = centered_rect(46, 24, area);
    f.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(theme::BORDER_FOCUS)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            " Confirm Branch Switch ",
            Style::default()
                .fg(theme::BORDER_FOCUS)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    f.render_widget(block, modal);
    let lines = vec![
        Line::from(Span::styled(
            format!("Switch to branch '{branch}'?"),
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Uncommitted changes may be carried over or conflict.",
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y",
                Style::default()
                    .fg(theme::SELECTED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm    ", Style::default().fg(theme::MUTED)),
            Span::styled(
                "n / Esc",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(theme::MUTED)),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}
fn render_path_bar(f: &mut Frame, app: &AppState, area: Rect) {
    let (path, title_str, path_style) =
        if app.mode == AppMode::RgFocused || app.mode == AppMode::RgNavigating {
            let rg_display = format!("rg: {}", app.rg_query);
            let title = if app.mode == AppMode::RgFocused {
                "Enter pattern, Esc to leave rg".to_string()
            } else {
                "y copy result   space select   c copy files   Esc to leave".to_string()
            };
            let style = if app.mode == AppMode::RgFocused {
                Style::default()
                    .fg(theme::MATCH)
                    .bg(theme::CURSOR_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            (rg_display, title, style)
        } else if app.mode == AppMode::SearchFocused || app.mode == AppMode::SearchNavigating {
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
            let title_str = if app.mode == AppMode::GitStatus {
                "Git Status".to_string()
            } else {
                let mut t = "Current Directory".to_string();
                if app.no_path {
                    t.push_str(" [n: no path]");
                } else if app.relative {
                    t.push_str(" [r: relative]");
                }
                t
            };
            (path, title_str, Style::default())
        };
    let block = panel(
        &title_str,
        app.mode != AppMode::Normal && app.mode != AppMode::GitStatus,
    );
    let inner = block.inner(area);
    let path_widget = Paragraph::new(path)
        .block(block)
        .style(path_style)
        .wrap(Wrap { trim: true });
    f.render_widget(path_widget, area);
    if app.mode == AppMode::SearchFocused {
        // Position the native cursor right after the typed query text.
        // "Search: " prefix is 8 chars; clamp so a very long query can't
        // push the cursor outside the panel.
        let prefix_len = 8u16;
        let cursor_x = inner
            .x
            .saturating_add(prefix_len)
            .saturating_add(app.search_query.chars().count() as u16)
            .min(inner.x + inner.width.saturating_sub(1));
        f.set_cursor_position(Position::new(cursor_x, inner.y));
    } else if app.mode == AppMode::RgFocused {
        // "rg: " prefix is 4 chars.
        let prefix_len = 4u16;
        let cursor_x = inner
            .x
            .saturating_add(prefix_len)
            .saturating_add(app.rg_query.chars().count() as u16)
            .min(inner.x + inner.width.saturating_sub(1));
        f.set_cursor_position(Position::new(cursor_x, inner.y));
    }
}
fn render_git_tree(f: &mut Frame, app: &mut AppState, area: Rect, list_height: usize) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let hash_style = Style::default()
        .fg(theme::HASH)
        .add_modifier(Modifier::BOLD);
    let refs_style = Style::default()
        .fg(theme::REFS)
        .add_modifier(Modifier::BOLD);
    let commit_items: Vec<ListItem> = app
        .git_commits
        .iter()
        .enumerate()
        .skip(app.git_commit_scroll_offset)
        .take(list_height)
        .map(|(i, commit)| {
            let is_cursor = i == app.git_commit_cursor;
            let line_style = if is_cursor {
                Style::default().bg(theme::CURSOR_BG)
            } else {
                Style::default()
            };
            let fg_style = Style::default().fg(theme::FG);
            let mut h_style = hash_style;
            if is_cursor {
                h_style = h_style.bg(theme::CURSOR_BG);
            }
            let mut r_style = refs_style;
            if is_cursor {
                r_style = r_style.bg(theme::CURSOR_BG);
            }
            let marker_style = Style::default()
                .fg(theme::SELECTED)
                .add_modifier(Modifier::BOLD);
            let marker = if app.is_git_commit_marked(&commit.hash) {
                "✓ "
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = vec![Span::styled(marker, marker_style)];
            if !commit.hash.is_empty() {
                if let Some(pos) = commit.display.find(commit.hash.as_str()) {
                    let before = commit.display[..pos].to_string();
                    let after = commit.display[pos + commit.hash.len()..].to_string();
                    spans.push(Span::styled(before, fg_style));
                    spans.push(Span::styled(commit.hash.clone(), h_style));
                    if !commit.refs.is_empty() {
                        spans.push(Span::styled(format!(" ({})", commit.refs), r_style));
                    }
                    spans.push(Span::styled(after, fg_style));
                } else {
                    spans.push(Span::styled(commit.display.clone(), fg_style));
                }
            } else {
                spans.push(Span::styled(commit.display.clone(), fg_style));
            }
            ListItem::new(Line::from(spans)).style(line_style)
        })
        .collect();
    let commit_list = List::new(commit_items).block(panel("Commits", app.git_panel_focused));
    f.render_widget(commit_list, chunks[0]);
    if app.show_git_diff {
        app.sync_git_diff_scroll(list_height);
        let diff_title = app
            .git_files
            .get(app.git_files_cursor)
            .cloned()
            .unwrap_or_default();
        let cursor_line = if app.git_panel_focused {
            None
        } else {
            Some(app.git_diff_cursor)
        };
        let diff_block = panel(&format!("Diff: {diff_title}"), !app.git_panel_focused);
        let diff_inner_width = diff_block.inner(chunks[1]).width;
        let diff_widget = Paragraph::new(build_diff_lines(
            &app.git_diff_content,
            cursor_line,
            diff_inner_width,
        ))
        .block(diff_block)
        .scroll((app.git_diff_scroll_offset as u16, 0))
        .wrap(Wrap { trim: false });
        f.render_widget(diff_widget, chunks[1]);
    } else {
        let file_items: Vec<ListItem> = app
            .git_files
            .iter()
            .enumerate()
            .skip(app.git_files_scroll_offset)
            .take(list_height)
            .map(|(i, file)| {
                let is_cursor = i == app.git_files_cursor;
                let row_style = if is_cursor {
                    Style::default().bg(theme::CURSOR_BG)
                } else {
                    Style::default()
                };
                let is_selected = app.is_git_file_selected(file);
                let marker = if is_selected { "✓ " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(
                        marker,
                        Style::default()
                            .fg(theme::SELECTED)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(file.clone(), Style::default().fg(theme::FG)),
                ]);
                ListItem::new(line).style(row_style)
            })
            .collect();
        let file_list = List::new(file_items).block(panel("Files", !app.git_panel_focused));
        f.render_widget(file_list, chunks[1]);
    }
}
/// Style diff lines: additions green, deletions red, hunk headers highlighted,
/// file headers (+++/---) muted.
fn build_diff_lines(content: &str, cursor_line: Option<usize>, width: u16) -> Vec<Line<'static>> {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let mut style = if line.starts_with("+++") || line.starts_with("---") {
                Style::default()
                    .fg(theme::MUTED)
                    .add_modifier(Modifier::BOLD)
            } else if line.starts_with("@@") {
                Style::default()
                    .fg(theme::HASH)
                    .add_modifier(Modifier::BOLD)
            } else if line.starts_with('+') {
                Style::default().fg(Color::Green)
            } else if line.starts_with('-') {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(theme::FG)
            };
            let is_cursor = cursor_line == Some(i);
            let text = if is_cursor {
                style = style.bg(theme::CURSOR_BG);
                let pad_width = width.max(line.chars().count() as u16) as usize;
                format!("{:<pad_width$}", line)
            } else {
                line.to_string()
            };
            Line::from(Span::styled(text, style))
        })
        .collect()
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
fn render_git_status(f: &mut Frame, app: &mut AppState, area: Rect, list_height: usize) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    let left_area = chunks[0];
    let right_area = chunks[1];
    // ── Left panel: status list ──
    let staged: Vec<(usize, &GitStatusItem)> = app
        .git_status_items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.section == GitStatusSection::Staged)
        .collect();
    let unstaged: Vec<(usize, &GitStatusItem)> = app
        .git_status_items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.section == GitStatusSection::Unstaged)
        .collect();
    let untracked: Vec<(usize, &GitStatusItem)> = app
        .git_status_items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.section == GitStatusSection::Untracked)
        .collect();
    let items_len = app.git_status_items.len();
    let header_style = Style::default()
        .fg(theme::MUTED)
        .add_modifier(Modifier::BOLD);
    let divider_style = Style::default().fg(theme::BORDER);
    let mut visual_items: Vec<(Option<usize>, ListItem)> = Vec::new();
    let push_header = |v: &mut Vec<(Option<usize>, ListItem)>, title: &str, count: usize| {
        v.push((
            None,
            ListItem::new(Line::from(Span::styled(
                format!(" {title} ({count})"),
                header_style,
            ))),
        ));
        v.push((
            None,
            ListItem::new(Line::from(Span::styled(
                " ────────────────────────────────────────",
                divider_style,
            ))),
        ));
    };
    push_header(&mut visual_items, "Staged Changes", staged.len());
    if staged.is_empty() {
        visual_items.push((
            None,
            ListItem::new(Line::from(Span::styled(
                "   (none)",
                Style::default().fg(theme::MUTED),
            ))),
        ));
    } else {
        for (idx, item) in &staged {
            let is_cursor = *idx == app.git_status_cursor;
            let line = build_git_status_line(app, item);
            visual_items.push((
                Some(*idx),
                ListItem::new(line).style(if is_cursor {
                    Style::default().bg(theme::CURSOR_BG)
                } else {
                    Style::default()
                }),
            ));
        }
    }
    push_header(&mut visual_items, "Unstaged Changes", unstaged.len());
    if unstaged.is_empty() {
        visual_items.push((
            None,
            ListItem::new(Line::from(Span::styled(
                "   (none)",
                Style::default().fg(theme::MUTED),
            ))),
        ));
    } else {
        for (idx, item) in &unstaged {
            let is_cursor = *idx == app.git_status_cursor;
            let line = build_git_status_line(app, item);
            visual_items.push((
                Some(*idx),
                ListItem::new(line).style(if is_cursor {
                    Style::default().bg(theme::CURSOR_BG)
                } else {
                    Style::default()
                }),
            ));
        }
    }
    push_header(&mut visual_items, "Untracked Files", untracked.len());
    if untracked.is_empty() {
        visual_items.push((
            None,
            ListItem::new(Line::from(Span::styled(
                "   (none)",
                Style::default().fg(theme::MUTED),
            ))),
        ));
    } else {
        for (idx, item) in &untracked {
            let is_cursor = *idx == app.git_status_cursor;
            let line = build_git_status_line(app, item);
            visual_items.push((
                Some(*idx),
                ListItem::new(line).style(if is_cursor {
                    Style::default().bg(theme::CURSOR_BG)
                } else {
                    Style::default()
                }),
            ));
        }
    }
    push_header(&mut visual_items, "Stash", app.git_stash_items.len());
    if app.git_stash_items.is_empty() {
        visual_items.push((
            None,
            ListItem::new(Line::from(Span::styled(
                "   (none)",
                Style::default().fg(theme::MUTED),
            ))),
        ));
    } else {
        for (i, stash) in app.git_stash_items.iter().enumerate() {
            let idx = items_len + i;
            let is_cursor = idx == app.git_status_cursor;
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", stash.stash_ref),
                    Style::default()
                        .fg(theme::HASH)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(stash.message.clone(), Style::default().fg(theme::FG)),
            ]);
            visual_items.push((
                Some(idx),
                ListItem::new(line).style(if is_cursor {
                    Style::default().bg(theme::CURSOR_BG)
                } else {
                    Style::default()
                }),
            ));
        }
    }
    push_header(&mut visual_items, "Branches", app.git_branch_items.len());
    if app.git_branch_items.is_empty() {
        visual_items.push((
            None,
            ListItem::new(Line::from(Span::styled(
                "   (none)",
                Style::default().fg(theme::MUTED),
            ))),
        ));
    } else {
        for (i, branch) in app.git_branch_items.iter().enumerate() {
            let idx = items_len + app.git_stash_items.len() + i;
            let is_cursor = idx == app.git_status_cursor;
            let marker = if branch.is_current { "* " } else { "  " };
            let name_style = if branch.is_current {
                Style::default()
                    .fg(theme::SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG)
            };
            let line = Line::from(vec![
                Span::styled(
                    marker.to_string(),
                    Style::default()
                        .fg(theme::SELECTED)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(branch.name.clone(), name_style),
            ]);
            visual_items.push((
                Some(idx),
                ListItem::new(line).style(if is_cursor {
                    Style::default().bg(theme::CURSOR_BG)
                } else {
                    Style::default()
                }),
            ));
        }
    }
    let selected_visual = visual_items
        .iter()
        .position(|(idx, _)| *idx == Some(app.git_status_cursor))
        .unwrap_or(0);
    let scroll_offset = if selected_visual < app.git_status_scroll_offset {
        selected_visual
    } else if selected_visual >= app.git_status_scroll_offset + list_height {
        selected_visual + 1 - list_height
    } else {
        app.git_status_scroll_offset
    };
    app.git_status_scroll_offset = scroll_offset;
    let items: Vec<ListItem> = visual_items
        .into_iter()
        .skip(scroll_offset)
        .take(list_height)
        .map(|(_, item)| item)
        .collect();
    let list = List::new(items).block(panel(
        if app.pending_stash_pop.is_some() {
            "Git Status — confirm pop"
        } else if app.pending_branch_switch.is_some() {
            "Git Status — confirm switch"
        } else {
            "Git Status"
        },
        !app.git_status_diff_focused,
    ));
    f.render_widget(list, left_area);
    // ── Right panel: diff ──
    app.sync_git_status_diff_scroll(list_height);
    let diff_title = if app.git_status_cursor < items_len {
        app.git_status_items
            .get(app.git_status_cursor)
            .map(|i| i.path.clone())
            .unwrap_or_else(|| "No changes".to_string())
    } else {
        let after_items_idx = app.git_status_cursor - items_len;
        let stash_len = app.git_stash_items.len();
        if after_items_idx < stash_len {
            app.git_stash_items
                .get(after_items_idx)
                .map(|s| format!("{} {}", s.stash_ref, s.message))
                .unwrap_or_else(|| "No stash".to_string())
        } else {
            let branch_idx = after_items_idx - stash_len;
            app.git_branch_items
                .get(branch_idx)
                .map(|b| format!("branch: {}", b.name))
                .unwrap_or_else(|| "No branch".to_string())
        }
    };
    let diff_block = panel(&format!("Diff: {diff_title}"), app.git_status_diff_focused);
    let diff_inner_width = diff_block.inner(right_area).width;
    let cursor_line = if app.git_status_diff_focused {
        Some(app.git_status_diff_cursor)
    } else {
        None
    };
    let diff_widget = Paragraph::new(build_diff_lines(
        &app.git_status_diff_content,
        cursor_line,
        diff_inner_width,
    ))
    .block(diff_block)
    .scroll((app.git_status_diff_scroll_offset as u16, 0))
    .wrap(Wrap { trim: false });
    f.render_widget(diff_widget, right_area);
}
fn build_git_status_line(app: &AppState, item: &GitStatusItem) -> Line<'static> {
    let abs_path = app.git_file_abs_path(&item.path);
    let is_selected = app.selected.contains(&abs_path);
    let marker = if is_selected { "✓ " } else { "  " };
    let (section_str, section_color) = match item.section {
        GitStatusSection::Staged => ("M ", Color::Green),
        GitStatusSection::Unstaged => ("M ", Color::Yellow),
        GitStatusSection::Untracked => ("? ", Color::Red),
    };
    Line::from(vec![
        Span::styled(
            marker,
            Style::default()
                .fg(theme::SELECTED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(section_str, Style::default().fg(section_color)),
        Span::styled(item.path.clone(), Style::default().fg(theme::FG)),
    ])
}

fn render_rg(f: &mut Frame, app: &mut AppState, area: Rect, list_height: usize) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    // ── Left panel: unique files that matched, derived from rg_results ──
    // (not the full directory tree — only files rg actually found matches in)
    let mut seen: HashSet<&std::path::Path> = HashSet::new();
    let mut match_files: Vec<&std::path::Path> = Vec::new();
    for result in &app.rg_results {
        if seen.insert(result.path.as_path()) {
            match_files.push(result.path.as_path());
        }
    }
    let current_path = app.rg_results.get(app.rg_cursor).map(|r| r.path.as_path());
    let current_idx = current_path
        .and_then(|p| match_files.iter().position(|x| *x == p))
        .unwrap_or(0);
    let list_len = match_files.len();
    let files_scroll_offset = if list_len <= list_height {
        0
    } else if current_idx >= list_height {
        (current_idx + 1 - list_height).min(list_len - list_height)
    } else {
        0
    };
    let file_items: Vec<ListItem> = match_files
        .iter()
        .enumerate()
        .skip(files_scroll_offset)
        .take(list_height)
        .map(|(_, path)| {
            let is_selected = app.selected.contains(*path);
            let is_current = current_path == Some(*path);
            let marker = if is_selected { "✓ " } else { "  " };
            let path_style = Style::default().fg(theme::DIR);
            let line = Line::from(vec![
                Span::styled(
                    marker,
                    Style::default()
                        .fg(theme::SELECTED)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(path.display().to_string(), path_style),
            ]);
            let row_style = if is_current {
                Style::default().bg(theme::CURSOR_BG)
            } else {
                Style::default()
            };
            ListItem::new(line).style(row_style)
        })
        .collect();
    let files_title = format!("Files ({list_len})");
    let file_list = List::new(file_items).block(panel(&files_title, app.rg_files_focused));
    f.render_widget(file_list, chunks[0]);
    if list_len > list_height {
        let mut sb_state = ScrollbarState::new(list_len).position(current_idx);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            chunks[0].inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
    // ── Right panel: rg results, grouped by file (rg --heading style) ──
    // Consecutive results sharing a path are grouped under one header row
    // instead of repeating the (often long) path on every match line.
    let match_style = Style::default()
        .fg(theme::MATCH)
        .add_modifier(Modifier::BOLD);
    let header_style = Style::default().fg(theme::DIR).add_modifier(Modifier::BOLD);
    let line_num_style = Style::default().fg(theme::HASH);
    let content_style = Style::default().fg(theme::FG);
    let muted_style = Style::default().fg(theme::MUTED);
    let mut visual_items: Vec<(Option<usize>, ListItem)> = Vec::new();
    let mut last_path: Option<&std::path::Path> = None;
    for (i, result) in app.rg_results.iter().enumerate() {
        if last_path != Some(result.path.as_path()) {
            visual_items.push((
                None,
                ListItem::new(Line::from(Span::styled(
                    result.path.display().to_string(),
                    header_style,
                ))),
            ));
            last_path = Some(result.path.as_path());
        }
        let is_cursor = i == app.rg_cursor;
        let is_selected = app.selected.contains(&result.path);
        let marker = if is_selected { "✓ " } else { "  " };
        let mut spans: Vec<Span<'static>> = vec![
            Span::styled(
                format!("  {marker}"),
                Style::default()
                    .fg(theme::SELECTED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(result.line_number.to_string(), line_num_style),
            Span::styled(":", muted_style),
        ];
        let content_line = highlight_matches(
            &result.line_content,
            &result.match_indices,
            content_style,
            match_style,
        );
        spans.extend(content_line.spans);
        let cursor_style = if is_cursor {
            Style::default()
                .bg(theme::CURSOR_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        visual_items.push((
            Some(i),
            ListItem::new(Line::from(spans)).style(cursor_style),
        ));
    }
    let visual_len = visual_items.len();
    let selected_visual = visual_items
        .iter()
        .position(|(idx, _)| *idx == Some(app.rg_cursor))
        .unwrap_or(0);
    let mut scroll_offset = if selected_visual < app.rg_scroll_offset {
        selected_visual
    } else if selected_visual >= app.rg_scroll_offset + list_height {
        selected_visual + 1 - list_height
    } else {
        app.rg_scroll_offset
    };
    scroll_offset = scroll_offset.min(visual_len.saturating_sub(list_height));
    app.rg_scroll_offset = scroll_offset;
    let items: Vec<ListItem> = visual_items
        .into_iter()
        .skip(scroll_offset)
        .take(list_height)
        .map(|(_, item)| item)
        .collect();
    let title = if app.rg_results.is_empty() && !app.rg_query.is_empty() {
        "rg Results (no matches)".to_string()
    } else {
        format!("rg Results ({})", app.rg_results.len())
    };
    let list = List::new(items).block(panel(
        &title,
        app.mode == AppMode::RgNavigating && !app.rg_files_focused,
    ));
    f.render_widget(list, chunks[1]);
    if visual_len > 0 {
        let mut sb_state = ScrollbarState::new(visual_len).position(selected_visual);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}
fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    message: &str,
    file_count: usize,
    loc_count: u64,
    mode: AppMode,
    aider: bool,
) {
    let hint_str = match mode {
        AppMode::GitStatus => {
            "space select   s stage   z stash   Tab switch   c copy   m aider   ? help   q quit "
        }
        AppMode::GitTree => "space select   d diff   c copy   m aider   ? help   q quit ",
        AppMode::RgFocused | AppMode::RgNavigating => {
            "' edit   Tab switch panel   y copy result   space select   c copy files   m aider   ? help   q quit "
        }
        _ => "space select   c copy   m aider   ? help   q quit ",
    };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(hint_str.len() as u16),
        ])
        .split(area);
    if message.is_empty() {
        let mut spans = vec![Span::styled(
            format!(
                " {file_count} file{} selected | {loc_count} LOC",
                if file_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(theme::SELECTED),
        )];
        if aider {
            spans.push(Span::styled(
                " (aider patch)",
                Style::default()
                    .fg(theme::MATCH)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let left = Line::from(spans);
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
        ("1", "Git status mode"),
        ("2", "Git tree mode"),
        ("Tab", "Switch panel / exit git"),
        ("s", "Stage/Unstage (Git Status mode)"),
        ("z", "Stash changes (Git Status mode)"),
        ("Enter", "Pop stash / Switch branch"),
        ("d", "Toggle diff (Git mode)"),
        ("/ or Ctrl-f", "Search files"),
        ("'", "rg search file contents"),
        ("?", "Toggle help"),
        ("c", "Confirm selection"),
        ("m", "Toggle aider patch"),
        ("p", "Restore last selection"),
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
