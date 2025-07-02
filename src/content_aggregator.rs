use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::path_formatter::PathFormatter;

pub struct ContentAggregator {
    path_formatter: PathFormatter,
    include_headers: bool,
    file_count: usize,
}

impl ContentAggregator {
    pub fn new(use_relative: bool, no_path: bool) -> Self {
        Self {
            path_formatter: PathFormatter::new(use_relative),
            include_headers: !no_path,
            file_count: 0,
        }
    }

    /// Aggregate content from multiple paths
    pub fn aggregate_paths(&mut self, paths: &[String]) -> Result<String> {
        let mut content = String::new();
        
        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(anyhow::anyhow!("Path does not exist: {}", path_str));
            }
            
            if path.is_file() {
                self.aggregate_file(path, &mut content)?;
            } else if path.is_dir() {
                self.aggregate_directory(path, &mut content)?;
            }
        }
        
        Ok(content)
    }

    /// Aggregate content from a single file
    fn aggregate_file(&mut self, path: &Path, content: &mut String) -> Result<()> {
        let file_content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        if self.include_headers {
            content.push_str(&self.path_formatter.create_header(path));
        }
        
        content.push_str(&file_content);
        content.push('\n');
        
        self.file_count += 1;
        Ok(())
    }

    /// Aggregate content from a directory recursively
    fn aggregate_directory(&mut self, dir_path: &Path, content: &mut String) -> Result<()> {
        for entry in WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            // Skip directories and hidden files
            if path.is_dir() || self.is_hidden_file(path) {
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

        let mut aggregator = ContentAggregator::new(false, false);
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

        let mut aggregator = ContentAggregator::new(false, true);
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

        let mut aggregator = ContentAggregator::new(false, false);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("File 1 content"));
        assert!(content.contains("File 2 content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_aggregate_nonexistent_path() {
        let mut aggregator = ContentAggregator::new(false, false);
        let result = aggregator.aggregate_paths(&["nonexistent_file.txt".to_string()]);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_skip_hidden_files() {
        let dir = tempdir().unwrap();
        let visible_file = dir.path().join("visible.txt");
        let hidden_file = dir.path().join(".hidden.txt");
        
        fs::write(&visible_file, "Visible content").unwrap();
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Visible content"));
        assert!(!content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 1);
    }
} 