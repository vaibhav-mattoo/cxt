use anyhow::Result;
use std::env;
use std::io::{self, Write};

use crate::cli::Destination;
use crate::clipboard::{
    self, ArboardBackend, ClipboardBackend, ClipboardWriter, NamedProcessBackend, WlCopyBackend,
    X11Backend,
};

struct TeeWriter<'a, A: Write, B: Write> {
    a: &'a mut A,
    b: &'a mut B,
}

impl<A: Write, B: Write> Write for TeeWriter<'_, A, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.a.write_all(buf)?;
        self.b.write_all(buf)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.a.flush()?;
        self.b.flush()?;
        Ok(())
    }
}

impl Destination {
    pub fn write_with<R>(&self, f: impl FnOnce(&mut dyn Write) -> Result<R>) -> Result<R> {
        match self {
            Destination::Clipboard { echo } => {
                let mut handler = OutputHandler::new();
                let mut cw = handler.get_clipboard_writer()?;
                let result = if *echo {
                    let stdout = io::stdout();
                    let mut lock = stdout.lock();
                    {
                        let mut tee = TeeWriter {
                            a: &mut lock,
                            b: &mut cw,
                        };
                        f(&mut tee)?
                    }
                } else {
                    f(&mut cw)?
                };
                cw.finish()?;
                Ok(result)
            }
            Destination::File { path, gzip } => {
                if *gzip {
                    let file = std::fs::File::create(path)?;
                    let mut enc =
                        flate2::write::GzEncoder::new(file, flate2::Compression::default());
                    let r = f(&mut enc)?;
                    enc.finish()?;
                    Ok(r)
                } else {
                    let mut file = std::fs::File::create(path)?;
                    f(&mut file)
                }
            }
            Destination::Stdout => {
                let stdout = io::stdout();
                let lock = stdout.lock();
                let mut buf = io::BufWriter::with_capacity(256 * 1024, lock);
                let r = f(&mut buf)?;
                buf.flush()?;
                Ok(r)
            }
            Destination::Discard => f(&mut io::sink()),
        }
    }

    pub fn requires_clipboard(&self) -> bool {
        matches!(self, Destination::Clipboard { .. })
    }
}

pub struct OutputHandler {
    backends: Vec<Box<dyn ClipboardBackend>>,
}

impl OutputHandler {
    pub fn new() -> Self {
        Self {
            backends: Self::build_backend_chain(),
        }
    }

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
                chain.push(Box::new(WlCopyBackend));
                push_clipboard_managers(&mut chain);
                chain.push(Box::new(ArboardBackend::new()));
            } else {
                chain.push(Box::new(ArboardBackend::new()));
                push_clipboard_managers(&mut chain);
                chain.push(Box::new(WlCopyBackend));
                chain.push(Box::new(X11Backend));
            }
        }

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
