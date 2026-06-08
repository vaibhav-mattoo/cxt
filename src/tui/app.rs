use fuzzy_matcher::FuzzyMatcher;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
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
    pub relative: bool,
    pub no_path: bool,
    pub show_help: bool,
    pub search_history: HashMap<PathBuf, (String, Vec<SearchResult>)>,
    pub mode: AppMode,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_cursor: usize,
    pub search_scroll_offset: usize,
    pub visible_height: usize,
    pub original_cursor: usize,
    pub original_scroll_offset: usize,
    selected_file_count_cache: Option<usize>,
    dir_select_cache: RefCell<HashMap<PathBuf, bool>>,
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
            relative: false,
            no_path: false,
            show_help: false,
            search_history: HashMap::new(),
            mode: AppMode::Normal,
            search_query: String::new(),
            search_results: Vec::new(),
            search_cursor: 0,
            search_scroll_offset: 0,
            visible_height: 10,
            original_cursor: 0,
            original_scroll_offset: 0,
            selected_file_count_cache: None,
            dir_select_cache: RefCell::new(HashMap::new()),
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
    fn invalidate_caches(&mut self) {
        self.selected_file_count_cache = None;
        self.dir_select_cache.get_mut().clear();
    }

    pub fn toggle_selection(&mut self, path: PathBuf, is_dir: bool) {
        self.invalidate_caches();
        if is_dir {
            let files = files_under(&path);
            let all = !files.is_empty() && files.iter().all(|f| self.selected.contains(f));
            if all {
                for f in files {
                    self.selected.remove(&f);
                }
            } else {
                for f in files {
                    self.selected.insert(f);
                }
            }
        } else if !self.selected.remove(&path) {
            self.selected.insert(path);
        }
    }

    /// True iff dir has at least one descendant file and ALL are selected.
    /// Result is cached until the next selection change.
    pub fn dir_fully_selected(&self, dir: &Path) -> bool {
        let cached = self.dir_select_cache.borrow().get(dir).copied();
        if let Some(v) = cached {
            return v;
        }
        let files = files_under(dir);
        let result = !files.is_empty() && files.iter().all(|f| self.selected.contains(f));
        self.dir_select_cache
            .borrow_mut()
            .insert(dir.to_path_buf(), result);
        result
    }

    /// Render-facing selection test that handles files and directories uniformly.
    pub fn is_selected(&self, path: &Path, is_dir: bool) -> bool {
        if is_dir {
            self.dir_fully_selected(path)
        } else {
            self.selected.contains(path)
        }
    }

    pub fn collect_selected_paths(&self) -> Vec<String> {
        self.selected
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    }

    pub fn selected_file_count(&mut self) -> usize {
        if let Some(cached) = self.selected_file_count_cache {
            return cached;
        }
        let count = self.selected.len();
        self.selected_file_count_cache = Some(count);
        count
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
            if let std::collections::hash_map::Entry::Vacant(e) = self.dir_cache.entry(subdir) {
                if let Ok(sub_entries) = read_dir_sorted(e.key()) {
                    e.insert(sub_entries);
                }
            }
        }
    }

    /// Change the tree root to the parent directory of root_dir.
    pub fn go_up_root(&mut self) {
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

/// Returns all files under `dir` using the same walker settings as path collection.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(false)
        .follow_links(true)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect()
}

pub fn read_dir_sorted(dir: &PathBuf) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| {
        let md = e.metadata();
        (!md.as_ref().map(|m| m.is_dir()).unwrap_or(false), e.file_name())
    });
    Ok(entries)
}
