use anyhow::{Result, Context};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
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
    collections::{HashMap, HashSet},
    env,
    fs,
    io,
    path::PathBuf,
    time::Duration,
};

#[derive(Clone)]
struct SearchResult {
    path: PathBuf,
    display_name: String,
    is_dir: bool,
}
use std::io::Write;
use walkdir;

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
    deselected: HashSet<PathBuf>,
    relative: bool,
    no_path: bool,
    directory_history: HashMap<PathBuf, (usize, usize)>, // (cursor, scroll_offset) for each directory
    search_history: HashMap<PathBuf, (String, Vec<SearchResult>)>, // (search_query, search_results) for each directory
    // Search mode fields
    search_mode: bool,
    search_focused: bool, // Whether search box is focused for input
    search_query: String,
    search_results: Vec<SearchResult>,
    original_cursor: usize,
    original_scroll_offset: usize,
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
            deselected: HashSet::new(),
            relative: false,
            no_path: false,
            directory_history: HashMap::new(),
            search_history: HashMap::new(),
            search_mode: false,
            search_focused: false,
            search_query: String::new(),
            search_results: Vec::new(),
            original_cursor: 0,
            original_scroll_offset: 0,
        })
    }

    /// Ensure cursor and scroll_offset are within valid bounds.
    fn ensure_cursor_visible(&mut self, visible_height: usize) {
        self.visible_height = visible_height;

        let entries_len = if self.search_mode {
            self.search_results.len()
        } else {
            self.entries.len()
        };

        // Clamp cursor
        if self.cursor >= entries_len {
            self.cursor = entries_len.saturating_sub(1);
        }

        // No scrolling needed if everything fits
        if entries_len <= visible_height {
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
        else if self.cursor + bottom_margin >= visible_end && self.scroll_offset + visible_height < entries_len {
            self.scroll_offset = (self.cursor + bottom_margin + 1).saturating_sub(visible_height);
        }

        // Clamp scroll_offset
        let max_scroll = entries_len.saturating_sub(visible_height);
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

    fn save_directory_state(&mut self) {
        self.directory_history.insert(
            self.current_dir.clone(),
            (self.cursor, self.scroll_offset),
        );
    }

    fn restore_directory_state(&mut self) {
        if let Some(&(cursor, scroll_offset)) = self.directory_history.get(&self.current_dir) {
            self.cursor = cursor;
            self.scroll_offset = scroll_offset;
        } else {
            self.reset_cursor();
        }
    }

    fn save_search_state(&mut self) {
        if self.search_mode {
            self.search_history.insert(
                self.current_dir.clone(),
                (self.search_query.clone(), self.search_results.clone()),
            );
        }
    }



    fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_focused = true;
        self.original_cursor = self.cursor;
        self.original_scroll_offset = self.scroll_offset;
        
        // Always start with a fresh search - clear any previous search state
        self.search_query.clear();
        self.search_results.clear();
        // Initialize search results with current directory entries
        self.update_search();
        
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    fn exit_search_mode(&mut self) {
        // Clear search state when exiting with ESC to allow fresh search
        self.search_mode = false;
        self.search_focused = false;
        self.search_query.clear();
        // Re-read the directory entries
        if let Ok(entries) = read_dir_sorted(&self.current_dir) {
            self.entries = entries;
        }
        self.cursor = self.original_cursor;
        self.scroll_offset = self.original_scroll_offset;
        self.search_results.clear();
        
        // Remove the saved search state for this directory to ensure fresh search
        self.search_history.remove(&self.current_dir);
    }

    fn update_search(&mut self) {
        if self.search_query.is_empty() {
            // When search query is empty, show current directory entries
            self.search_results.clear();
            for entry in &self.entries {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
                
                self.search_results.push(SearchResult {
                    path: path.to_path_buf(),
                    display_name: file_name,
                    is_dir,
                });
            }
            self.cursor = 0;
            self.scroll_offset = 0;
            return;
        }

        let query = self.search_query.to_lowercase();
        let mut results = Vec::new();

        // Search in current directory and all subdirectories
        let walker = walkdir::WalkDir::new(&self.current_dir).into_iter();
        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            
            if file_name.contains(&query) {
                let display_name = if path.starts_with(&self.current_dir) {
                    let relative_path = path.strip_prefix(&self.current_dir).unwrap_or(path);
                    if relative_path == std::path::Path::new(".") {
                        path.file_name().unwrap_or_default().to_string_lossy().to_string()
                    } else {
                        relative_path.to_string_lossy().to_string()
                    }
                } else {
                    path.to_string_lossy().to_string()
                };

                results.push(SearchResult {
                    path: path.to_path_buf(),
                    display_name,
                    is_dir: entry.file_type().is_dir(),
                });
            }
        }

        // Sort results: directories first, then by shortest string length, then alphabetically
        results.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir) // Directories first
            } else {
                // Sort by length first (shortest first), then alphabetically
                let len_cmp = a.display_name.len().cmp(&b.display_name.len());
                if len_cmp == std::cmp::Ordering::Equal {
                    a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())
                } else {
                    len_cmp
                }
            }
        });

        self.search_results = results;
        self.cursor = 0;
        self.scroll_offset = 0;
        
        // Save search state for current directory
        if self.search_mode {
            self.save_search_state();
        }
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
        ("/", "Search files"),
        ("c", "Confirm"),
        ("q/Ctrl-c", "Quit"),
    ];

    // Move is_under_selected here so it's accessible in both draw and event handler
    fn is_under_selected(selected: &HashSet<PathBuf>, deselected: &HashSet<PathBuf>, path: &std::path::Path) -> bool {
        selected.iter().any(|sel| {
            sel.is_dir() && path.starts_with(sel) && path != sel && !deselected.contains(path)
        })
    }

    // Function to get the final list of selected paths
    fn get_final_selected_paths(selected: &HashSet<PathBuf>, deselected: &HashSet<PathBuf>) -> Vec<String> {
        let mut final_paths = Vec::new();
        
        for path in selected {
            if !deselected.iter().any(|d| path.starts_with(d)) {
                if let Ok(metadata) = std::fs::metadata(path) {
                    if metadata.is_dir() {
                        // For directories, collect all files recursively
                        let entries: Vec<String> = walkdir::WalkDir::new(path)
                            .follow_links(true)
                            .into_iter()
                            .filter_map(|e| e.ok())
                            .filter(|e| e.path() != path) // Exclude the directory itself
                            .filter(|e| !deselected.iter().any(|d| e.path().starts_with(d))) // Exclude any path under a deselected path
                            .map(|e| e.path().to_string_lossy().to_string())
                            .collect();
                        final_paths.extend(entries);
                    } else {
                        // For files, add them directly
                        final_paths.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
        
        final_paths
    }

    fn handle_key_event(app: &mut AppState, key_event: KeyEvent, message: &mut String) -> Option<Vec<String>> {
        if key_event.kind != KeyEventKind::Press {
            return None;
        }

        // Handle search mode
        if app.search_mode {
            if app.search_focused {
                // Search box is focused - handle search input
                match key_event.code {
                    KeyCode::Esc => {
                        app.exit_search_mode();
                        return None;
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                        app.update_search();
                        return None;
                    }
                    KeyCode::Enter | KeyCode::Down | KeyCode::Up => {
                        // Exit search focus mode and enter navigation mode
                        app.search_focused = false;
                        return None;
                    }
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                        app.update_search();
                        return None;
                    }
                    _ => return None,
                }
            } else {
                // Search box is not focused - handle navigation and selection
                match key_event.code {
                    KeyCode::Char('q') => return Some(vec![]),
                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => return Some(vec![]),
                    KeyCode::Char('c') => {
                        if app.selected.is_empty() {
                            *message = "No files or directories selected!".to_string();
                        } else {
                            return Some(get_final_selected_paths(&app.selected, &app.deselected));
                        }
                    }
                    KeyCode::Char('/') => {
                        // Return to focused search mode to continue editing
                        app.search_focused = true;
                        return None;
                    }
                    KeyCode::Esc => {
                        app.exit_search_mode();
                        return None;
                    }
                    KeyCode::Enter => {
                        // Select the current search result or navigate into directory
                        if let Some(result) = app.search_results.get(app.cursor) {
                            if result.is_dir {
                                // Navigate into directory
                                let new_path = result.path.clone();
                                // Save current search state before navigating away
                                app.save_search_state();
                                app.save_directory_state();
                                app.current_dir = new_path;
                                app.entries = read_dir_sorted(&app.current_dir).unwrap_or_default();
                                // Don't restore search state - start fresh in new directory
                                app.search_mode = false;
                                app.search_focused = false;
                                app.search_query.clear();
                                app.search_results.clear();
                                app.reset_cursor();
                            } else {
                                // Select the file
                                let path = &result.path;
                                let is_dir = result.is_dir;
                                
                                if app.selected.contains(path) {
                                    app.selected.remove(path);
                                    if is_dir {
                                        app.deselected.retain(|p| !p.starts_with(path));
                                    }
                                } else if is_under_selected(&app.selected, &app.deselected, path) {
                                    if app.deselected.contains(path) {
                                        app.deselected.remove(path);
                                    } else {
                                        app.deselected.insert(path.clone());
                                    }
                                } else {
                                    app.selected.insert(path.clone());
                                }
                            }
                        }
                        return None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.cursor > 0 {
                            app.cursor -= 1;
                        }
                        return None;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.cursor + 1 < app.search_results.len() {
                            app.cursor += 1;
                        }
                        return None;
                    }
                    KeyCode::Char(' ') => {
                        // Allow space selection in search mode
                        if let Some(result) = app.search_results.get(app.cursor) {
                            let path = &result.path;
                            let is_dir = result.is_dir;
                            
                            if app.selected.contains(path) {
                                app.selected.remove(path);
                                if is_dir {
                                    app.deselected.retain(|p| !p.starts_with(path));
                                }
                            } else if is_under_selected(&app.selected, &app.deselected, path) {
                                if app.deselected.contains(path) {
                                    app.deselected.remove(path);
                                } else {
                                    app.deselected.insert(path.clone());
                                }
                            } else {
                                app.selected.insert(path.clone());
                            }
                        }
                        return None;
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        // Navigate into directory in search mode
                        if let Some(result) = app.search_results.get(app.cursor) {
                            if result.is_dir {
                                let new_path = result.path.clone();
                                // Save current search state before navigating away
                                app.save_search_state();
                                app.save_directory_state();
                                app.current_dir = new_path;
                                app.entries = read_dir_sorted(&app.current_dir).unwrap_or_default();
                                // Don't restore search state - start fresh in new directory
                                app.search_mode = false;
                                app.search_focused = false;
                                app.search_query.clear();
                                app.search_results.clear();
                                app.reset_cursor();
                            }
                        }
                        return None;
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        // Navigate to parent directory in search mode
                        if let Some(parent) = app.current_dir.parent() {
                            let parent_path = parent.to_path_buf();
                            // Save current search state before navigating away
                            app.save_search_state();
                            app.save_directory_state();
                            app.current_dir = parent_path;
                            app.entries = read_dir_sorted(&app.current_dir).unwrap_or_default();
                            // Don't restore search state - start fresh in parent directory
                            app.search_mode = false;
                            app.search_focused = false;
                            app.search_query.clear();
                            app.search_results.clear();
                            app.restore_directory_state();
                        }
                        return None;
                    }
                    _ => return None,
                }
            }
        }

        // Handle normal mode
        match key_event.code {
            KeyCode::Char('q') => return Some(vec![]),
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => return Some(vec![]),
            KeyCode::Char('c') => {
                if app.selected.is_empty() {
                    *message = "No files or directories selected!".to_string();
                } else {
                    return Some(get_final_selected_paths(&app.selected, &app.deselected));
                }
            }
            KeyCode::Char('/') => {
                app.enter_search_mode();
                return None;
            }
            KeyCode::Up | KeyCode::Char('k')    => app.move_cursor_up(),
            KeyCode::Down | KeyCode::Char('j')  => app.move_cursor_down(),
            KeyCode::Char(' ') => {
                if let Some(entry) = app.entries.get(app.cursor) {
                    let path = entry.path();
                    let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
                    if app.selected.contains(&path) {
                        app.selected.remove(&path);
                        if is_dir {
                            app.deselected.retain(|p| !p.starts_with(&path));
                        }
                    } else if is_under_selected(&app.selected, &app.deselected, &path) {
                        if app.deselected.contains(&path) {
                            app.deselected.remove(&path);
                        } else {
                            app.deselected.insert(path);
                        }
                    } else {
                        app.selected.insert(path);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                if let Some(entry) = app.entries.get(app.cursor) {
                    if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                        let new_path = entry.path();
                        // Save current directory state before navigating away
                        app.save_directory_state();
                        app.save_search_state();
                        app.current_dir = new_path;
                        app.entries = read_dir_sorted(&app.current_dir).unwrap_or_default();
                        // Don't restore search state - start fresh in new directory
                        app.search_mode = false;
                        app.search_focused = false;
                        app.search_query.clear();
                        app.search_results.clear();
                        // For entering a new directory, reset to top
                        app.reset_cursor();
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                if let Some(parent) = app.current_dir.parent() {
                    let parent_path = parent.to_path_buf();
                    // Save current directory state before navigating away
                    app.save_directory_state();
                    app.save_search_state();
                    app.current_dir = parent_path;
                    app.entries = read_dir_sorted(&app.current_dir).unwrap_or_default();
                    // For going back to parent, restore previous state if available
                    app.restore_directory_state();
                    // Don't restore search state - start fresh in parent directory
                    app.search_mode = false;
                    app.search_focused = false;
                    app.search_query.clear();
                    app.search_results.clear();
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
        None
    }

    loop {
        terminal.draw(|f| {
            // Build help lines
            let max_width = f.size().width.saturating_sub(6) as usize;
            let mut help_lines = vec![];
            let mut spans = vec![];
            let mut current_width = 0;
            let extra_help = if app.search_mode {
                if app.search_focused {
                    vec![
                        ("Esc", "Leave search"),
                        ("Enter/↑/↓", "Search"),
                    ]
                } else {
                    vec![
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
                vec![
                    ("r", "Toggle relative path"),
                    ("n", "Toggle no path headers"),
                ]
            };
            let help_items_to_show: Vec<&(&str, &str)> = if app.search_mode {
                extra_help.iter().collect()
            } else {
                help_items.iter().chain(extra_help.iter()).collect()
            };
            for (i, (key, desc)) in help_items_to_show.iter().enumerate() {
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
            let (path, title_str, path_style) = if app.search_mode {
                let search_display = format!("Search: {}", app.search_query);
                let title = if app.search_focused {
                    "Enter to search, Esc to leave search".to_string()
                } else {
                    "Esc to leave search".to_string()
                };
                let style = if app.search_focused {
                    Style::default().fg(Color::Yellow).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                (search_display, title, style)
            } else {
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
                (path, title_str, Style::default())
            };
            let current_dir_title = Span::styled(
                title_str,
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            );
            let path_widget = Paragraph::new(path)
                .block(Block::default().borders(Borders::ALL).title(current_dir_title))
                .style(path_style)
                .wrap(Wrap { trim: true });

            // Build the file list
            let (visible_items, files_title) = if app.search_mode {
                let items: Vec<ListItem> = app.search_results
                    .iter()
                    .enumerate()
                    .skip(app.scroll_offset)
                    .take(inner_list_height)
                    .map(|(i, result)| {
                        let mut style = Style::default();
                        let mut text = result.display_name.clone();
                        if result.is_dir {
                            style = style.fg(Color::Blue);
                            if !text.ends_with('/') {
                                text.push('/');
                            }
                        }
                        let is_selected = (app.selected.contains(&result.path) && !app.deselected.contains(&result.path)) || is_under_selected(&app.selected, &app.deselected, &result.path);
                        if is_selected {
                            style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
                        }
                        if i == app.cursor {
                            if is_selected && result.is_dir {
                                style = style.fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::REVERSED | Modifier::BOLD);
                            } else {
                                style = style.fg(Color::Yellow).add_modifier(Modifier::REVERSED);
                            }
                        }
                        ListItem::new(text).style(style)
                    })
                    .collect();
                let title = Span::styled(
                    format!("Search Results ({} found)", app.search_results.len()),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                );
                (items, title)
            } else {
                let items: Vec<ListItem> = app.entries
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
                        let is_selected = (app.selected.contains(&path) && !app.deselected.contains(&path)) || is_under_selected(&app.selected, &app.deselected, &path);
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
                let title = Span::styled(
                    "Files (Space: select, Enter/l/→: open, Backspace/h/←: up)",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                );
                (items, title)
            };
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
            if let Event::Key(key_event) = event::read()? {
                if let Some(result) = handle_key_event(&mut app, key_event, &mut message) {
                    return Ok(result);
                }
            }
        }
    }
}
