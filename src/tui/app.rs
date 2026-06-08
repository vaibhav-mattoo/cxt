use fuzzy_matcher::FuzzyMatcher;
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
    pub match_score: i64,
    pub match_indices: Vec<usize>,
}

pub struct AppState {
    pub root_dir: PathBuf,
    pub tree_state: tui_tree_widget::TreeState<PathBuf>,
    pub dir_cache: HashMap<PathBuf, Vec<fs::DirEntry>>,
    pub root_history: Vec<PathBuf>,
    pub selected: HashSet<PathBuf>,
    pub deselected: HashSet<PathBuf>,
    pub relative: bool,
    pub no_path: bool,
    pub search_history: HashMap<PathBuf, (String, Vec<SearchResult>)>,
    pub mode: AppMode,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_cursor: usize,
    pub search_scroll_offset: usize,
    pub visible_height: usize,
    pub original_cursor: usize,
    pub original_scroll_offset: usize,
    token_estimate_cache: Option<usize>,
    matcher: fuzzy_matcher::skim::SkimMatcherV2,
}

impl AppState {
    pub fn new() -> io::Result<Self> {
        let root_dir = env::current_dir()?;
        let mut dir_cache = HashMap::new();
        let root_entries = read_dir_sorted(&root_dir)?;

        // Eagerly load one level of subdirs so ▶ indicators appear immediately.
        let subdirs: Vec<PathBuf> = root_entries
            .iter()
            .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        dir_cache.insert(root_dir.clone(), root_entries);
        for subdir in subdirs {
            if let Ok(sub_entries) = read_dir_sorted(&subdir) {
                dir_cache.insert(subdir, sub_entries);
            }
        }

        let mut app = Self {
            root_dir,
            tree_state: tui_tree_widget::TreeState::default(),
            dir_cache,
            root_history: Vec::new(),
            selected: HashSet::new(),
            deselected: HashSet::new(),
            relative: false,
            no_path: false,
            search_history: HashMap::new(),
            mode: AppMode::Normal,
            search_query: String::new(),
            search_results: Vec::new(),
            search_cursor: 0,
            search_scroll_offset: 0,
            visible_height: 10,
            original_cursor: 0,
            original_scroll_offset: 0,
            token_estimate_cache: None,
            matcher: fuzzy_matcher::skim::SkimMatcherV2::default(),
        };
        app.select_first_entry();
        Ok(app)
    }

    /// Select the first entry in the current root directory.
    pub fn select_first_entry(&mut self) {
        if let Some(entries) = self.dir_cache.get(&self.root_dir) {
            if let Some(first) = entries.first() {
                self.tree_state.select(vec![first.path()]);
            }
        }
    }

    pub fn save_search_state(&mut self) {
        if self.mode != AppMode::Normal {
            self.search_history.insert(
                self.root_dir.clone(),
                (self.search_query.clone(), self.search_results.clone()),
            );
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

    /// Returns the path currently highlighted in the tree cursor.
    pub fn highlighted_path(&self) -> Option<PathBuf> {
        self.tree_state.selected().last().cloned()
    }
}

// NavigationExt
impl AppState {
    /// Load `dir`'s direct children into the cache if not already present.
    /// Also eagerly loads one level of subdirectories for ▶ indicators.
    pub fn ensure_dir_loaded(&mut self, dir: &PathBuf) {
        if self.dir_cache.contains_key(dir) {
            return;
        }
        let Ok(entries) = read_dir_sorted(dir) else {
            return;
        };
        let subdirs: Vec<PathBuf> = entries
            .iter()
            .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        self.dir_cache.insert(dir.clone(), entries);
        for subdir in subdirs {
            if !self.dir_cache.contains_key(&subdir) {
                if let Ok(sub_entries) = read_dir_sorted(&subdir) {
                    self.dir_cache.insert(subdir, sub_entries);
                }
            }
        }
    }

    /// Change the tree root to the parent directory of root_dir.
    pub fn go_up_root(&mut self) {
        self.token_estimate_cache = None;
        if let Some(parent) = self.root_dir.parent() {
            let old_root = self.root_dir.clone();
            let parent_path = parent.to_path_buf();
            self.root_history.push(self.root_dir.clone());
            self.root_dir = parent_path.clone();
            self.ensure_dir_loaded(&parent_path);
            self.tree_state = tui_tree_widget::TreeState::default();
            // Restore cursor to the directory we just backed out of.
            self.tree_state.select(vec![old_root]);
            self.mode = AppMode::Normal;
            self.search_query.clear();
            self.search_results.clear();
        }
    }

    /// Navigate into a directory from search mode (sets root_dir, resets tree).
    pub fn navigate_to_dir(&mut self, path: PathBuf) {
        self.token_estimate_cache = None;
        self.root_dir = path.clone();
        self.ensure_dir_loaded(&path);
        self.tree_state = tui_tree_widget::TreeState::default();
        self.mode = AppMode::Normal;
        self.search_query.clear();
        self.search_results.clear();
        self.select_first_entry();
    }
}

// SearchExt
impl AppState {
    pub fn enter_search(&mut self) {
        self.mode = AppMode::SearchFocused;
        self.original_cursor = self.search_cursor;
        self.original_scroll_offset = self.search_scroll_offset;
        self.search_query.clear();
        self.search_results.clear();
        self.update_search();
        self.search_cursor = 0;
        self.search_scroll_offset = 0;
    }

    pub fn exit_search(&mut self) {
        self.mode = AppMode::Normal;
        self.search_query.clear();
        self.search_results.clear();
        self.search_cursor = 0;
        self.search_scroll_offset = 0;
        self.search_history.remove(&self.root_dir);
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
            if let Some(entries) = self.dir_cache.get(&self.root_dir) {
                for entry in entries {
                    let path = entry.path();
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
                    self.search_results.push(SearchResult {
                        path,
                        display_name: file_name,
                        is_dir,
                        match_score: 0,
                        match_indices: vec![],
                    });
                }
            }
            self.search_cursor = 0;
            self.search_scroll_offset = 0;
            return;
        }

        let mut results = Vec::new();

        let walker = ignore::WalkBuilder::new(&self.root_dir)
            .hidden(false)
            .git_ignore(false)
            .follow_links(false)
            .build();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            let display_name = if path.starts_with(&self.root_dir) {
                let rel = path.strip_prefix(&self.root_dir).unwrap_or(path);
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

            if let Some((score, indices)) =
                self.matcher.fuzzy_indices(&display_name, &self.search_query)
            {
                results.push(SearchResult {
                    path: path.to_path_buf(),
                    display_name,
                    is_dir: entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false),
                    match_score: score,
                    match_indices: indices,
                });
            }
        }

        results.sort_by(|a, b| {
            b.match_score
                .cmp(&a.match_score)
                .then_with(|| b.is_dir.cmp(&a.is_dir))
                .then_with(|| a.display_name.len().cmp(&b.display_name.len()))
                .then_with(|| {
                    a.display_name
                        .to_lowercase()
                        .cmp(&b.display_name.to_lowercase())
                })
        });

        self.search_results = results;
        self.search_cursor = 0;
        self.search_scroll_offset = 0;
        if self.mode != AppMode::Normal {
            self.save_search_state();
        }
    }
}

// ScrollExt (search mode only — tree widget self-manages scrolling)
impl AppState {
    pub fn sync_search_scroll(&mut self, visible_height: usize) {
        self.visible_height = visible_height;
        let len = self.search_results.len();
        if self.search_cursor >= len {
            self.search_cursor = len.saturating_sub(1);
        }
        if len <= visible_height {
            self.search_scroll_offset = 0;
            return;
        }
        let top_margin = 2;
        let bottom_margin = 2;
        if self.search_cursor < self.search_scroll_offset + top_margin
            && self.search_scroll_offset > 0
        {
            self.search_scroll_offset = self.search_cursor.saturating_sub(top_margin);
        } else if self.search_cursor + bottom_margin
            >= self.search_scroll_offset + visible_height
            && self.search_scroll_offset + visible_height < len
        {
            self.search_scroll_offset =
                (self.search_cursor + bottom_margin + 1).saturating_sub(visible_height);
        }
        self.search_scroll_offset = self
            .search_scroll_offset
            .min(len.saturating_sub(visible_height));
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
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
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
