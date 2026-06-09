use clap::Parser;

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

    #[arg(short, long, help = "Print content to stdout")]
    pub print: bool,

    #[arg(short, long, help = "Write content to specified file")]
    pub write: Option<String>,

    #[arg(short, long, help = "Use relative paths in headers")]
    pub relative: bool,

    #[arg(short, long, help = "Disable file path headers")]
    pub no_path: bool,

    #[arg(long, help = "Include hidden files when walking directories")]
    pub hidden: bool,

    /// Run in non-interactive CI mode (disables clipboard operations)
    #[arg(long, hide = true)]
    pub ci: bool,

    #[arg(short, long, help = "Launch interactive TUI file selector")]
    pub tui: bool,

    #[arg(short, long, help = "Ignore a file or directory", value_name = "PATH", action = clap::ArgAction::Append)]
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

    #[arg(
        long,
        help = "Output files in arbitrary order (faster for large directories; implies non-deterministic output)"
    )]
    pub no_sort: bool,

    #[arg(
        long,
        help = "Gzip-compress the output file (only valid with --write)",
        requires = "write"
    )]
    pub compress: bool,

    #[arg(
        long,
        value_enum,
        default_value = "xml",
        help = "Output format: xml (default) wraps files in <file path=\"...\"> tags \
                inside a <context> block; markdown uses ## headings and fenced code blocks"
    )]
    pub format: crate::formatter::FormatChoice,
}

impl Args {
    /// Validate that conflicting flags are not used together
    pub fn validate(&self) -> Result<(), String> {
        if self.relative && self.no_path {
            return Err("Cannot use --relative and --no-path together".to_string());
        }
        if self.compress && self.write.is_none() {
            return Err("--compress requires --write to specify an output file".to_string());
        }
        // multiple files in ignore path provided as arguments like "cxt target_dir src/* -i dir -i file" should be ignored
        for ignore_path in &self.ignore {
            if !crate::content_aggregator::is_glob_pattern(ignore_path)
                && !std::path::Path::new(ignore_path).exists()
            {
                return Err(format!("Ignore path does not exist: {ignore_path}"));
            }
        }
        // Validate --lang values and handle the special "help" value.
        for raw in &self.lang {
            for token in raw.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                if token.eq_ignore_ascii_case("help") {
                    continue; // handled in main.rs
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
