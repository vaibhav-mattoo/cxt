//! Centralised platform / environment detection.
//!
//! Previously duplicated across `clipboard.rs`, `image_handler.rs`,
//! and `output_handler.rs`.

use std::process::{Command, Stdio};

/// Check if a command is available on PATH.
/// Uses `which` on Unix and `where` on Windows.
pub fn command_available(program: &str) -> bool {
    let checker = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    Command::new(checker)
        .arg(program)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Whether the current session is Wayland.
pub fn is_wayland_session() -> bool {
    let session = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    session == "wayland" || !display.is_empty()
}

/// Whether we are running inside WSL.
pub fn is_wsl() -> bool {
    std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSL_ENV").is_ok()
}

/// Whether an X11 DISPLAY variable is set.
pub fn has_x11_display() -> bool {
    !std::env::var("DISPLAY").unwrap_or_default().is_empty()
}
