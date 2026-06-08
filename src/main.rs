use anyhow::Result;
use clap::Parser;
use std::io::{self, Write};

mod cli;
mod clipboard;
mod content_aggregator;
mod output_handler;
mod path_formatter;
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

    let mut aggregator = ContentAggregator::new(
        args.relative,
        args.no_path,
        args.hidden,
        args.ignore.clone().into_iter().collect::<Vec<_>>(),
        !args.no_sort,
    );

    let mut output_handler = OutputHandler::new();

    // Scenario 1: stream directly to a file — O(1) memory
    if let Some(file_path) = &args.write {
        let mut file = std::fs::File::create(file_path)?;
        aggregator.aggregate_paths(&paths, &mut file)?;
        println!(
            "Wrote content from {} files to {}",
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
            "Copied content from {} files to clipboard.",
            aggregator.file_count()
        );
    }
    // Scenario 4: --ci with no print and no write — validate paths, discard output
    else {
        aggregator.aggregate_paths(&paths, &mut io::sink())?;
    }
    Ok(())
}
