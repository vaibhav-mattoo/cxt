use anyhow::Result;
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
    let args = Args::parse();
    
    // Validate arguments
    if let Err(e) = args.validate() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    // Determine paths: from TUI or CLI
    let paths: Vec<String> = if args.tui {
        let selected = tui::run_tui()?;
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

    // Initialize content aggregator
    let mut aggregator = ContentAggregator::new(
        args.relative,
        args.no_path,
        args.hidden,
        args.ignore.clone().into_iter().collect::<Vec<_>>(),
    );

    // Aggregate content from all specified paths
    let content = aggregator.aggregate_paths(&paths)?;

    // Handle output based on flags
    let mut output_handler = OutputHandler::new();
    
    // Print to stdout if requested
    if args.print {
        output_handler.print_to_stdout(&content)?;
    }
    
    // Write to file if requested
    if let Some(file_path) = &args.write {
        output_handler.write_to_file(file_path, &content)?;
        println!("Wrote content from {} files to {}", aggregator.file_count(), file_path);
    }
    
    // Copy to clipboard if no specific output was requested, or if print was requested
    if !args.print && args.write.is_none() {
        // Default: copy to clipboard only
        if !args.ci {
            output_handler.copy_to_clipboard(&content)?;
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

    Ok(())
} 