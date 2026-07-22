use anyhow::Result;
use std::cell::RefCell;
use std::io::{self, BufWriter, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;

/// Backend trait: each implementation owns one clipboard mechanism.
/// Process-based backends stream directly; `flush_to_clipboard` is a no-op for them.
/// `ArboardBackend` must buffer first, so it overrides `flush_to_clipboard`.
pub trait ClipboardBackend {
    fn is_available(&self) -> bool;
    fn get_writer(&mut self) -> Result<Box<dyn Write>>;
    fn flush_to_clipboard(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Wraps a chosen backend and its writer so the caller only needs `Write + finish()`.
pub struct ClipboardWriter {
    writer: Option<Box<dyn Write>>,
    backend: Box<dyn ClipboardBackend>,
}

impl ClipboardWriter {
    pub fn new(writer: Box<dyn Write>, backend: Box<dyn ClipboardBackend>) -> Self {
        Self {
            writer: Some(writer),
            backend,
        }
    }

    /// Drop the writer (closes stdin / flushes buffer) then let the backend finalise.
    pub fn finish(mut self) -> Result<()> {
        self.writer.take();
        self.backend.flush_to_clipboard()
    }
}

impl Write for ClipboardWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.as_mut().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.as_mut().unwrap().flush()
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

struct ProcessWriter {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
}

impl Write for ProcessWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdin.as_mut().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stdin.as_mut().unwrap().flush()
    }
}

impl Drop for ProcessWriter {
    fn drop(&mut self) {
        self.stdin.take(); // close stdin so the process gets EOF
        let _ = self.child.wait();
    }
}

fn spawn_process_writer(program: &str, args: &[&str]) -> Result<Box<dyn Write>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let stdin = child.stdin.take().unwrap();
    let process_writer = ProcessWriter {
        child,
        stdin: Some(stdin),
    };
    // BufWriter flushes on drop before ProcessWriter drops, which is the correct order:
    // flush remaining bytes → close stdin (EOF) → wait for child process.
    Ok(Box::new(BufWriter::with_capacity(
        256 * 1024,
        process_writer,
    )))
}

/// Transparent writer that converts bare LF → CRLF (required by Windows clip.exe).
struct CrlfWriter<W: Write> {
    inner: W,
}

impl<W: Write> Write for CrlfWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut start = 0;
        for (i, &byte) in buf.iter().enumerate() {
            if byte == b'\n' {
                if i > start {
                    self.inner.write_all(&buf[start..i])?;
                }
                self.inner.write_all(b"\r\n")?;
                start = i + 1;
            }
        }
        if start < buf.len() {
            self.inner.write_all(&buf[start..])?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn command_available(program: &str) -> bool {
    Command::new("which")
        .arg(program)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Backends ─────────────────────────────────────────────────────────────────

pub struct WlCopyBackend;
impl ClipboardBackend for WlCopyBackend {
    fn is_available(&self) -> bool {
        command_available("wl-copy")
    }
    fn get_writer(&mut self) -> Result<Box<dyn Write>> {
        spawn_process_writer("wl-copy", &[])
    }
}

pub struct X11Backend;
impl ClipboardBackend for X11Backend {
    fn is_available(&self) -> bool {
        !std::env::var("DISPLAY").unwrap_or_default().is_empty() && command_available("xclip")
    }
    fn get_writer(&mut self) -> Result<Box<dyn Write>> {
        spawn_process_writer("xclip", &["-selection", "clipboard"])
    }
}

#[cfg(target_os = "macos")]
pub struct PbcopyBackend;

#[cfg(target_os = "macos")]
impl ClipboardBackend for PbcopyBackend {
    fn is_available(&self) -> bool {
        command_available("pbcopy")
    }
    fn get_writer(&mut self) -> Result<Box<dyn Write>> {
        spawn_process_writer("pbcopy", &[])
    }
}

pub struct WslBackend;
impl ClipboardBackend for WslBackend {
    fn is_available(&self) -> bool {
        (std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSL_ENV").is_ok())
            && std::path::Path::new("/mnt/c/Windows/System32/clip.exe").exists()
    }
    fn get_writer(&mut self) -> Result<Box<dyn Write>> {
        let inner = spawn_process_writer("/mnt/c/Windows/System32/clip.exe", &[])?;
        Ok(Box::new(CrlfWriter { inner }))
    }
}

/// Shared buffer writer for ArboardBackend (arboard cannot accept a stream).
struct SharedVecWriter(Rc<RefCell<Vec<u8>>>);
impl Write for SharedVecWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct ArboardBackend {
    buffer: Rc<RefCell<Vec<u8>>>,
    clipboard: Option<arboard::Clipboard>,
}

impl ArboardBackend {
    pub fn new() -> Self {
        Self {
            buffer: Rc::new(RefCell::new(Vec::new())),
            clipboard: None,
        }
    }
}

impl ClipboardBackend for ArboardBackend {
    fn is_available(&self) -> bool {
        arboard::Clipboard::new().is_ok()
    }

    /// Returns a writer that accumulates into an internal buffer shared with `flush_to_clipboard`.
    fn get_writer(&mut self) -> Result<Box<dyn Write>> {
        self.buffer.borrow_mut().clear();
        Ok(Box::new(SharedVecWriter(Rc::clone(&self.buffer))))
    }

    fn flush_to_clipboard(&mut self) -> Result<()> {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        let buf = self.buffer.borrow();
        let text = String::from_utf8_lossy(&buf);
        if let Some(ref mut clipboard) = self.clipboard {
            clipboard
                .set_text(text.as_ref())
                .map_err(|e| anyhow::anyhow!("arboard set_text failed: {e}"))?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            return Ok(());
        }
        Err(anyhow::anyhow!("Clipboard not available on this system"))
    }
}

/// Generic backend for clipboard managers (copyq, clipman, cliphist, etc.).
pub struct NamedProcessBackend {
    program: &'static str,
    args: &'static [&'static str],
}

impl NamedProcessBackend {
    pub fn new(program: &'static str, args: &'static [&'static str]) -> Self {
        Self { program, args }
    }
}

impl ClipboardBackend for NamedProcessBackend {
    fn is_available(&self) -> bool {
        command_available(self.program)
    }
    fn get_writer(&mut self) -> Result<Box<dyn Write>> {
        spawn_process_writer(self.program, self.args)
    }
}
