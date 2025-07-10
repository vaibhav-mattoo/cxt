use anyhow::Result;
use clap::Parser;
use glob::glob;

mod cli;
mod content_aggregator;
mod output_handler;
mod path_formatter;
mod tui;

use cli::Args;
use content_aggregator::ContentAggregator;
use output_handler::OutputHandler;

/// Expand wildcard patterns in paths
fn expand_wildcards(paths: &[String]) -> Result<Vec<String>> {
    let mut expanded_paths = Vec::new();
    
    for path_str in paths {
        // Check if the path contains wildcards
        if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
            // Use glob to expand the pattern
            match glob(path_str) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(path) => {
                                let path_str = path.to_string_lossy().to_string();
                                expanded_paths.push(path_str);
                            }
                            Err(e) => {
                                eprintln!("Warning: Failed to expand glob pattern '{}': {}", path_str, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Invalid glob pattern '{}': {}", path_str, e));
                }
            }
        } else {
            // No wildcards, add the path as-is
            expanded_paths.push(path_str.clone());
        }
    }
    
    Ok(expanded_paths)
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Validate arguments
    if let Err(e) = args.validate() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    // Determine paths: from TUI or CLI
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

    // Expand wildcard patterns in paths
    let expanded_paths = expand_wildcards(&paths)?;
    
    if expanded_paths.is_empty() {
        println!("No files found matching the specified patterns. Exiting.");
        return Ok(());
    }

    // Initialize content aggregator
    let mut aggregator = ContentAggregator::new(
        args.relative,
        args.no_path,
        args.hidden,
        args.ignore.clone().into_iter().collect::<Vec<_>>(),
    );

    // Aggregate content from all specified paths
    let content = aggregator.aggregate_paths(&expanded_paths)?;

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
