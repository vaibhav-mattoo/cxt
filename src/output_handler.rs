use anyhow::{Context, Result};
use arboard::Clipboard;
use dialoguer::Select;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

pub struct OutputHandler {
    clipboard: Option<Clipboard>,
}

impl OutputHandler {
    pub fn new() -> Self {
        let clipboard = Clipboard::new().ok();
        Self { clipboard }
    }

    /// Copy content to system clipboard, using the best available method for the platform
    pub fn copy_to_clipboard(&mut self, content: &str) -> Result<()> {
        // macOS: use pbcopy
        #[cfg(target_os = "macos")]
        {
            // println!("DEBUG: Using pbcopy for macOS");
            let mut child = Command::new("pbcopy")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .with_context(|| "Failed to spawn pbcopy")?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(content.as_bytes())
                    .with_context(|| "Failed to write to pbcopy stdin")?;
            }
            child.wait().with_context(|| "Failed to wait for pbcopy")?;
            return Ok(());
        }

        // Windows: use arboard
        #[cfg(target_os = "windows")]
        {
            // println!("DEBUG: Using arboard for Windows");
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(content)
                    .with_context(|| "Failed to copy content to clipboard")?;
                // Keep clipboard alive for a short time
                thread::sleep(Duration::from_millis(500));
                return Ok(());
            } else {
                return Err(anyhow::anyhow!("Clipboard not available on this system"));
            }
        }

        // Linux/Unix: detect Wayland/X11 and use the best tool
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
        {
            let session_type = env::var("XDG_SESSION_TYPE").unwrap_or_default();
            let wayland_display = env::var("WAYLAND_DISPLAY").unwrap_or_default();
            let x11_display = env::var("DISPLAY").unwrap_or_default();

            // println!("DEBUG: Session type: '{session_type}', Wayland display: '{wayland_display}', X11 display: '{x11_display}'");

            // Try popular clipboard managers first
            let clipboard_managers = [
                ("copyq", vec!["add", "-"]),
                ("clipman", vec!["add", "-"]),
                ("cliphist", vec!["store"]),
                ("gpaste-client", vec!["add"]),
                ("clipse", vec!["add"]),
            ];

            for (manager, args) in clipboard_managers.iter() {
                if Command::new("which").arg(manager).output().map(|o| o.status.success()).unwrap_or(false) {
                    // println!("DEBUG: Using {} for clipboard", manager);
                    let mut child = Command::new(manager)
                        .args(args)
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                        .with_context(|| format!("Failed to spawn {manager}. Is {manager} installed?"))?;
                    if let Some(mut stdin) = child.stdin.take() {
                        stdin.write_all(content.as_bytes())
                            .with_context(|| format!("Failed to write to {manager} stdin"))?;
                    }
                    let status = child.wait().with_context(|| format!("Failed to wait for {manager}"))?;
                    if status.success() {
                        // println!("DEBUG: {} completed successfully", manager);
                        return Ok(());
                    }
                }
            }

            // Wayland: use wl-copy
            if session_type == "wayland" || !wayland_display.is_empty() {
                // println!("DEBUG: Using wl-copy for Wayland");
                let mut child = Command::new("wl-copy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .with_context(|| "Failed to spawn wl-copy. Is wl-clipboard installed?")?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(content.as_bytes())
                        .with_context(|| "Failed to write to wl-copy stdin")?;
                }
                let status = child.wait().with_context(|| "Failed to wait for wl-copy")?;
                if !status.success() {
                    return Err(anyhow::anyhow!("wl-copy failed with status: {}", status));
                }
                // println!("DEBUG: wl-copy completed successfully");
                return Ok(());
            }

            // X11: use xclip if available
            if !x11_display.is_empty() && Command::new("which").arg("xclip").output().map(|o| o.status.success()).unwrap_or(false) {
                // println!("DEBUG: Using xclip for X11");
                let mut child = Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .with_context(|| "Failed to spawn xclip. Is xclip installed?")?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(content.as_bytes())
                        .with_context(|| "Failed to write to xclip stdin")?;
                }
                child.wait().with_context(|| "Failed to wait for xclip")?;
                return Ok(());
            }

            // Fallback: try arboard
            if let Some(ref mut clipboard) = self.clipboard {
                // println!("DEBUG: Using arboard fallback");
                clipboard.set_text(content)
                    .with_context(|| "Failed to copy content to clipboard")?;
                thread::sleep(Duration::from_millis(500));
                return Ok(());
            }

            // If all else fails
            Err(anyhow::anyhow!("No supported clipboard system detected. Try installing wl-clipboard, xclip, or a clipboard manager like copyq/clipman/cliphist."))
        }

        // Other OS: fallback to arboard
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "macos", target_os = "windows")))]
        {
            // println!("DEBUG: Using arboard for other OS");
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(content)
                    .with_context(|| "Failed to copy content to clipboard")?;
                thread::sleep(Duration::from_millis(500));
                return Ok(());
            }
            Err(anyhow::anyhow!("Clipboard not available on this system"))
        }
    }

    /// Print content to stdout
    pub fn print_to_stdout(&self, content: &str) -> Result<()> {
        print!("{content}");
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
                        .with_context(|| format!("Failed to write to file: {file_path}"))?;
                }
                FileWriteChoice::Append => {
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .with_context(|| format!("Failed to open file for appending: {file_path}"))?;
                    
                    writeln!(file, "\n{content}")
                        .with_context(|| format!("Failed to append to file: {file_path}"))?;
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
                .with_context(|| format!("Failed to write to file: {file_path}"))?;
        }
        
        Ok(())
    }

    /// Handle file conflict with interactive prompt
    fn handle_file_conflict(&self, file_path: &str) -> Result<FileWriteChoice> {
        println!("File '{file_path}' already exists. What would you like to do?");
        
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
