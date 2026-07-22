use image::GenericImageView;
use std::io::Write;

pub fn is_image_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff" | "tif" | "ico")
    )
}

/// Call after path resolution, before any aggregation.
/// Returns Ok(true) if all paths are images and should use image mode.
/// Returns Ok(false) if no paths are images (normal text mode).
/// Returns Err if there is a mix, or any other invalid combination.
pub fn check_image_mode(paths: &[String]) -> anyhow::Result<bool> {
    let image_count = paths
        .iter()
        .filter(|p| is_image_path(std::path::Path::new(p)))
        .count();

    if image_count == 0 {
        return Ok(false);
    }
    if image_count < paths.len() {
        anyhow::bail!(
            "Cannot mix image and text files in a single invocation.\n\
             Provide only image files or only text/code files."
        );
    }
    if paths.len() > 1 {
        anyhow::bail!(
            "Only one image can be copied to the clipboard at a time.\n\
             Received {} image files: provide a single image file.",
            paths.len()
        );
    }
    Ok(true)
}

/// Decode `path`, re-encode as PNG for maximum paste compatibility, and write
/// it to the system clipboard. Prints a confirmation line to stdout on success.
pub fn copy_image_to_clipboard(path: &std::path::Path) -> anyhow::Result<()> {
    let img = image::open(path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open image '{}': {e}\n\
                 Supported formats: JPEG, PNG, GIF, BMP, WebP, TIFF, ICO",
            path.display()
        )
    })?;

    let (width, height) = img.dimensions();

    // Re-encode as PNG regardless of input format — browsers and most apps only
    // recognise image/png when pasting from clipboard.
    let mut png_bytes: Vec<u8> = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .map_err(|e| anyhow::anyhow!("Failed to encode image as PNG: {e}"))?;

    // On Wayland, arboard exits with the process and takes its clipboard data with it,
    // so clipboard managers see nothing. wl-copy stays alive as a persistent clipboard
    // server — the same strategy used for text in output_handler.rs.
    if is_wayland_session() && command_available("wl-copy") {
        pipe_to_clipboard_process("wl-copy", &["--type", "image/png"], &png_bytes)?;
    } else if command_available("xclip") {
        // X11: xclip also acts as a persistent clipboard server.
        pipe_to_clipboard_process(
            "xclip",
            &["-selection", "clipboard", "-t", "image/png"],
            &png_bytes,
        )?;
    } else {
        // macOS / Windows / fallback: arboard owns the clipboard for its lifetime.
        // The 500 ms sleep gives clipboard managers time to fetch before we exit.
        let rgba = img.into_rgba8();
        let bytes = rgba.into_raw();
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| anyhow::anyhow!("Failed to open clipboard: {e}"))?;
        clipboard
            .set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: bytes.into(),
            })
            .map_err(|e| anyhow::anyhow!("Failed to write image to clipboard: {e}"))?;
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    println!(
        "Copied {}×{} image '{}' to clipboard.",
        width,
        height,
        path.display()
    );
    Ok(())
}

fn is_wayland_session() -> bool {
    let session = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    session == "wayland" || !display.is_empty()
}

fn command_available(program: &str) -> bool {
    std::process::Command::new("which")
        .arg(program)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn `program`, write `data` to its stdin, close stdin, then return without
/// waiting. The child continues running as a persistent clipboard server
/// (wl-copy / xclip model — they exit only when the clipboard is replaced).
fn pipe_to_clipboard_process(program: &str, args: &[&str], data: &[u8]) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn {program}: {e}"))?;

    {
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(data)
            .map_err(|e| anyhow::anyhow!("Failed to write image data to {program}: {e}"))?;
        // stdin drops here → EOF to child, which starts serving the clipboard
    }

    // Give the child a moment to register the clipboard offer before we return.
    // We intentionally do not call child.wait() — it must outlive this process.
    std::thread::sleep(std::time::Duration::from_millis(100));
    drop(child); // drops handle only; the OS process keeps running
    Ok(())
}
