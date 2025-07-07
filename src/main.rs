use anyhow::Result;
use std::time::Instant;
use clap::Parser;
use clap::CommandFactory;

mod cli;
mod content_aggregator;
mod output_handler;
mod path_formatter;
mod tui;

use cli::Args;
use content_aggregator::ContentAggregator;
use output_handler::OutputHandler;

fn main() -> Result<()> {
    let start_main = Instant::now();
    println!("DEBUG: main started");
    let args = Args::parse();
    println!("DEBUG: Parsed args");
    
    // Validate arguments
    if let Err(e) = args.validate() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    // Determine paths: from TUI or CLI
    let paths: Vec<String> = if args.tui {
        let tui_start = Instant::now();
        let selected = tui::run_tui()?;
        println!("DEBUG: TUI finished in {:?}", tui_start.elapsed());
        if selected.is_empty() {
            println!("No files or directories selected. Exiting.");
            return Ok(());
        }
        selected
    } else {
        if args.paths.is_empty() {
            Args::command().print_help()?;
            std::process::exit(1);
        }
        args.paths.clone()
    };
    println!("DEBUG: Paths determined: {:?}", paths);

    // Initialize content aggregator
    let mut aggregator = ContentAggregator::new(
        args.relative,
        args.no_path,
        args.hidden,
        args.ignore.clone().into_iter().collect::<Vec<_>>(),
    );
    println!("DEBUG: ContentAggregator initialized");
    let agg_start = Instant::now();

    // Aggregate content from all specified paths
    let content = aggregator.aggregate_paths(&paths)?;
    println!(
        "DEBUG: Content aggregated from {} files in {:?}",
        aggregator.file_count(),
        agg_start.elapsed()
    );

    // Handle output based on flags
    let mut output_handler = OutputHandler::new();
    println!("DEBUG: OutputHandler initialized");
    
    // Print to stdout if requested
    if args.print {
        let print_start = Instant::now();
        output_handler.print_to_stdout(&content)?;
        println!("DEBUG: Printed to stdout in {:?}", print_start.elapsed());
    }
    
    // Write to file if requested
    if let Some(file_path) = &args.write {
                let write_start = Instant::now();
        output_handler.write_to_file(file_path, &content)?;
        println!("DEBUG: Wrote to file '{}' in {:?}", file_path, write_start.elapsed());
        println!("Wrote content from {} files to {}", aggregator.file_count(), file_path);
    }
    
    // Copy to clipboard if no specific output was requested, or if print was requested
    if !args.print && args.write.is_none() {
        // Default: copy to clipboard only
        if !args.ci {
                        let clip_start = Instant::now();
            output_handler.copy_to_clipboard(&content)?;
            println!("DEBUG: Copied to clipboard in {:?}", clip_start.elapsed());
            println!("Copied content from {} files to clipboard.", aggregator.file_count());
        }
    } else if args.print {
        // Print was requested, also copy to clipboard
        if !args.ci {
            if let Err(e) = output_handler.copy_to_clipboard(&content) {
                eprintln!("Warning: Failed to copy to clipboard: {e}");
            } else {
                println!("Copied content from {} files to clipboard.", aggregator.file_count());
            }
        }
    }
    println!("DEBUG: main finished in {:?}", start_main.elapsed());
    Ok(())
} 
