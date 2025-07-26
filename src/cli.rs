use clap::Parser;

#[derive(Parser)]
#[command(
    name = "cxt",
    about = "Aggregates file/directory contents and sends them to the clipboard (default), a file, or stdout",
    version,
    long_about = "cxt is a command-line tool that aggregates the contents of specified files and directories into a single string, then directs it to the clipboard, a file, or standard output."
)]
pub struct Args {
    #[arg(help = "File and/or directory paths to aggregate")]
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
}

impl Args {
    /// Validate that conflicting flags are not used together
    pub fn validate(&self) -> Result<(), String> {
        if self.relative && self.no_path {
            return Err("Cannot use --relative and --no-path together".to_string());
        }
        /// multiple files in ignore path provided as arguments like "cxt target_dir src/* -i dir -i file" should be ignored
        for ignore_path in &self.ignore {
            if !std::path::Path::new(ignore_path).exists() {
                return Err(format!("Ignore path does not exist: {ignore_path}"));
            }
        }
        Ok(())
    }
} 
