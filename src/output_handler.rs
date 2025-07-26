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

    /// optional instace of clipboard since you may or may not have initialized it
    clipboard: Option<Clipboard>,
}

impl OutputHandler {
    pub fn new() -> Self {
        // Do NOT initialize clipboard here to avoid hangs in WSL.
        Self { clipboard: None }
    }

    /// Helper to check if we are running inside WSL
    fn is_wsl() -> bool {
        
        // check if in WSL by seeing if WSL_DISTRO_NAME or WSL_ENV variables are set
        // also if on reading /proc/version we map to run closure which checks if Microsoft
        // if any errors in process assume not WSL

        env::var("WSL_DISTRO_NAME").is_ok() || env::var("WSL_ENV").is_ok()
            || std::fs::read_to_string("/proc/version").map(|v| v.contains("Microsoft")).unwrap_or(false)
    }

    /// Copy content to system clipboard, trying popular managers first,
    /// then wl-copy (Wayland), xclip (X11), and finally arboard as a fallback.

    pub fn copy_to_clipboard(&mut self, content: &str) -> Result<()> {

        // macOS: use pbcopy

        // this is a Rust conditional compilation attribute
        // tells the compiler to include or exclude based on target operating system
        // this will only be compiled on macos
        #[cfg(target_os = "macos")]
        {
            // macos has native pbcopy command line tool to copy text
            // we spawn pbcopy with the child process stdin to be piped
            //     this allows us to control the stdin for it
            //     without this we would have to read input from our terminal
            // with_context uses anyhow which allows custom error message if spawn fails
            // ? operator causes a early return if spawn fails

            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .with_context(|| "Failed to spawn pbcopy")?;

            // the stdin field on child is Option and take replaces it inside child with none and
            // give it to us. If stdin was Some() then it is destructured and assigned
            //     if it is None, the block is skipped
            //
            // write_all writes all the bytes of string content into stdin.
            // the as_bytes converts the &str into byte slice &[u8] for write_all
            //
            // the wait() blocks our thread waiting for the child process to finish execution
            //     this returns a Result<ExitStatus, Error>; the ExitStatus indicates how the child process exited
            //     .success() checks if child process exited successfully

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
                
                //lazy clipboard initialization with Clipboard::new()
                self.clipboard = Clipboard::new().ok();

            }
            if let Some(ref mut clipboard) = self.clipboard {
                // the set_text on clipboard instance copies the content
                // NOTE: 500 ms needed for clipboard to not drop content immediately
                // Might not need this but keeping it for now as it works
                clipboard.set_text(content.to_string())
                    .with_context(|| "Failed to copy content to clipboard via arboard")?;
                thread::sleep(Duration::from_millis(500));
                return Ok(());
            } else {
                return Err(anyhow::anyhow!("Clipboard not available on this system"));
            }
        }

        // Linux/Unix: try arboard first, then managers → Wayland → X11
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
        {
            // Handler for WSL where need to use clip.exe instead of linux clipboards
            if Self::is_wsl() {

                // Windows programs expect \r\n as line endings
                // so this ensures Windows software receives clipboard text formatted correctly.
                let windows_content = content.replace('\n', "\r\n");

                // Spawn clip.exe as a detached process and do NOT wait for it
                // In WSL Windows file system mounted on /mnt/c
                // we need to access native Windows path from Linux
                // we configure the stdin to be piped and discard tbe std out and stderr

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
                    // for clip.exe we need to manually tell it no more input coming so it
                    // proceeds
                    drop(stdin);
                }

                // Optionally, sleep a tiny bit to let the process start
                std::thread::sleep(std::time::Duration::from_millis(50));
                return Ok(());
            }
            // tells you wayland or X11, display server type
            let session_type  = env::var("XDG_SESSION_TYPE").unwrap_or_default().to_lowercase();

            // wayland display socket name, non-empty if wayland running
            let wayland_disp  = env::var("WAYLAND_DISPLAY").unwrap_or_default();

            // X11 display string non-empty for X11 running
            let x11_disp      = env::var("DISPLAY").unwrap_or_default();

            // On Wayland, use wl-copy first, then try other managers, then arboard as last resort

            if session_type == "wayland" || !wayland_disp.is_empty() {

                // run "which wl-copy" and check if output comes successfully without printing
                let have_wl_copy = Command::new("which").arg("wl-copy")
                    .stdout(Stdio::null()).stderr(Stdio::null())
                    .status().map(|s| s.success()).unwrap_or(false);


                if have_wl_copy {
                    // spawn wlcopy with piped stdio if present
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

                // Try other managers
                // defines an array of tuples which has clipboard managers and args
                // &str representing name of manager
                // reference to a slice of string slices
                // [..] is full slice syntax which converts array literal into slice reference
                // &["add", "-"][..] creates a slice reference for the array ["add", "-"]

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
                // Last resort: arboard
                if self.clipboard.is_none() {
                    self.clipboard = Clipboard::new().ok();
                }
                if let Some(ref mut clipboard) = self.clipboard {
                    if clipboard.set_text(content.to_string()).is_ok() {
                        if clipboard.get_text().map(|s| s == content).unwrap_or(false) {
                            thread::sleep(Duration::from_millis(500));
                            return Ok(());
                        } else {
                            eprintln!("[cxt debug] arboard clipboard set failed, nothing else worked");
                        }
                    } else {
                        eprintln!("[cxt debug] arboard clipboard set_text errored, nothing else worked");
                    }
                }
            } else {
                // On X11 or other, try arboard first
                if self.clipboard.is_none() {
                    self.clipboard = Clipboard::new().ok();
                }
                if let Some(ref mut clipboard) = self.clipboard {
                    if clipboard.set_text(content.to_string()).is_ok() {
                        if clipboard.get_text().map(|s| s == content).unwrap_or(false) {
                            thread::sleep(Duration::from_millis(500));
                            return Ok(());
                        } else {
                            eprintln!("[cxt debug] arboard clipboard set failed, falling back to external clipboard tools");
                        }
                    } else {
                        eprintln!("[cxt debug] arboard clipboard set_text errored, falling back to external clipboard tools");
                    }
                }
                // Try other managers
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
                // Last resort: wl-copy (if available)
                let have_wl_copy = Command::new("which").arg("wl-copy")
                    .stdout(Stdio::null()).stderr(Stdio::null())
                    .status().map(|s| s.success()).unwrap_or(false);
                if have_wl_copy {
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

            // Nothing available
            Err(anyhow::anyhow!(
                "No supported clipboard tool found. \n                 Please install one of: copyq, clipman, cliphist, gpaste-client, \n                 wl-clipboard (for wl-copy), xclip, or ensure arboard works."
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

