use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    path::PathBuf,
};

#[derive(Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    SearchFocused,
    SearchNavigating,
}

#[derive(Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub display_name: String,
    pub is_dir: bool,
}

pub struct AppState {
    pub current_dir: PathBuf,
    pub entries: Vec<fs::DirEntry>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub visible_height: usize,
    pub selected: HashSet<PathBuf>,
    pub deselected: HashSet<PathBuf>,
    pub relative: bool,
    pub no_path: bool,
    pub directory_history: HashMap<PathBuf, (usize, usize)>,
    pub search_history: HashMap<PathBuf, (String, Vec<SearchResult>)>,
    pub mode: AppMode,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub original_cursor: usize,
    pub original_scroll_offset: usize,
    /// Cached estimated token count for the current selection.
    /// None means the cache is invalid and must be recomputed.
    token_estimate_cache: Option<usize>,
}

impl AppState {
    pub fn new() -> io::Result<Self> {
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
            mode: AppMode::Normal,
            search_query: String::new(),
            search_results: Vec::new(),
            original_cursor: 0,
            original_scroll_offset: 0,
            token_estimate_cache: None,
        })
    }

    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    pub fn save_directory_state(&mut self) {
        self.directory_history
            .insert(self.current_dir.clone(), (self.cursor, self.scroll_offset));
    }

    pub fn restore_directory_state(&mut self) {
        if let Some(&(cursor, scroll_offset)) = self.directory_history.get(&self.current_dir) {
            self.cursor = cursor;
            self.scroll_offset = scroll_offset;
        } else {
            self.reset_cursor();
        }
    }

    pub fn save_search_state(&mut self) {
        if self.mode != AppMode::Normal {
            self.search_history.insert(
                self.current_dir.clone(),
                (self.search_query.clone(), self.search_results.clone()),
            );
        }
    }

    pub fn move_cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }
}

// SelectionExt
impl AppState {
    pub fn is_implicitly_selected(&self, path: &std::path::Path) -> bool {
        self.selected.iter().any(|sel| {
            sel.is_dir() && path.starts_with(sel) && path != sel && !self.deselected.contains(path)
        })
    }

    pub fn toggle_selection(&mut self, path: PathBuf, is_dir: bool) {
        self.token_estimate_cache = None;
        if self.selected.contains(&path) {
            self.selected.remove(&path);
            if is_dir {
                self.deselected.retain(|p| !p.starts_with(&path));
            }
        } else if self.is_implicitly_selected(&path) {
            if self.deselected.contains(&path) {
                self.deselected.remove(&path);
            } else {
                self.deselected.insert(path);
            }
        } else {
            self.selected.insert(path);
        }
    }

    /// Returns an estimated token count for the current selection.
    /// Uses file-size / 4 approximation — fast, no file reads.
    /// Result is cached until the selection or directory changes.
    pub fn estimated_tokens(&mut self) -> usize {
        if let Some(cached) = self.token_estimate_cache {
            return cached;
        }
        let estimate = compute_token_estimate(&self.selected, &self.deselected);
        self.token_estimate_cache = Some(estimate);
        estimate
    }

    pub fn collect_selected_paths(&self) -> Vec<String> {
        let mut final_paths = Vec::new();
        for path in &self.selected {
            if !self.deselected.iter().any(|d| path.starts_with(d)) {
                if let Ok(metadata) = std::fs::metadata(path) {
                    if metadata.is_dir() {
                        let entries: Vec<String> = ignore::WalkBuilder::new(path)
                            .hidden(false)
                            .git_ignore(false)
                            .follow_links(true)
                            .build()
                            .filter_map(|e| e.ok())
                            .filter(|e| e.path() != path)
                            .filter(|e| !self.deselected.iter().any(|d| e.path().starts_with(d)))
                            .map(|e| e.path().to_string_lossy().to_string())
                            .collect();
                        final_paths.extend(entries);
                    } else {
                        final_paths.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
        final_paths
    }
}

// NavigationExt
impl AppState {
    pub fn navigate_into(&mut self, new_path: PathBuf) {
        self.token_estimate_cache = None;
        self.save_directory_state();
        self.save_search_state();
        self.current_dir = new_path;
        self.entries = read_dir_sorted(&self.current_dir).unwrap_or_default();
        self.mode = AppMode::Normal;
        self.search_query.clear();
        self.search_results.clear();
        self.reset_cursor();
    }

    pub fn navigate_to_parent(&mut self) {
        self.token_estimate_cache = None;
        if let Some(parent) = self.current_dir.parent() {
            let parent_path = parent.to_path_buf();
            self.save_directory_state();
            self.save_search_state();
            self.current_dir = parent_path;
            self.entries = read_dir_sorted(&self.current_dir).unwrap_or_default();
            self.mode = AppMode::Normal;
            self.search_query.clear();
            self.search_results.clear();
            self.restore_directory_state();
        }
    }
}

// SearchExt
impl AppState {
    pub fn enter_search(&mut self) {
        self.mode = AppMode::SearchFocused;
        self.original_cursor = self.cursor;
        self.original_scroll_offset = self.scroll_offset;
        self.search_query.clear();
        self.search_results.clear();
        self.update_search();
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    pub fn exit_search(&mut self) {
        self.mode = AppMode::Normal;
        self.search_query.clear();
        if let Ok(entries) = read_dir_sorted(&self.current_dir) {
            self.entries = entries;
        }
        self.cursor = self.original_cursor;
        self.scroll_offset = self.original_scroll_offset;
        self.search_results.clear();
        self.search_history.remove(&self.current_dir);
    }

    pub fn push_search_char(&mut self, c: char) {
        self.search_query.push(c);
        self.update_search();
    }

    pub fn pop_search_char(&mut self) {
        self.search_query.pop();
        self.update_search();
    }

    pub fn update_search(&mut self) {
        if self.search_query.is_empty() {
            self.search_results.clear();
            for entry in &self.entries {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
                self.search_results.push(SearchResult {
                    path,
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

        for entry in ignore::WalkBuilder::new(&self.current_dir)
            .hidden(false)
            .git_ignore(false)
            .follow_links(false)
            .build()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if file_name.contains(&query) {
                let display_name = if path.starts_with(&self.current_dir) {
                    let rel = path.strip_prefix(&self.current_dir).unwrap_or(path);
                    if rel == std::path::Path::new(".") {
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    } else {
                        rel.to_string_lossy().to_string()
                    }
                } else {
                    path.to_string_lossy().to_string()
                };
                results.push(SearchResult {
                    path: path.to_path_buf(),
                    display_name,
                    is_dir: entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false),
                });
            }
        }

        results.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
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
        if self.mode != AppMode::Normal {
            self.save_search_state();
        }
    }
}

// ScrollExt
impl AppState {
    pub fn sync_scroll(&mut self, visible_height: usize) {
        self.visible_height = visible_height;

        let entries_len = if self.mode != AppMode::Normal {
            self.search_results.len()
        } else {
            self.entries.len()
        };

        if self.cursor >= entries_len {
            self.cursor = entries_len.saturating_sub(1);
        }

        if entries_len <= visible_height {
            self.scroll_offset = 0;
            return;
        }

        let top_margin = 2;
        let bottom_margin = 2;
        let visible_start = self.scroll_offset;
        let visible_end = self.scroll_offset + visible_height;

        if self.cursor < visible_start + top_margin && self.scroll_offset > 0 {
            self.scroll_offset = self.cursor.saturating_sub(top_margin);
        } else if self.cursor + bottom_margin >= visible_end
            && self.scroll_offset + visible_height < entries_len
        {
            self.scroll_offset =
                (self.cursor + bottom_margin + 1).saturating_sub(visible_height);
        }

        let max_scroll = entries_len.saturating_sub(visible_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }
}

pub fn read_dir_sorted(dir: &PathBuf) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| {
        let md = e.metadata();
        (!md.as_ref().map(|m| m.is_dir()).unwrap_or(false), e.file_name())
    });
    Ok(entries)
}

fn compute_token_estimate(
    selected: &std::collections::HashSet<std::path::PathBuf>,
    deselected: &std::collections::HashSet<std::path::PathBuf>,
) -> usize {
    let mut total: usize = 0;
    for path in selected {
        if deselected.iter().any(|d| path.starts_with(d)) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.is_dir() {
            let walker = ignore::WalkBuilder::new(path)
                .hidden(false)
                .git_ignore(false)
                .follow_links(true)
                .build();
            for entry in walker.filter_map(|e| e.ok()) {
                if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    let entry_path = entry.path();
                    if deselected.iter().any(|d| entry_path.starts_with(d)) {
                        continue;
                    }
                    if let Ok(entry_meta) = entry.metadata() {
                        total += crate::token_counter::estimate_from_bytes(entry_meta.len());
                    }
                }
            }
        } else {
            total += crate::token_counter::estimate_from_bytes(meta.len());
        }
    }
    total
}
