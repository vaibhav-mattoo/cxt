use std::path::Path;

pub struct PathFormatter {
    relative: bool,
    no_path: bool,
}

impl PathFormatter {
    pub fn new(relative: bool, no_path: bool) -> Self {
        Self { relative, no_path }
    }

    /// Format a path for display in the output
    pub fn format_path(&self, path: &Path) -> String {
        if self.no_path {
            return String::new();
        }

        let formatted_path = if self.relative {
            self.get_relative_path(path)
        } else {
            self.get_absolute_path(path)
        };

        format!("--- File: {formatted_path} ---\n")
    }

    /// Get the absolute path as a string
    fn get_absolute_path(&self, path: &Path) -> String {
        match path.canonicalize() {
            Ok(canonical_path) => canonical_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    }

    /// Get the relative path from the current working directory
    fn get_relative_path(&self, path: &Path) -> String {
        match std::env::current_dir() {
            Ok(current_dir) => {
                match pathdiff::diff_paths(path, &current_dir) {
                    Some(relative_path) => relative_path.display().to_string(),
                    None => path.display().to_string(),
                }
            }
            Err(_) => path.display().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_format_path_no_path() {
        let formatter = PathFormatter::new(false, true);
        let path = Path::new("/some/path/file.txt");
        assert_eq!(formatter.format_path(path), "");
    }

    #[test]
    fn test_format_path_absolute() {
        let formatter = PathFormatter::new(false, false);
        let path = Path::new("/some/path/file.txt");
        let result = formatter.format_path(path);
        assert!(result.contains("--- File:"));
        assert!(result.contains("file.txt"));
    }

    #[test]
    fn test_format_path_relative() {
        let formatter = PathFormatter::new(true, false);
        let path = Path::new("file.txt");
        let result = formatter.format_path(path);
        assert!(result.contains("--- File:"));
        assert!(result.contains("file.txt"));
    }

    #[test]
    fn test_format_path_with_temp_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();

        let formatter = PathFormatter::new(false, false);
        let result = formatter.format_path(&file_path);
        
        assert!(result.contains("--- File:"));
        assert!(result.contains("test.txt"));
    }
} 