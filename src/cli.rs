use clap::Parser;

#[derive(Parser)]
#[command(
    name = "cxt",
    about = "Aggregates file/directory contents and sends them to the clipboard (default), a file, or stdout",
    version,
    long_about = "cxt is a command-line tool that aggregates the contents of specified files and directories into a single string, then directs it to the clipboard, a file, or standard output."
)]
pub struct Args {
    /// File and/or directory paths to aggregate
    #[arg(help = "File and/or directory paths to aggregate")]
    pub paths: Vec<String>,

    /// Print content to stdout
    #[arg(short, long, help = "Print content to stdout")]
    pub print: bool,

    /// Write content to specified file
    #[arg(short, long, help = "Write content to specified file")]
    pub write: Option<String>,

    /// Use relative paths in headers
    #[arg(short, long, help = "Use relative paths in headers")]
    pub relative: bool,

    /// Disable file path headers
    #[arg(short, long, help = "Disable file path headers")]
    pub no_path: bool,

    /// Include hidden files when walking directories
    #[arg(long, help = "Include hidden files when walking directories")]
    pub hidden: bool,

    /// Run in non-interactive CI mode (disables clipboard operations)
    #[arg(long, hide = true)]
    pub ci: bool,

    /// Launch Terminal User Interface (TUI) mode
    #[arg(short, long, help = "Launch interactive TUI file selector")]
    pub tui: bool,

    /// Ignore a file or directory
    #[arg(short, long, help = "Ignore a file or directory", value_name = "PATH")]
    pub ignore: Option<String>,
}

impl Args {
    /// Validate that conflicting flags are not used together
    pub fn validate(&self) -> Result<(), String> {
        if self.relative && self.no_path {
            return Err("Cannot use --relative and --no-path together".to_string());
        }
        if let Some(ignore_path) = &self.ignore {
            if !std::path::Path::new(ignore_path).exists() {
                return Err(format!("Ignore path does not exist: {ignore_path}"));
            }
        }
        Ok(())
    }
} 