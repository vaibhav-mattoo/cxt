use anyhow::{Context, Result};
use arboard::Clipboard;
use dialoguer::Select;
use std::fs;
use std::io::Write;
use std::path::Path;

pub struct OutputHandler {
    clipboard: Option<Clipboard>,
}

impl OutputHandler {
    pub fn new() -> Self {
        let clipboard = Clipboard::new().ok();
        Self { clipboard }
    }

    /// Copy content to system clipboard
    pub fn copy_to_clipboard(&mut self, content: &str) -> Result<()> {
        if let Some(ref mut clipboard) = self.clipboard {
            clipboard
                .set_text(content)
                .with_context(|| "Failed to copy content to clipboard")?;
        } else {
            return Err(anyhow::anyhow!("Clipboard not available on this system"));
        }
        Ok(())
    }

    /// Print content to stdout
    pub fn print_to_stdout(&self, content: &str) -> Result<()> {
        print!("{}", content);
        std::io::stdout().flush()?;
        Ok(())
    }

    /// Write content to a file with interactive conflict resolution
    pub fn write_to_file(&self, file_path: &str, content: &str) -> Result<()> {
        let path = Path::new(file_path);
        
        if path.exists() {
            let choice = self.handle_file_conflict(file_path)?;
            
            match choice {
                FileWriteChoice::Replace => {
                    fs::write(path, content)
                        .with_context(|| format!("Failed to write to file: {}", file_path))?;
                }
                FileWriteChoice::Append => {
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .with_context(|| format!("Failed to open file for appending: {}", file_path))?;
                    
                    writeln!(file, "\n{}", content)
                        .with_context(|| format!("Failed to append to file: {}", file_path))?;
                }
                FileWriteChoice::Cancel => {
                    println!("Operation cancelled.");
                    return Ok(());
                }
            }
        } else {
            // Create parent directories if they don't exist
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }
            
            fs::write(path, content)
                .with_context(|| format!("Failed to write to file: {}", file_path))?;
        }
        
        Ok(())
    }

    /// Handle file conflict with interactive prompt
    fn handle_file_conflict(&self, file_path: &str) -> Result<FileWriteChoice> {
        println!("File '{}' already exists. What would you like to do?", file_path);
        
        let options = vec!["Replace", "Append", "Cancel"];
        let selection = Select::new()
            .with_prompt("Choose an option")
            .items(&options)
            .default(0)
            .interact()
            .with_context(|| "Failed to get user input")?;

        match selection {
            0 => Ok(FileWriteChoice::Replace),
            1 => Ok(FileWriteChoice::Append),
            2 => Ok(FileWriteChoice::Cancel),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
enum FileWriteChoice {
    Replace,
    Append,
    Cancel,
} 