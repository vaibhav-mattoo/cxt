use anyhow::Result;
use clap::Parser;
use std::io::{self, Write};

mod cli;
mod clipboard;
mod content_aggregator;
mod formatter;
mod image_handler;
mod lang;
mod output_handler;
mod token_counter;
mod tui;

use cli::Args;
use content_aggregator::ContentAggregator;
use output_handler::OutputHandler;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Writes every byte to two writers simultaneously.
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

fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Args::parse_from(wild::args());

    if let Err(e) = args.validate() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    // Special case: --lang help prints supported languages and exits.
    if args.lang.iter().any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("help"))) {
        println!("Supported languages for --lang:\n");
        for name in lang::all_names() {
            let def = lang::find(name).unwrap();
            println!(
                "  {:16} extensions: {}{}",
                name,
                def.extensions.join(", "),
                if def.aliases.is_empty() {
                    String::new()
                } else {
                    format!("  (aliases: {})", def.aliases.join(", "))
                }
            );
        }
        return Ok(());
    }

    let paths: Vec<String> = if args.tui || args.paths.is_empty() {
        let selected = tui::run_tui()?;
        if selected.is_empty() {
            println!("No files or directories selected. Exiting.");
            return Ok(());
        }
        selected
    } else {
        args.paths.clone()
    };

    // Detect image mode. Errors on mixed input or multiple images.
    if image_handler::check_image_mode(&paths)? {
        // Validate flag compatibility
        if args.ci {
            anyhow::bail!("Image mode requires clipboard access and is incompatible with --ci.");
        }
        if args.print {
            anyhow::bail!("--print is incompatible with image mode.");
        }
        if args.write.is_some() {
            anyhow::bail!("--write is incompatible with image mode.");
        }
        // paths is guaranteed to have exactly one entry here by check_image_mode
        let path = std::path::Path::new(&paths[0]);
        image_handler::copy_image_to_clipboard(path)?;
        return Ok(());
    }

    let allowed_extensions = lang::build_extension_filter(&args.lang, &args.ext)
        .unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });

    let fmt = formatter::build_formatter(args.format, args.no_path, args.relative);
    let mut aggregator = ContentAggregator::new(
        fmt,
        args.hidden,
        args.ignore.clone().into_iter().collect::<Vec<_>>(),
        !args.no_sort,
        allowed_extensions,
    );

    let mut output_handler = OutputHandler::new();

    // Scenario 1: stream directly to a file — O(1) memory
    if let Some(file_path) = &args.write {
        let mut file = std::fs::File::create(file_path)?;
        aggregator.aggregate_paths(&paths, &mut file)?;
        println!(
            "Wrote {} tokens from {} files to {}.",
            token_counter::format_count(aggregator.token_count()),
            aggregator.file_count(),
            file_path
        );
    }
    // Scenario 2: stream directly to stdout, no clipboard — O(1) memory
    else if args.print && args.ci {
        let stdout = io::stdout();
        let handle = stdout.lock();
        let mut buf_handle = io::BufWriter::with_capacity(256 * 1024, handle);
        aggregator.aggregate_paths(&paths, &mut buf_handle)?;
        buf_handle.flush()?;
    }
    // Scenario 3: clipboard required — stream through the clipboard backend
    else if !args.ci {
        let mut cw = output_handler.get_clipboard_writer()?;

        if args.print {
            // Tee: same bytes go to stdout and the clipboard writer simultaneously
            let stdout = io::stdout();
            let mut stdout_lock = stdout.lock();
            {
                let mut tee = TeeWriter {
                    a: &mut stdout_lock,
                    b: &mut cw,
                };
                aggregator.aggregate_paths(&paths, &mut tee)?;
            }
        } else {
            aggregator.aggregate_paths(&paths, &mut cw)?;
        }

        cw.finish()?;
        println!(
            "Copied {} tokens from {} files to clipboard.",
            token_counter::format_count(aggregator.token_count()),
            aggregator.file_count()
        );
    }
    // Scenario 4: --ci with no print and no write — validate paths, discard output
    else {
        aggregator.aggregate_paths(&paths, &mut io::sink())?;
    }

    let n = aggregator.skipped_binary_count();
    if n > 0 {
        eprintln!(
            "({n} binary file{} skipped — add -i patterns to suppress this warning)",
            if n == 1 { "" } else { "s" }
        );
    }
    Ok(())
}
