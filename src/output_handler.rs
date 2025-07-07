use anyhow::{Context, Result};
use arboard::Clipboard;
use dialoguer::Select;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub struct OutputHandler {
    clipboard: Option<Clipboard>,
}

impl OutputHandler {
    pub fn new() -> Self {
        // Do NOT initialize clipboard here to avoid hangs in WSL.
        Self { clipboard: None }
    }

    /// Helper to check if we are running inside WSL
    fn is_wsl() -> bool {
        env::var("WSL_DISTRO_NAME").is_ok() || env::var("WSL_ENV").is_ok()
            || std::fs::read_to_string("/proc/version").map(|v| v.contains("Microsoft")).unwrap_or(false)
    }

    /// Copy content to system clipboard, trying popular managers first,
    /// then wl-copy (Wayland), xclip (X11), and finally arboard as a fallback.
    pub fn copy_to_clipboard(&mut self, content: &str) -> Result<()> {

        // macOS: use pbcopy
        #[cfg(target_os = "macos")]
        {
            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .with_context(|| "Failed to spawn pbcopy")?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(content.as_bytes())
                    .with_context(|| "Failed to write to pbcopy stdin")?;
            }
            if child.wait().with_context(|| "Failed to wait for pbcopy")?.success() {
                return Ok(());
            }
            Err(anyhow::anyhow!("pbcopy exited with an error."))
        }

        // Windows: use arboard
        #[cfg(target_os = "windows")]
        {
            if self.clipboard.is_none() {
                self.clipboard = Clipboard::new().ok();
            }
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(content.to_string())
                    .with_context(|| "Failed to copy content to clipboard via arboard")?;
                thread::sleep(Duration::from_millis(500));
                return Ok(());
            } else {
                return Err(anyhow::anyhow!("Clipboard not available on this system"));
            }
        }

        // Linux/Unix: try managers → Wayland → X11 → arboard
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
        {
            if Self::is_wsl() {
                let windows_content = content.replace('\n', "\r\n");

                // Spawn clip.exe as a detached process and do NOT wait for it
                let mut child = Command::new("/mnt/c/Windows/System32/clip.exe")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| "Failed to spawn /mnt/c/Windows/System32/clip.exe. Is this a standard WSL setup?")?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(windows_content.as_bytes())
                        .with_context(|| "Failed to write to clip.exe stdin")?;
                    // Explicitly close stdin so clip.exe knows there's no more input
                    drop(stdin);
                }

                // Do NOT call child.wait() here!
                // Optionally, sleep a tiny bit to let the process start
                std::thread::sleep(std::time::Duration::from_millis(50));
                return Ok(());
            }
            let session_type  = env::var("XDG_SESSION_TYPE").unwrap_or_default().to_lowercase();
            let wayland_disp  = env::var("WAYLAND_DISPLAY").unwrap_or_default();
            let x11_disp      = env::var("DISPLAY").unwrap_or_default();

            // 1) Popular clipboard managers
            let clipboard_managers = [
                ("copyq", &["add", "-"][..]),
                ("clipman", &["add", "-"][..]),
                ("cliphist", &["store"][..]),
                ("gpaste-client", &["add"][..]),
                ("clipse", &["add"][..]),
            ];

            for (mgr, args) in &clipboard_managers {
                if Command::new("which").arg(mgr)
                        .stdout(Stdio::null()).stderr(Stdio::null())
                        .status().map(|s| s.success()).unwrap_or(false)
                {
                    let mut child = Command::new(mgr)
                        .args(*args)
                        .stdin(Stdio::piped())
                        .spawn()
                        .with_context(|| format!("Failed to spawn {mgr}. Is {mgr} installed?"))?;
                    if let Some(mut stdin) = child.stdin.take() {
                        stdin.write_all(content.as_bytes())
                            .with_context(|| format!("Failed to write to {mgr} stdin"))?;
                    }
                    if child.wait().with_context(|| format!("Failed to wait for {mgr}"))?.success() {
                        return Ok(());
                    }
                }
            }

            // 2) Wayland: wl-copy if installed
            let have_wl_copy = Command::new("which").arg("wl-copy")
                .stdout(Stdio::null()).stderr(Stdio::null())
                .status().map(|s| s.success()).unwrap_or(false);
            if (session_type == "wayland" || !wayland_disp.is_empty()) && have_wl_copy {
                let mut child = Command::new("wl-copy")
                    .stdin(Stdio::piped())
                    .spawn()
                    .with_context(|| "Failed to spawn wl-copy. Is wl-clipboard installed?")?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(content.as_bytes())
                        .with_context(|| "Failed to write to wl-copy stdin")?;
                }
                if child.wait().with_context(|| "Failed to wait for wl-copy")?.success() {
                    return Ok(());
                }
            }

            // 3) X11: xclip if DISPLAY set
            let have_xclip = Command::new("which").arg("xclip")
                .stdout(Stdio::null()).stderr(Stdio::null())
                .status().map(|s| s.success()).unwrap_or(false);
            if !x11_disp.is_empty() && have_xclip {
                let mut child = Command::new("xclip")
                    .args(&["-selection", "clipboard"])
                    .stdin(Stdio::piped())
                    .spawn()
                    .with_context(|| "Failed to spawn xclip. Is xclip installed?")?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(content.as_bytes())
                        .with_context(|| "Failed to write to xclip stdin")?;
                }
                child.wait().with_context(|| "Failed to wait for xclip")?;
                return Ok(());
            }

            // 4) Fallback: arboard crate
            if self.clipboard.is_none() {
                self.clipboard = Clipboard::new().ok();
            }
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(content.to_string())
                    .with_context(|| "Failed to copy content to clipboard via arboard")?;
                thread::sleep(Duration::from_millis(500));
                return Ok(());
            }

            // Nothing available
            Err(anyhow::anyhow!(
                "No supported clipboard tool found. \
                 Please install one of: copyq, clipman, cliphist, gpaste-client, \
                 wl-clipboard (for wl-copy), xclip, or ensure arboard works."
            ))
        }

        // Other OS: fallback to arboard
        #[cfg(not(any(
            target_os = "linux", target_os = "freebsd", target_os = "openbsd",
            target_os = "netbsd", target_os = "macos", target_os = "windows"
        )))]
        {
            if self.clipboard.is_none() {
                self.clipboard = Clipboard::new().ok();
            }
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(content.to_string())
                    .with_context(|| "Failed to copy content to clipboard via arboard")?;
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
        let options = &["Replace", "Append", "Cancel"];
        let selection = Select::new()
            .with_prompt("Choose an option")
            .items(options)
            .default(0)
            .interact()
            .with_context(|| "Failed to get user input")?;
        Ok(match selection {
            0 => FileWriteChoice::Replace,
            1 => FileWriteChoice::Append,
            _ => FileWriteChoice::Cancel,
        })
    }
}

#[derive(Debug)]
enum FileWriteChoice {
    Replace,
    Append,
    Cancel,
}

