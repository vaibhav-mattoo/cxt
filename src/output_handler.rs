use anyhow::Result;
use std::env;

use crate::clipboard::{
    self, ArboardBackend, ClipboardBackend, ClipboardWriter, NamedProcessBackend, WlCopyBackend,
    X11Backend,
};

pub struct OutputHandler {
    backends: Vec<Box<dyn ClipboardBackend>>,
}

impl OutputHandler {
    pub fn new() -> Self {
        Self {
            backends: Self::build_backend_chain(),
        }
    }

    /// Ordered list of backends to try, most preferred first.
    fn build_backend_chain() -> Vec<Box<dyn ClipboardBackend>> {
        let mut chain: Vec<Box<dyn ClipboardBackend>> = Vec::new();

        #[cfg(target_os = "macos")]
        chain.push(Box::new(clipboard::PbcopyBackend));

        #[cfg(target_os = "windows")]
        chain.push(Box::new(ArboardBackend::new()));

        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd"
        ))]
        {
            if env::var("WSL_DISTRO_NAME").is_ok() || env::var("WSL_ENV").is_ok() {
                chain.push(Box::new(clipboard::WslBackend));
                return chain;
            }

            let session_type = env::var("XDG_SESSION_TYPE")
                .unwrap_or_default()
                .to_lowercase();
            let wayland_display = env::var("WAYLAND_DISPLAY").unwrap_or_default();

            if session_type == "wayland" || !wayland_display.is_empty() {
                // Wayland: wl-copy → clipboard managers → arboard
                chain.push(Box::new(WlCopyBackend));
                push_clipboard_managers(&mut chain);
                chain.push(Box::new(ArboardBackend::new()));
            } else {
                // X11 / other: arboard → clipboard managers → wl-copy → xclip
                chain.push(Box::new(ArboardBackend::new()));
                push_clipboard_managers(&mut chain);
                chain.push(Box::new(WlCopyBackend));
                chain.push(Box::new(X11Backend));
            }
        }

        // Fallback for unrecognised platforms
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "macos",
            target_os = "windows"
        )))]
        chain.push(Box::new(ArboardBackend::new()));

        chain
    }

    /// Walk the backend chain and return the first writer that opens successfully.
    pub fn get_clipboard_writer(&mut self) -> Result<ClipboardWriter> {
        for mut backend in self.backends.drain(..) {
            if !backend.is_available() {
                continue;
            }
            match backend.get_writer() {
                Ok(writer) => return Ok(ClipboardWriter::new(writer, backend)),
                Err(_) => continue,
            }
        }
        Err(anyhow::anyhow!(
            "No supported clipboard tool found. \
             Install one of: wl-clipboard, xclip, copyq, clipman, cliphist, \
             gpaste-client, or ensure arboard can connect to a display."
        ))
    }
}

fn push_clipboard_managers(chain: &mut Vec<Box<dyn ClipboardBackend>>) {
    const MANAGERS: &[(&str, &[&str])] = &[
        ("copyq", &["add", "-"]),
        ("clipman", &["add", "-"]),
        ("cliphist", &["store"]),
        ("gpaste-client", &["add"]),
        ("clipse", &["add"]),
    ];
    for &(prog, args) in MANAGERS {
        chain.push(Box::new(NamedProcessBackend::new(prog, args)));
    }
}
