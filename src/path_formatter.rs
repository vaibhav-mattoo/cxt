use std::path::{Path, PathBuf};

pub struct PathFormatter {
    use_relative: bool,
    current_dir: Option<PathBuf>,
}

impl PathFormatter {
    pub fn new(use_relative: bool) -> Self {
        let current_dir = if use_relative {
            std::env::current_dir().ok()
        } else {
            None
        };

        Self {
            use_relative,
            current_dir,
        }
    }

    /// Format a file path for display in headers
    pub fn format_path(&self, path: &Path) -> String {
        if self.use_relative {
            self.format_relative_path(path)
        } else {
            self.format_absolute_path(path)
        }
    }

    /// Format an absolute path
    fn format_absolute_path(&self, path: &Path) -> String {
        match path.canonicalize() {
            Ok(canonical_path) => canonical_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    }

    /// Format a relative path from current working directory
    fn format_relative_path(&self, path: &Path) -> String {
        if let Some(ref current_dir) = self.current_dir {
            if let Ok(relative_path) = path.strip_prefix(current_dir) {
                return relative_path.display().to_string();
            }
        }
        // Fallback to absolute path if relative path calculation fails
        self.format_absolute_path(path)
    }

    /// Create a file header with the formatted path
    pub fn create_header(&self, path: &Path) -> String {
        let formatted_path = self.format_path(path);
        format!("--- File: {} ---\n", formatted_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_absolute_path_formatting() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();
        let formatter = PathFormatter::new(false);
        let formatted = formatter.format_path(&file_path);
        assert!(formatted.contains("test.txt"));
    }

    #[test]
    fn test_relative_path_formatting() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();
        let old_cwd = env::current_dir().unwrap();
        env::set_current_dir(dir.path()).unwrap();
        let formatter = PathFormatter::new(true);
        let formatted = formatter.format_path(&file_path);
        assert_eq!(formatted, "test.txt");
        env::set_current_dir(old_cwd).unwrap();
    }
} 