use anyhow::Result;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::path_formatter::PathFormatter;

pub struct ContentAggregator {
    path_formatter: PathFormatter,
    include_headers: bool,
    include_hidden_in_dirs: bool,
    file_count: usize,
    ignore: Vec<std::path::PathBuf>,
}

impl ContentAggregator {
    pub fn new(use_relative: bool, no_path: bool, include_hidden_in_dirs: bool, ignore: Vec<String>) -> Self {
        Self {
            path_formatter: PathFormatter::new(use_relative, no_path),
            include_headers: !no_path,
            include_hidden_in_dirs,
            file_count: 0,
            ignore: ignore.into_iter().map(std::path::PathBuf::from).collect(),
        }
    }

    /// Check if a path should be ignored
    fn is_ignored(&self, path: &Path) -> bool {
        self.ignore.iter().any(|ignore_path| {
            path == ignore_path || path.starts_with(ignore_path)
        })
    }

    /// Aggregate content from multiple paths
    pub fn aggregate_paths(&mut self, paths: &[String]) -> Result<String> {
        let mut content = String::new();
        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(anyhow::anyhow!("Path does not exist: {}", path_str));
            }
            if self.is_ignored(path) {
                continue;
            }
            if path.is_file() {
                self.aggregate_file(path, &mut content)?;
            } else if path.is_dir() {
                if !self.include_hidden_in_dirs && self.is_hidden_file(path) && !self.is_explicit_path(path, paths) {
                    continue;
                }
                self.aggregate_directory(path, &mut content)?;
            }
        }
        Ok(content)
    }

    /// Helper: check if a path is explicitly specified in the input paths
    fn is_explicit_path<'a>(&self, path: &Path, input_paths: &'a [String]) -> bool {
        input_paths.iter().any(|p| Path::new(p) == path)
    }

    /// Aggregate content from a single file
    fn aggregate_file(&mut self, path: &Path, content: &mut String) -> Result<()> {
        match fs::read_to_string(path) {
            Ok(file_content) => {
                if self.include_headers {
                    content.push_str(&self.path_formatter.format_path(path));
                }
                content.push_str(&file_content);
                content.push('\n');
                self.file_count += 1;
            },
            Err(e) => {
                eprintln!("Warning: Failed to read file '{}': {e}", path.display());
            }
        }
        Ok(())
    }

    /// Aggregate content from a directory recursively
    fn aggregate_directory(&mut self, dir_path: &Path, content: &mut String) -> Result<()> {
        let include_hidden = self.include_hidden_in_dirs;
        let ignore = self.ignore.clone();
        let is_hidden = |path: &Path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with('.'))
                .unwrap_or(false)
        };
        let is_ignored = |path: &Path| {
            ignore.iter().any(|ignore_path| path == ignore_path || path.starts_with(ignore_path))
        };
        let walker = WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|entry| {
                let path = entry.path();
                if is_ignored(path) {
                    return false;
                }
                if path == dir_path {
                    true
                } else if path.is_dir() && is_hidden(path) {
                    include_hidden
                } else {
                    true
                }
            });
        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if is_ignored(path) {
                continue;
            }
            if path.is_dir() {
                continue;
            }
            if !include_hidden && is_hidden(path) {
                continue;
            }
            self.aggregate_file(path, content)?;
        }
        Ok(())
    }

    /// Check if a file is hidden (starts with .)
    fn is_hidden_file(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }

    /// Get the total number of files processed
    pub fn file_count(&self) -> usize {
        self.file_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_aggregate_single_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[file_path.to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Hello, World!"));
        assert!(content.contains("--- File:"));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_aggregate_file_without_headers() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let mut aggregator = ContentAggregator::new(false, true, false, vec![]);
        let content = aggregator.aggregate_paths(&[file_path.to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Hello, World!"));
        assert!(!content.contains("--- File:"));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_aggregate_directory() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        
        let file1 = dir.path().join("file1.txt");
        let file2 = subdir.join("file2.txt");
        
        fs::write(&file1, "File 1 content").unwrap();
        fs::write(&file2, "File 2 content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("File 1 content"));
        assert!(content.contains("File 2 content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_aggregate_nonexistent_path() {
        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let result = aggregator.aggregate_paths(&["nonexistent_file.txt".to_string()]);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_skip_hidden_files_in_directory() {
        let dir = tempdir().unwrap();
        let visible_file = dir.path().join("visible.txt");
        let hidden_file = dir.path().join(".hidden.txt");
        
        fs::write(&visible_file, "Visible content").unwrap();
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Visible content"));
        assert!(!content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_include_hidden_files_in_directory_with_flag() {
        let dir = tempdir().unwrap();
        let visible_file = dir.path().join("visible.txt");
        let hidden_file = dir.path().join(".hidden.txt");
        
        fs::write(&visible_file, "Visible content").unwrap();
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, true, vec![]);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Visible content"));
        assert!(content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_always_read_hidden_file_when_explicitly_provided() {
        let dir = tempdir().unwrap();
        let hidden_file = dir.path().join(".hidden.txt");
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[hidden_file.to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 1);
    }
} 