use anyhow::Result;
use clap::Parser;
use clap::CommandFactory;

mod cli;
mod content_aggregator;
mod output_handler;
mod path_formatter;

use cli::Args;
use content_aggregator::ContentAggregator;
use output_handler::OutputHandler;

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Validate arguments
    if let Err(e) = args.validate() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    
    // Validate that paths are provided
    if args.paths.is_empty() {
        Args::command().print_help()?;
        std::process::exit(1);
    }

    // Initialize content aggregator
    let mut aggregator = ContentAggregator::new(
        args.relative,
        args.no_path,
    );

    // Aggregate content from all specified paths
    let content = aggregator.aggregate_paths(&args.paths)?;

    // Handle output based on flags
    let mut output_handler = OutputHandler::new();
    
    match (args.print, args.write.as_deref()) {
        (true, _) => {
            // Print to stdout
            output_handler.print_to_stdout(&content)?;
            // Also copy to clipboard
            if let Err(e) = output_handler.copy_to_clipboard(&content) {
                eprintln!("Warning: Failed to copy to clipboard: {}", e);
            } else {
                println!("Copied content from {} files to clipboard.", aggregator.file_count());
            }
        }
        (_, Some(file_path)) => {
            // Write to file
            output_handler.write_to_file(file_path, &content)?;
            println!("Wrote content from {} files to {}", aggregator.file_count(), file_path);
        }
        (_, None) => {
            // Default: copy to clipboard
            output_handler.copy_to_clipboard(&content)?;
            println!("Copied content from {} files to clipboard.", aggregator.file_count());
        }
    }

    Ok(())
} 