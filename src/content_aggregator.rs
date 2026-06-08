use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Files larger than this use byte estimation instead of exact BPE counting.
const MAX_EXACT_BYTES: u64 = 5 * 1024 * 1024; // 5 MB

pub struct ContentAggregator {
    formatter: Box<dyn crate::formatter::Formatter>,
    include_hidden_in_dirs: bool,
    file_count: usize,
    token_count: usize,
    token_counter: crate::token_counter::TokenCounter,
    ignore: Vec<PathBuf>,
    sort: bool,
    /// Extensions to include. Empty means all files are allowed.
    allowed_extensions: std::collections::HashSet<String>,
}

impl ContentAggregator {
    pub fn new(
        formatter: Box<dyn crate::formatter::Formatter>,
        include_hidden_in_dirs: bool,
        ignore: Vec<String>,
        sort: bool,
        allowed_extensions: std::collections::HashSet<String>,
    ) -> Self {
        Self {
            formatter,
            include_hidden_in_dirs,
            file_count: 0,
            token_count: 0,
            token_counter: crate::token_counter::TokenCounter::new(),
            ignore: ignore.into_iter().map(PathBuf::from).collect(),
            sort,
            allowed_extensions,
        }
    }

    fn is_ignored(&self, path: &Path) -> bool {
        self.ignore
            .iter()
            .any(|p| path == p || path.starts_with(p))
    }

    /// Returns true if `path` passes the extension filter.
    /// When `allowed_extensions` is empty, all files pass.
    fn extension_allowed(&self, path: &Path) -> bool {
        if self.allowed_extensions.is_empty() {
            return true;
        }
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => self.allowed_extensions.contains(&ext.to_lowercase()),
            None => false,
        }
    }

    pub fn aggregate_paths<W: Write>(&mut self, paths: &[String], writer: &mut W) -> Result<()> {
        writer.write_all(self.formatter.document_start().as_bytes())?;
        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(anyhow::anyhow!("Path does not exist: {}", path_str));
            }
            if self.is_ignored(path) {
                continue;
            }
            if path.is_file() {
                self.aggregate_file(path, writer)?;
            } else if path.is_dir() {
                if !self.include_hidden_in_dirs
                    && self.is_hidden_file(path)
                    && !self.is_explicit_path(path, paths)
                {
                    continue;
                }
                self.aggregate_directory(path, writer)?;
            }
        }
        writer.write_all(self.formatter.document_end().as_bytes())?;
        Ok(())
    }

    fn is_explicit_path(&self, path: &Path, input_paths: &[String]) -> bool {
        input_paths.iter().any(|p| Path::new(p) == path)
    }

    /// Aggregate a single file; canonicalises path before passing to formatter.
    fn aggregate_file<W: Write>(&mut self, path: &Path, writer: &mut W) -> Result<()> {
        if !self.extension_allowed(path) {
            return Ok(());
        }
        let display_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.formatter.write_file_header(&display_path, writer)?;
        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if file_size <= MAX_EXACT_BYTES {
            let content = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Warning: Failed to read file '{}': {e}", path.display());
                    return Ok(());
                }
            };
            let text = String::from_utf8_lossy(&content);
            self.token_count += self.token_counter.count(&text);
            if let Err(e) = writer.write_all(&content) {
                eprintln!("Warning: Failed to write file '{}': {e}", path.display());
            }
        } else {
            self.token_count += crate::token_counter::estimate_from_bytes(file_size);
            let mut file = match fs::File::open(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Warning: Failed to read file '{}': {e}", path.display());
                    return Ok(());
                }
            };
            if let Err(e) = std::io::copy(&mut file, writer) {
                eprintln!("Warning: Failed to copy file '{}': {e}", path.display());
            }
        }
        writer.write_all(self.formatter.file_footer().as_bytes())?;
        self.file_count += 1;
        Ok(())
    }

    /// Like `aggregate_file` but skips `canonicalize()` — path is already canonical.
    /// Called from `aggregate_directory` which pre-canonicalises the base directory once.
    fn aggregate_file_precanon<W: Write>(&mut self, path: &Path, writer: &mut W) -> Result<()> {
        self.formatter.write_file_header(path, writer)?;
        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if file_size <= MAX_EXACT_BYTES {
            let content = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Warning: Failed to read file '{}': {e}", path.display());
                    return Ok(());
                }
            };
            let text = String::from_utf8_lossy(&content);
            self.token_count += self.token_counter.count(&text);
            if let Err(e) = writer.write_all(&content) {
                eprintln!("Warning: Failed to write file '{}': {e}", path.display());
            }
        } else {
            self.token_count += crate::token_counter::estimate_from_bytes(file_size);
            let mut file = match fs::File::open(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Warning: Failed to read file '{}': {e}", path.display());
                    return Ok(());
                }
            };
            if let Err(e) = std::io::copy(&mut file, writer) {
                eprintln!("Warning: Failed to copy file '{}': {e}", path.display());
            }
        }
        writer.write_all(self.formatter.file_footer().as_bytes())?;
        self.file_count += 1;
        Ok(())
    }

    /// Walk `dir_path` in parallel, collect file paths, sort for determinism, then
    /// stream each file through `aggregate_file_precanon`.
    fn aggregate_directory<W: Write>(&mut self, dir_path: &Path, writer: &mut W) -> Result<()> {
        use ignore::WalkBuilder;

        // Canonicalise once here; all paths returned by the walker are prefixed with
        // this canonical root, so per-file canonicalize() calls are unnecessary.
        let canon_dir = dir_path
            .canonicalize()
            .unwrap_or_else(|_| dir_path.to_path_buf());

        let ignore_list = self.ignore.clone();
        let allowed_ext = self.allowed_extensions.clone();
        let (tx, rx) = mpsc::channel::<PathBuf>();

        let walker = WalkBuilder::new(&canon_dir)
            .hidden(!self.include_hidden_in_dirs) // hidden(true) = skip dotfiles
            .git_ignore(true)
            .follow_links(true)
            .build_parallel();

        walker.run(|| {
            let tx = tx.clone();
            let ignore_list = ignore_list.clone();
            let allowed_ext = allowed_ext.clone();
            Box::new(move |result| {
                use ignore::WalkState;
                if let Ok(entry) = result {
                    let path = entry.path();
                    let ignored = ignore_list
                        .iter()
                        .any(|ig| path == ig || path.starts_with(ig));
                    let allowed = if allowed_ext.is_empty() {
                        true
                    } else {
                        path.extension()
                            .and_then(|e| e.to_str())
                            .map(|ext| allowed_ext.contains(&ext.to_lowercase()))
                            .unwrap_or(false)
                    };
                    if !ignored && entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) && allowed {
                        let _ = tx.send(path.to_path_buf());
                    }
                }
                WalkState::Continue
            })
        });
        drop(tx); // close the last sender so rx drains cleanly

        if self.sort {
            let mut file_paths: Vec<PathBuf> = rx.into_iter().collect();
            file_paths.sort_unstable();
            for path in file_paths {
                self.aggregate_file_precanon(&path, writer)?;
            }
        } else {
            for path in rx {
                self.aggregate_file_precanon(&path, writer)?;
            }
        }
        Ok(())
    }

    fn is_hidden_file(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }

    pub fn file_count(&self) -> usize {
        self.file_count
    }

    pub fn token_count(&self) -> usize {
        self.token_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::{build_formatter, FormatChoice};
    use std::fs;
    use tempfile::tempdir;

    fn xml_aggregator(no_path: bool) -> ContentAggregator {
        ContentAggregator::new(
            build_formatter(FormatChoice::Xml, no_path, false),
            false,
            vec![],
            true,
            std::collections::HashSet::new(),
        )
    }

    #[test]
    fn test_aggregate_single_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let mut aggregator = xml_aggregator(false);
        let mut buffer = Vec::new();
        aggregator
            .aggregate_paths(&[file_path.to_str().unwrap().to_string()], &mut buffer)
            .unwrap();
        let content = String::from_utf8(buffer).unwrap();

        assert!(content.contains("Hello, World!"));
        assert!(content.contains("<file path="));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_aggregate_file_without_headers() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let mut aggregator = xml_aggregator(true);
        let mut buffer = Vec::new();
        aggregator
            .aggregate_paths(&[file_path.to_str().unwrap().to_string()], &mut buffer)
            .unwrap();
        let content = String::from_utf8(buffer).unwrap();

        assert!(content.contains("Hello, World!"));
        assert!(content.contains("<file>"));
        assert!(!content.contains("path="));
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

        let mut aggregator = xml_aggregator(false);
        let mut buffer = Vec::new();
        aggregator
            .aggregate_paths(&[dir.path().to_str().unwrap().to_string()], &mut buffer)
            .unwrap();
        let content = String::from_utf8(buffer).unwrap();

        assert!(content.contains("File 1 content"));
        assert!(content.contains("File 2 content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_aggregate_nonexistent_path() {
        let mut aggregator = xml_aggregator(false);
        let mut buffer = Vec::new();
        let result =
            aggregator.aggregate_paths(&["nonexistent_file.txt".to_string()], &mut buffer);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path does not exist"));
    }

    #[test]
    fn test_skip_hidden_files_in_directory() {
        let dir = tempdir().unwrap();
        let visible_file = dir.path().join("visible.txt");
        let hidden_file = dir.path().join(".hidden.txt");

        fs::write(&visible_file, "Visible content").unwrap();
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = xml_aggregator(false);
        let mut buffer = Vec::new();
        aggregator
            .aggregate_paths(&[dir.path().to_str().unwrap().to_string()], &mut buffer)
            .unwrap();
        let content = String::from_utf8(buffer).unwrap();

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

        let mut aggregator = ContentAggregator::new(
            build_formatter(FormatChoice::Xml, false, false),
            true,
            vec![],
            true,
            std::collections::HashSet::new(),
        );
        let mut buffer = Vec::new();
        aggregator
            .aggregate_paths(&[dir.path().to_str().unwrap().to_string()], &mut buffer)
            .unwrap();
        let content = String::from_utf8(buffer).unwrap();

        assert!(content.contains("Visible content"));
        assert!(content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_always_read_hidden_file_when_explicitly_provided() {
        let dir = tempdir().unwrap();
        let hidden_file = dir.path().join(".hidden.txt");
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = xml_aggregator(false);
        let mut buffer = Vec::new();
        aggregator
            .aggregate_paths(&[hidden_file.to_str().unwrap().to_string()], &mut buffer)
            .unwrap();
        let content = String::from_utf8(buffer).unwrap();

        assert!(content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 1);
    }
}
