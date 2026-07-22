use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathHeader {
    Absolute,
    Relative,
    None,
}

// ── Destination ──────────────────────────────────────────────────────────────

pub enum Destination {
    /// Write to clipboard; echo=true also tees to stdout.
    Clipboard { echo: bool },
    /// Write to a file; path already has .gz suffix when gzip=true.
    File { path: PathBuf, gzip: bool },
    /// Write to stdout (--ci --print).
    Stdout,
    /// Discard output (--ci alone — still validates paths).
    Discard,
}

// ── Mode ─────────────────────────────────────────────────────────────────────

pub enum Mode {
    ListLanguages,
    GitDiff(u8),
    Aggregate,
}

// ── Flat Args (clap entry point) ──────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "cxt",
    about = "Aggregates file/directory contents and sends them to the clipboard (default), a file, or stdout",
    version,
    long_about = "cxt is a command-line tool that aggregates the contents of specified files and directories into a single string, then directs it to the clipboard, a file, or standard output."
)]
pub struct Args {
    #[arg(help = "File and/or directory paths to aggregate, or a single image file to copy")]
    pub paths: Vec<String>,

    #[command(flatten)]
    pub source: SourceArgs,

    #[command(flatten)]
    pub select: SelectArgs,

    #[command(flatten)]
    pub render: RenderArgs,

    #[command(flatten)]
    pub output: OutputArgs,
}

impl Args {
    pub fn mode(&self) -> Mode {
        if self
            .select
            .lang
            .iter()
            .any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("help")))
        {
            return Mode::ListLanguages;
        }
        if let Some(n) = self.source.df {
            return Mode::GitDiff(n);
        }
        Mode::Aggregate
    }

    pub fn validate(&self) -> Result<(), String> {
        for ignore_path in &self.select.ignore {
            if crate::content_aggregator::is_glob_pattern(ignore_path) {
                if let Err(e) = globset::Glob::new(ignore_path) {
                    return Err(format!("Invalid ignore pattern '{ignore_path}': {e}"));
                }
            }
        }
        for raw in &self.select.lang {
            for token in raw.split(',') {
                let token = token.trim();
                if token.is_empty() || token.eq_ignore_ascii_case("help") {
                    continue;
                }
                if crate::lang::find(token).is_none() {
                    return Err(format!(
                        "Unknown language '{}'. Supported languages:\n  {}\n\
                         Use --lang help to list all supported languages.",
                        token,
                        crate::lang::all_names().join(", ")
                    ));
                }
            }
        }
        Ok(())
    }
}

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(ClapArgs)]
pub struct SourceArgs {
    #[arg(
        short,
        long,
        help = "Launch interactive TUI file selector",
        conflicts_with = "df"
    )]
    pub tui: bool,

    #[arg(
        long = "df",
        num_args = 0..=1,
        default_missing_value = "0",
        require_equals = false,
        help = "Copy git diff output to clipboard. Use --df=1 for HEAD~1..HEAD, --df=2 for HEAD~2..HEAD, etc.",
        conflicts_with_all = ["st", "tui", "write"],
    )]
    pub df: Option<u8>,

    #[arg(
        long = "st",
        num_args = 0..=1,
        default_missing_value = "0",
        require_equals = false,
        help = "Aggregate git status modified files. Use --st=N for files changed in last N commits.",
        conflicts_with = "df",
    )]
    pub st: Option<u8>,
}

#[derive(ClapArgs)]
pub struct SelectArgs {
    #[arg(short, long, help = "Ignore a file or directory", value_name = "PATH",
          action = clap::ArgAction::Append)]
    pub ignore: Vec<String>,

    #[arg(
        long,
        value_name = "EXT[,EXT...]",
        help = "Include only files with these extensions (e.g. --ext rs,toml). \
                May be specified multiple times or comma-separated.",
        action = clap::ArgAction::Append,
    )]
    pub ext: Vec<String>,

    #[arg(
        long,
        value_name = "LANG[,LANG...]",
        help = "Include only files for the given language(s) (e.g. --lang rust). \
                Expands to that language's canonical extensions. \
                Run `cxt --lang help` to list supported languages. \
                May be specified multiple times or comma-separated.",
        action = clap::ArgAction::Append,
    )]
    pub lang: Vec<String>,

    #[arg(long, help = "Include hidden files when walking directories")]
    pub hidden: bool,

    #[arg(
        long,
        help = "Output files in arbitrary order (faster for large directories; implies non-deterministic output)"
    )]
    pub no_sort: bool,
}

impl SelectArgs {
    pub fn extensions(&self) -> Result<std::collections::HashSet<String>, String> {
        crate::lang::build_extension_filter(&self.lang, &self.ext)
    }
}

#[derive(ClapArgs, Clone, Copy)]
pub struct RenderArgs {
    #[arg(
        short,
        long,
        help = "Use relative paths in headers",
        conflicts_with = "no_path"
    )]
    pub relative: bool,

    #[arg(short, long, help = "Disable file path headers")]
    pub no_path: bool,

    #[arg(
        long,
        value_enum,
        default_value = "xml",
        help = "Output format: xml (default) wraps files in <file path=\"...\"> tags \
                inside a <context> block; markdown uses ## headings and fenced code blocks"
    )]
    pub format: crate::formatter::FormatChoice,
}

impl RenderArgs {
    pub fn header(&self) -> PathHeader {
        if self.no_path {
            PathHeader::None
        } else if self.relative {
            PathHeader::Relative
        } else {
            PathHeader::Absolute
        }
    }
}

#[derive(ClapArgs)]
pub struct OutputArgs {
    #[arg(short, long, help = "Print content to stdout")]
    pub print: bool,

    #[arg(short, long, help = "Write content to specified file")]
    pub write: Option<String>,

    #[arg(
        long,
        help = "Gzip-compress the output file (only valid with --write)",
        requires = "write"
    )]
    pub compress: bool,

    /// Run in non-interactive CI mode (disables clipboard operations)
    #[arg(long, hide = true)]
    pub ci: bool,
}

impl OutputArgs {
    pub fn destination(&self) -> Destination {
        if let Some(ref file_path) = self.write {
            let path = if self.compress && !file_path.ends_with(".gz") {
                PathBuf::from(format!("{file_path}.gz"))
            } else {
                PathBuf::from(file_path)
            };
            return Destination::File {
                path,
                gzip: self.compress,
            };
        }
        if self.print && self.ci {
            return Destination::Stdout;
        }
        if !self.ci {
            return Destination::Clipboard { echo: self.print };
        }
        Destination::Discard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Args {
        Args::parse_from(args)
    }

    // ── Destination ───────────────────────────────────────────────────────────

    #[test]
    fn dest_clipboard_plain() {
        let a = parse(&["cxt", "src/"]);
        let d = a.output.destination();
        assert!(matches!(d, Destination::Clipboard { echo: false }));
    }

    #[test]
    fn dest_clipboard_echo() {
        let a = parse(&["cxt", "src/", "--print"]);
        let d = a.output.destination();
        assert!(matches!(d, Destination::Clipboard { echo: true }));
    }

    #[test]
    fn dest_stdout() {
        let a = parse(&["cxt", "src/", "--ci", "--print"]);
        let d = a.output.destination();
        assert!(matches!(d, Destination::Stdout));
    }

    #[test]
    fn dest_discard() {
        let a = parse(&["cxt", "src/", "--ci"]);
        let d = a.output.destination();
        assert!(matches!(d, Destination::Discard));
    }

    #[test]
    fn dest_file_plain() {
        let a = parse(&["cxt", "src/", "--write", "out.txt"]);
        let d = a.output.destination();
        match d {
            Destination::File { path, gzip } => {
                assert_eq!(path, PathBuf::from("out.txt"));
                assert!(!gzip);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn dest_file_gz_normalizes_suffix() {
        let a = parse(&["cxt", "src/", "--write", "out.txt", "--compress"]);
        let d = a.output.destination();
        match d {
            Destination::File { path, gzip } => {
                assert_eq!(path, PathBuf::from("out.txt.gz"));
                assert!(gzip);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn dest_file_gz_no_double_suffix() {
        let a = parse(&["cxt", "src/", "--write", "out.gz", "--compress"]);
        let d = a.output.destination();
        match d {
            Destination::File { path, gzip } => {
                assert_eq!(path, PathBuf::from("out.gz"));
                assert!(gzip);
            }
            _ => panic!("expected File"),
        }
    }

    // ── PathHeader ────────────────────────────────────────────────────────────

    #[test]
    fn header_absolute() {
        let a = parse(&["cxt", "src/"]);
        assert_eq!(a.render.header(), PathHeader::Absolute);
    }

    #[test]
    fn header_relative() {
        let a = parse(&["cxt", "src/", "--relative"]);
        assert_eq!(a.render.header(), PathHeader::Relative);
    }

    #[test]
    fn header_none() {
        let a = parse(&["cxt", "src/", "--no-path"]);
        assert_eq!(a.render.header(), PathHeader::None);
    }

    // ── Mode ──────────────────────────────────────────────────────────────────

    #[test]
    fn mode_aggregate() {
        let a = parse(&["cxt", "src/"]);
        assert!(matches!(a.mode(), Mode::Aggregate));
    }

    #[test]
    fn mode_list_languages() {
        let a = parse(&["cxt", "--lang", "help"]);
        assert!(matches!(a.mode(), Mode::ListLanguages));
    }

    #[test]
    fn mode_git_diff_0() {
        let a = parse(&["cxt", "--df"]);
        assert!(matches!(a.mode(), Mode::GitDiff(0)));
    }

    #[test]
    fn mode_git_diff_n() {
        let a = parse(&["cxt", "--df", "2"]);
        assert!(matches!(a.mode(), Mode::GitDiff(2)));
    }

    // ── Clap conflicts ────────────────────────────────────────────────────────

    #[test]
    fn conflict_relative_and_no_path() {
        let result = Args::try_parse_from(&["cxt", "src/", "--relative", "--no-path"]);
        assert!(result.is_err());
    }

    #[test]
    fn conflict_df_and_st() {
        let result = Args::try_parse_from(&["cxt", "--df", "--st"]);
        assert!(result.is_err());
    }

    #[test]
    fn conflict_df_and_write() {
        let result = Args::try_parse_from(&["cxt", "--df", "--write", "out.txt"]);
        assert!(result.is_err());
    }

    #[test]
    fn conflict_df_and_tui() {
        let result = Args::try_parse_from(&["cxt", "--df", "--tui"]);
        assert!(result.is_err());
    }

    #[test]
    fn compress_requires_write() {
        let result = Args::try_parse_from(&["cxt", "src/", "--compress"]);
        assert!(result.is_err());
    }
}
