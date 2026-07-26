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
    // Route through the same backend chain used for text — each backend
    // decides how to handle image data (wl-copy/xclip pipe PNG bytes;
    // arboard decodes to RGBA internally).
    let mut handler = crate::output_handler::OutputHandler::new();
    handler.copy_image_to_clipboard(&png_bytes)?;

    println!(
        "Copied {}×{} image '{}' to clipboard.",
        width,
        height,
        path.display()
    );
    Ok(())
}

// Platform detection and clipboard process spawning now live in
// `platform.rs` and `clipboard.rs` respectively — no more local copies.
