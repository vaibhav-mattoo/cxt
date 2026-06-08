use std::io::Write;
use std::path::Path;

pub struct PathFormatter {
    relative: bool,
    no_path: bool,
    cwd: Option<std::path::PathBuf>,
}

impl PathFormatter {
    pub fn new(relative: bool, no_path: bool) -> Self {
        let cwd = if relative {
            std::env::current_dir().ok()
        } else {
            None
        };
        Self { relative, no_path, cwd }
    }

    /// Write `--- File: <path> ---\n` directly to `writer`. No-op when `no_path` is set.
    /// Calls `canonicalize()` for absolute paths to resolve symlinks and `.`/`..` components.
    pub fn write_header<W: Write>(&self, path: &Path, writer: &mut W) -> std::io::Result<()> {
        if self.no_path {
            return Ok(());
        }
        write!(writer, "--- File: ")?;
        if self.relative {
            self.write_relative_path(path, writer)?;
        } else {
            match path.canonicalize() {
                Ok(canon) => write!(writer, "{}", canon.display())?,
                Err(_) => write!(writer, "{}", path.display())?,
            }
        }
        writeln!(writer, " ---")
    }

    /// Like `write_header` but skips `canonicalize()` for absolute paths.
    /// Use when the caller has already ensured the path is canonical (e.g. after a
    /// single `dir_path.canonicalize()` + join in `aggregate_directory`).
    pub fn write_header_precanon<W: Write>(
        &self,
        path: &Path,
        writer: &mut W,
    ) -> std::io::Result<()> {
        if self.no_path {
            return Ok(());
        }
        write!(writer, "--- File: ")?;
        if self.relative {
            self.write_relative_path(path, writer)?;
        } else {
            write!(writer, "{}", path.display())?;
        }
        writeln!(writer, " ---")
    }

    fn write_relative_path<W: Write>(&self, path: &Path, writer: &mut W) -> std::io::Result<()> {
        match self.cwd.as_deref() {
            Some(cwd) => match pathdiff::diff_paths(path, cwd) {
                Some(rel) => write!(writer, "{}", rel.display()),
                None => write!(writer, "{}", path.display()),
            },
            None => write!(writer, "{}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn header(formatter: &PathFormatter, path: &Path) -> String {
        let mut buf = Vec::new();
        formatter.write_header(path, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn test_format_path_no_path() {
        let formatter = PathFormatter::new(false, true);
        let mut buf = Vec::new();
        formatter
            .write_header(Path::new("/some/path/file.txt"), &mut buf)
            .unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_format_path_absolute() {
        let formatter = PathFormatter::new(false, false);
        let result = header(&formatter, Path::new("/some/path/file.txt"));
        assert!(result.contains("--- File:"));
        assert!(result.contains("file.txt"));
    }

    #[test]
    fn test_format_path_relative() {
        let formatter = PathFormatter::new(true, false);
        let result = header(&formatter, Path::new("file.txt"));
        assert!(result.contains("--- File:"));
        assert!(result.contains("file.txt"));
    }

    #[test]
    fn test_format_path_with_temp_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();

        let formatter = PathFormatter::new(false, false);
        let result = header(&formatter, &file_path);
        assert!(result.contains("--- File:"));
        assert!(result.contains("test.txt"));
    }
}
