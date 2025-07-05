use anyhow::{Result, Context};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute as crossterm_execute,
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    layout::{Layout, Constraint, Direction},
    style::{Style, Color, Modifier},
};
use ratatui::text::{Span, Line};
use std::{
    collections::HashSet,
    env,
    fs,
    io,
    path::PathBuf,
    time::Duration,
};
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
    scroll_offset: usize,
    visible_height: usize,
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
            scroll_offset: 0,
            visible_height: 10,
            selected: HashSet::new(),
            relative: false,
            no_path: false,
        })
    }

    /// Ensure cursor and scroll_offset are within valid bounds.
    fn ensure_cursor_visible(&mut self, visible_height: usize) {
        self.visible_height = visible_height;

        // Clamp cursor
        if self.cursor >= self.entries.len() {
            self.cursor = self.entries.len().saturating_sub(1);
        }

        // No scrolling needed if everything fits
        if self.entries.len() <= visible_height {
            self.scroll_offset = 0;
            return;
        }

        let top_margin = 2;
        let bottom_margin = 2;

        let visible_start = self.scroll_offset;
        let visible_end = self.scroll_offset + visible_height;

        // Scroll up
        if self.cursor < visible_start + top_margin && self.scroll_offset > 0 {
            self.scroll_offset = self.cursor.saturating_sub(top_margin);
        }
        // Scroll down
        else if self.cursor + bottom_margin >= visible_end && self.scroll_offset + visible_height < self.entries.len() {
            self.scroll_offset = (self.cursor + bottom_margin + 1).saturating_sub(visible_height);
        }

        // Clamp scroll_offset
        let max_scroll = self.entries.len().saturating_sub(visible_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    fn move_cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_cursor_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }

    fn reset_cursor(&mut self) {
        self.cursor = 0;
        self.scroll_offset = 0;
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
            // Build help lines
            let max_width = f.size().width.saturating_sub(6) as usize;
            let mut help_lines = vec![];
            let mut spans = vec![];
            let mut current_width = 0;
            let extra_help = [
                ("r", "Toggle relative path"),
                ("n", "Toggle no path headers"),
            ];
            for (i, (key, desc)) in help_items.iter().chain(extra_help.iter()).enumerate() {
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
                spans.push(Span::styled(key_str.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(": ", Style::default().fg(Color::White)));
                spans.push(Span::styled(desc.to_string(), Style::default().fg(Color::Green)));
                current_width += key_str.len() + 2 + desc_str.len();
            }
            if !spans.is_empty() {
                help_lines.push(Line::from(spans));
            }
            let help_height = help_lines.len().max(1) as u16 + 2;

            // Split the terminal into (path bar, file list, help/footer)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(help_height),
                ])
                .split(f.size());

            // Determine inner list height (subtract top+bottom border)
            let inner_list_height = chunks[1].height.saturating_sub(2) as usize;
            app.ensure_cursor_visible(inner_list_height);

            // Build the path widget
            let path = if app.no_path {
                "[No Path Headers]".to_string()
            } else if app.relative {
                match app.current_dir.strip_prefix(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
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
            let current_dir_title = Span::styled(
                title_str,
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            );
            let path_widget = Paragraph::new(path)
                .block(Block::default().borders(Borders::ALL).title(current_dir_title))
                .wrap(Wrap { trim: true });

            // Helper to check nested selection
            fn is_under_selected(selected: &HashSet<PathBuf>, path: &std::path::Path) -> bool {
                selected.iter().any(|sel| {
                    sel.is_dir() && path.starts_with(sel) && path != sel
                })
            }

            // Build the file list
            let visible_items: Vec<ListItem> = app.entries
                .iter()
                .enumerate()
                .skip(app.scroll_offset)
                .take(inner_list_height)
                .map(|(i, entry)| {
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
                    if i == app.cursor {
                        if is_selected && is_dir {
                            style = style.fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::REVERSED | Modifier::BOLD);
                        } else {
                            style = style.fg(Color::Yellow).add_modifier(Modifier::REVERSED);
                        }
                    }
                    ListItem::new(text).style(style)
                })
                .collect();
            let files_title = Span::styled(
                "Files (Space: select, Enter/l/→: open, Backspace/h/←: up)",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            );
            let list = List::new(visible_items)
                .block(Block::default().borders(Borders::ALL).title(files_title));

            // Build the footer/help widget
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

            // Render all three panes
            f.render_widget(path_widget,   chunks[0]);
            f.render_widget(list,          chunks[1]);
            f.render_widget(footer_widget, chunks[2]);
        })?;
        terminal.backend_mut().flush()?;

        // Input handling
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') => return Ok(vec![]),
                    KeyCode::Char('c') => {
                        if app.selected.is_empty() {
                            message = "No files or directories selected!".to_string();
                        } else {
                            return Ok(app
                                .selected
                                .iter()
                                .map(|p| p.to_string_lossy().to_string())
                                .collect());
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k')    => app.move_cursor_up(),
                    KeyCode::Down | KeyCode::Char('j')  => app.move_cursor_down(),
                    KeyCode::Char(' ')                  => {
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
                                app.reset_cursor();
                            }
                        }
                    }
                    KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                        if let Some(parent) = app.current_dir.parent() {
                            app.current_dir = parent.to_path_buf();
                            app.entries = read_dir_sorted(&app.current_dir)?;
                            app.reset_cursor();
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
