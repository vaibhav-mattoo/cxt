use fuzzy_matcher::FuzzyMatcher;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    SearchFocused,
    SearchNavigating,
    GitTree,
}

#[derive(Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub display_name: String,
    pub is_dir: bool,
    pub match_score: i64,
    pub match_indices: Vec<usize>,
}

#[derive(Clone)]
pub struct GitCommit {
    pub display: String,
    pub hash: String,
}

/// Detect whether `dir` is inside a git work-tree.
fn is_git_repo(dir: &Path) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Clone)]
pub struct DirItem {
    path: PathBuf,
    file_name: std::ffi::OsString,
    is_dir: bool,
}
impl DirItem {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
    pub fn file_name(&self) -> &std::ffi::OsStr {
        &self.file_name
    }
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }
}

pub struct AppState {
    pub root_dir: PathBuf,
    pub tree_state: tui_tree_widget::TreeState<PathBuf>,
    pub dir_cache: HashMap<PathBuf, Vec<DirItem>>,
    pub root_history: Vec<PathBuf>,
    pub selected: HashSet<PathBuf>,
    pub relative: bool,
    pub no_path: bool,
    pub respect_gitignore: bool,
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
    pub list_area: Option<ratatui::layout::Rect>,
    selected_file_count_cache: Option<usize>,
    selected_loc_cache: Option<u64>,
    pub git_commits: Vec<GitCommit>,
    pub git_commit_cursor: usize,
    pub git_commit_scroll_offset: usize,
    pub git_files: Vec<String>,
    pub git_files_cursor: usize,
    pub git_files_scroll_offset: usize,
    pub git_panel_focused: bool,
    dir_select_cache: RefCell<HashMap<PathBuf, bool>>,
    dir_files_cache: RefCell<HashMap<PathBuf, Rc<Vec<PathBuf>>>>,
    matcher: fuzzy_matcher::skim::SkimMatcherV2,
}

impl AppState {
    pub fn new(relative: bool, no_path: bool) -> io::Result<Self> {
        let root_dir = env::current_dir()?;
        let respect_gitignore = is_git_repo(&root_dir);
        let mut dir_cache = HashMap::new();
        let root_entries = read_dir_sorted(&root_dir, respect_gitignore)?;

        // Eagerly load one level of subdirs so ▶ indicators appear immediately.
        let subdirs: Vec<PathBuf> = root_entries
            .iter()
            .filter(|e| e.is_dir())
            .map(|e| e.path())
            .collect();
        dir_cache.insert(root_dir.clone(), root_entries);
        for subdir in subdirs {
            if let Ok(sub_entries) = read_dir_sorted(&subdir, respect_gitignore) {
                dir_cache.insert(subdir, sub_entries);
            }
        }

        let mut app = Self {
            root_dir,
            tree_state: tui_tree_widget::TreeState::default(),
            dir_cache,
            root_history: Vec::new(),
            selected: HashSet::new(),
            relative,
            no_path,
            respect_gitignore,
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
            list_area: None,
            selected_file_count_cache: None,
            selected_loc_cache: None,
            git_commits: Vec::new(),
            git_commit_cursor: 0,
            git_commit_scroll_offset: 0,
            git_files: Vec::new(),
            git_files_cursor: 0,
            git_files_scroll_offset: 0,
            git_panel_focused: true,
            dir_select_cache: RefCell::new(HashMap::new()),
            dir_files_cache: RefCell::new(HashMap::new()),
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
        self.selected_loc_cache = None;
        self.dir_select_cache.get_mut().clear();
    }

    pub fn toggle_selection(&mut self, path: PathBuf, is_dir: bool) {
        self.invalidate_caches();
        if is_dir {
            let files = files_under(&path, self.respect_gitignore);
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

    fn files_under_cached(&self, dir: &Path) -> Rc<Vec<PathBuf>> {
        if let Some(v) = self.dir_files_cache.borrow().get(dir) {
            return Rc::clone(v);
        }
        let files = Rc::new(files_under(dir, self.respect_gitignore));
        self.dir_files_cache
            .borrow_mut()
            .insert(dir.to_path_buf(), Rc::clone(&files));
        files
    }

    /// True iff dir has at least one descendant file and ALL are selected.
    /// Result is cached until the next selection change.
    pub fn dir_fully_selected(&self, dir: &Path) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        if let Some(v) = self.dir_select_cache.borrow().get(dir).copied() {
            return v;
        }
        let has_selected_descendant = self.selected.iter().any(|s| s.starts_with(dir));
        let result = if !has_selected_descendant {
            false
        } else {
            let files = self.files_under_cached(dir);
            !files.is_empty() && files.iter().all(|f| self.selected.contains(f))
        };
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

    pub fn selected_loc(&mut self) -> u64 {
        if let Some(cached) = self.selected_loc_cache {
            return cached;
        }
        let mut loc: u64 = 0;
        for path in &self.selected {
            if let Ok(bytes) = fs::read(path) {
                loc += bytes.iter().filter(|&&b| b == b'\n').count() as u64;
                if !bytes.is_empty() && *bytes.last().unwrap() != b'\n' {
                    loc += 1;
                }
            }
        }
        self.selected_loc_cache = Some(loc);
        loc
    }

    /// Returns the path currently highlighted in the tree cursor.
    pub fn highlighted_path(&self) -> Option<PathBuf> {
        self.tree_state.selected().last().cloned()
    }

    pub fn enter_git_tree_mode(&mut self) {
        if let Ok(output) = std::process::Command::new("git")
            .args(["log", "--graph", "--pretty=format:%H%x00%s"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.git_commits = stdout
                    .lines()
                    .map(|line| {
                        let mut parts = line.splitn(2, '\0');
                        let graph_hash = parts.next().unwrap_or("");
                        let message = parts.next().unwrap_or("");
                        
                        let long_hash = graph_hash
                            .split_whitespace()
                            .find(|s| s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() >= 6)
                            .unwrap_or("");
                        
                        let hash = if long_hash.len() >= 6 { long_hash[..6].to_string() } else { String::new() };
                        
                        let display_graph = if !long_hash.is_empty() {
                            graph_hash.replacen(long_hash, &hash, 1)
                        } else {
                            graph_hash.to_string()
                        };
                        
                        let display = if message.is_empty() {
                            display_graph
                        } else {
                            format!("{} {}", display_graph, message)
                        };
                        
                        GitCommit { display, hash }
                    })
                    .collect();
            } else {
                self.git_commits = vec![GitCommit { 
                    display: "Failed to load git log. Not a git repository?".to_string(), 
                    hash: String::new() 
                }];
            }
        } else {
            self.git_commits = vec![GitCommit { 
                display: "Failed to execute git command.".to_string(), 
                hash: String::new() 
            }];
        }
        
        self.git_commit_cursor = 0;
        self.git_files_cursor = 0;
        self.git_commit_scroll_offset = 0;
        self.git_files_scroll_offset = 0;
        self.git_panel_focused = true;
        self.fetch_git_files();
        self.mode = AppMode::GitTree;
    }

    pub fn fetch_git_files(&mut self) {
        if let Some(commit) = self.git_commits.get(self.git_commit_cursor) {
            if !commit.hash.is_empty() {
                if let Ok(output) = std::process::Command::new("git")
                    .args(["diff-tree", "--no-commit-id", "--name-only", "-r", "--root", &commit.hash])
                    .output()
                {
                    if output.status.success() {
                        self.git_files = String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(String::from)
                            .collect();
                        self.git_files_cursor = 0;
                        return;
                    }
                }
            }
        }
        self.git_files.clear();
        self.git_files_cursor = 0;
    }

    fn git_file_abs_path(&self, file: &str) -> PathBuf {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(file)
    }
    pub fn is_git_file_selected(&self, file: &str) -> bool {
        self.selected.contains(&self.git_file_abs_path(file))
    }
    pub fn toggle_git_file_selection(&mut self) {
        if let Some(file) = self.git_files.get(self.git_files_cursor).cloned() {
            let path = self.git_file_abs_path(&file);
            self.invalidate_caches();
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
        }
    }
    pub fn toggle_git_commit_selection(&mut self) {
        if self.git_files.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = self
            .git_files
            .iter()
            .map(|f| self.git_file_abs_path(f))
            .collect();
        let all_selected = paths.iter().all(|p| self.selected.contains(p));
        self.invalidate_caches();
        if all_selected {
            for p in paths {
                self.selected.remove(&p);
            }
        } else {
            for p in paths {
                self.selected.insert(p);
            }
        }
    }
    pub fn sync_git_scroll(&mut self, visible_height: usize) {
        if self.git_panel_focused {
            let len = self.git_commits.len();
            if self.git_commit_cursor >= len {
                self.git_commit_cursor = len.saturating_sub(1);
            }
            if len <= visible_height {
                self.git_commit_scroll_offset = 0;
                return;
            }
            if self.git_commit_cursor < self.git_commit_scroll_offset {
                self.git_commit_scroll_offset = self.git_commit_cursor;
            } else if self.git_commit_cursor >= self.git_commit_scroll_offset + visible_height {
                self.git_commit_scroll_offset = self.git_commit_cursor + 1 - visible_height;
            }
            self.git_commit_scroll_offset = self.git_commit_scroll_offset.min(len.saturating_sub(visible_height));
        } else {
            let len = self.git_files.len();
            if self.git_files_cursor >= len {
                self.git_files_cursor = len.saturating_sub(1);
            }
            if len <= visible_height {
                self.git_files_scroll_offset = 0;
                return;
            }
            if self.git_files_cursor < self.git_files_scroll_offset {
                self.git_files_scroll_offset = self.git_files_cursor;
            } else if self.git_files_cursor >= self.git_files_scroll_offset + visible_height {
                self.git_files_scroll_offset = self.git_files_cursor + 1 - visible_height;
            }
            self.git_files_scroll_offset = self.git_files_scroll_offset.min(len.saturating_sub(visible_height));
        }
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
        let Ok(entries) = read_dir_sorted(dir, self.respect_gitignore) else {
            return;
        };
        let subdirs: Vec<PathBuf> = entries
            .iter()
            .filter(|e| e.is_dir())
            .map(|e| e.path())
            .collect();
        self.dir_cache.insert(dir.clone(), entries);
        for subdir in subdirs {
            if let std::collections::hash_map::Entry::Vacant(e) = self.dir_cache.entry(subdir) {
                if let Ok(sub_entries) =
                    read_dir_sorted(e.key(), self.respect_gitignore)
                {
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
                    let is_dir = entry.is_dir();
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
            .git_ignore(self.respect_gitignore)
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
pub fn files_under(dir: &Path, respect_gitignore: bool) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(respect_gitignore)
        .follow_links(true)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect()
}

pub fn read_dir_sorted(dir: &PathBuf, respect_gitignore: bool) -> io::Result<Vec<DirItem>> {
    let mut entries: Vec<DirItem> = ignore::WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(respect_gitignore)
        .follow_links(true)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.depth() > 0)
        .map(|e| {
            let is_dir = e.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            DirItem {
                path: e.path().to_path_buf(),
                file_name: e.file_name().to_os_string(),
                is_dir,
            }
        })
        .collect();
    entries.sort_by_key(|e| (!e.is_dir(), e.file_name().to_os_string()));
    Ok(entries)
}