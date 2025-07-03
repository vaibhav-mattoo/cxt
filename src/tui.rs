use anyhow::{Result, Context};
use crossterm::{event::{self, Event, KeyCode, KeyEvent}, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}, execute as crossterm_execute};
use ratatui::{backend::CrosstermBackend, Terminal, widgets::{Block, Borders, List, ListItem, Paragraph, Wrap}, layout::{Layout, Constraint, Direction}, style::{Style, Color, Modifier}};
use ratatui::text::{Span, Line};
use std::{collections::HashSet, env, fs, io, path::{PathBuf}, time::Duration};
use std::io::Write;

pub fn run_tui() -> Result<Vec<String>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm_execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = tui_main(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    crossterm_execute!(io::stdout(), LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
    res
}

struct AppState {
    current_dir: PathBuf,
    entries: Vec<fs::DirEntry>,
    cursor: usize,
    selected: HashSet<PathBuf>,
    relative: bool,
    no_path: bool,
}

impl AppState {
    fn new() -> io::Result<Self> {
        let current_dir = env::current_dir()?;
        let entries = read_dir_sorted(&current_dir)?;
        Ok(Self {
            current_dir,
            entries,
            cursor: 0,
            selected: HashSet::new(),
            relative: false,
            no_path: false,
        })
    }
}

fn read_dir_sorted(dir: &PathBuf) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| {
        let md = e.metadata();
        (!md.as_ref().map(|m| m.is_dir()).unwrap_or(false), e.file_name())
    });
    Ok(entries)
}

fn tui_main(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<Vec<String>> {
    let mut app = AppState::new().context("Failed to read current directory")?;
    let mut message = String::new();
    let help_items = vec![
        ("↑/k", "Move up"),
        ("↓/j", "Move down"),
        ("←/h/Backspace", "Up dir"),
        ("→/l/Enter", "Open dir"),
        ("Space", "Select/Unselect"),
        ("c", "Confirm"),
        ("q", "Quit"),
    ];
    loop {
        terminal.draw(|f| {
            // Current path (top bar)
            let path = if app.no_path {
                String::from("[No Path Headers]")
            } else if app.relative {
                match app.current_dir.strip_prefix(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
                    Ok(rel) => rel.display().to_string(),
                    Err(_) => app.current_dir.display().to_string(),
                }
            } else {
                app.current_dir.display().to_string()
            };
            let mut title_str = String::from("Current Directory");
            if app.no_path {
                title_str.push_str(" [n: no path]");
            } else if app.relative {
                title_str.push_str(" [r: relative]");
            }
            let current_dir_title = Span::styled(
                title_str,
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            );
            let path_widget = Paragraph::new(path)
                .block(Block::default().borders(Borders::ALL).title(current_dir_title))
                .wrap(Wrap { trim: true });

            // Helper: recursively check if a path is under a selected directory
            fn is_under_selected(selected: &std::collections::HashSet<std::path::PathBuf>, path: &std::path::Path) -> bool {
                selected.iter().any(|sel| {
                    sel.is_dir() && path.starts_with(sel) && path != sel
                })
            }

            // File list
            let items: Vec<ListItem> = app.entries.iter().enumerate().map(|(i, entry)| {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let md = entry.metadata().ok();
                let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let path = entry.path();
                let mut style = Style::default();
                let mut text = file_name.clone();
                if is_dir {
                    style = style.fg(Color::Blue);
                    text.push('/');
                }
                let is_selected = app.selected.contains(&path) || is_under_selected(&app.selected, &path);
                if is_selected {
                    style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
                }
                // If cursor is over a selected directory, use a different color
                if i == app.cursor {
                    if is_selected && is_dir {
                        style = style.fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::REVERSED | Modifier::BOLD);
                    } else {
                        style = style.fg(Color::Yellow).add_modifier(Modifier::REVERSED);
                    }
                }
                ListItem::new(text).style(style)
            }).collect();
            let files_title = Span::styled(
                "Files (Space: select, Enter/l/→: open, Backspace/h/←: up)",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            );
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(files_title));

            // Dynamically calculate help lines based on width
            let max_width = f.size().width.saturating_sub(6) as usize; // account for borders/margins
            let mut help_lines = vec![];
            let mut spans = vec![];
            let mut current_width = 0;
            let extra_help = vec![
                ("r", "Toggle relative path"),
                ("n", "Toggle no path headers"),
            ];
            let all_help = help_items.iter().chain(extra_help.iter());
            for (i, (key, desc)) in all_help.enumerate() {
                let key_str = format!("{}", key);
                let desc_str = format!(": {}", desc);
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
                spans.push(Span::styled(key_str.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(": ", Style::default().fg(Color::White)));
                spans.push(Span::styled(desc.to_string(), Style::default().fg(Color::Green)));
                current_width += key_str.len() + 2 + desc_str.len();
            }
            if !spans.is_empty() {
                help_lines.push(Line::from(spans));
            }
            let help_height = help_lines.len().max(1) as u16 + 2; // +2 for borders
            let help_title = Span::styled(
                "Help",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            );
            let footer_widget = if message.is_empty() {
                Paragraph::new(help_lines)
                    .block(Block::default().borders(Borders::ALL).title(help_title))
                    .wrap(Wrap { trim: true })
            } else {
                Paragraph::new(vec![Line::from(vec![Span::styled(
                    &message,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )])])
                    .block(Block::default().borders(Borders::ALL).title("Help"))
                    .wrap(Wrap { trim: true })
            };

            // Layout: dynamically set help/footer height
            let layout_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(help_height),
                ])
                .split(f.size());

            f.render_widget(path_widget, layout_chunks[0]);
            f.render_widget(list, layout_chunks[1]);
            f.render_widget(footer_widget, layout_chunks[2]);
        })?;
        terminal.backend_mut().flush()?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') => {
                        return Ok(vec![]); // Quit without selection
                    }
                    KeyCode::Char('c') => {
                        // Confirm selection
                        if app.selected.is_empty() {
                            message = "No files or directories selected!".to_string();
                        } else {
                            return Ok(app.selected.iter().map(|p| p.to_string_lossy().to_string()).collect());
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.cursor > 0 { app.cursor -= 1; }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.cursor + 1 < app.entries.len() { app.cursor += 1; }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(entry) = app.entries.get(app.cursor) {
                            let path = entry.path();
                            if app.selected.contains(&path) {
                                app.selected.remove(&path);
                            } else {
                                app.selected.insert(path);
                            }
                        }
                    }
                    KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                        if let Some(entry) = app.entries.get(app.cursor) {
                            if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                                app.current_dir = entry.path();
                                app.entries = read_dir_sorted(&app.current_dir)?;
                                app.cursor = 0;
                            }
                        }
                    }
                    KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                        if let Some(parent) = app.current_dir.parent() {
                            app.current_dir = parent.to_path_buf();
                            app.entries = read_dir_sorted(&app.current_dir)?;
                            app.cursor = 0;
                        }
                    }
                    KeyCode::Char('r') => {
                        if !app.no_path {
                            app.relative = !app.relative;
                        }
                    }
                    KeyCode::Char('n') => {
                        app.no_path = !app.no_path;
                        if app.no_path {
                            app.relative = false;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
} 
