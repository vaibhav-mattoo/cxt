use anyhow::Result;
use clap::Parser;

mod cli;
mod clipboard;
mod content_aggregator;
mod formatter;
mod image_handler;
mod lang;
mod notebook;
mod output_handler;
mod token_counter;
mod tui;

use cli::{Args, Destination, Mode};
use content_aggregator::ContentAggregator;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Read newline-delimited paths from stdin, stripping CR and skipping blank lines.
fn read_stdin_paths() -> anyhow::Result<Vec<String>> {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let paths = stdin
        .lock()
        .lines()
        .map_while(Result::ok)
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(paths)
}

fn dedup_paths(paths: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

fn expand_braces(inputs: Vec<String>) -> Vec<String> {
    inputs
        .into_iter()
        .flat_map(|s| {
            if s.contains('{') {
                match bracoxide::explode(&s) {
                    Ok(expanded) => expanded,
                    Err(_) => vec![s],
                }
            } else {
                vec![s]
            }
        })
        .collect()
}

fn destination_from_args(args: &Args) -> Destination {
    args.output.destination()
}

fn print_binary_skip_warning(aggregator: &ContentAggregator) {
    let n = aggregator.skipped_binary_count();
    if n > 0 {
        eprintln!(
            "({n} binary file{} skipped — add -i patterns to suppress this warning)",
            if n == 1 { "" } else { "s" }
        );
    }
}

fn print_aggregate_summary(aggregator: &ContentAggregator, dest: &Destination) {
    let files = aggregator.file_count();
    let tokens = token_counter::format_count(aggregator.token_count());
    let plural = if files == 1 { "" } else { "s" };
    match dest {
        Destination::File { path, .. } => {
            println!(
                "Wrote {tokens} tokens from {files} file{plural} to {}.",
                path.display()
            );
        }
        Destination::Clipboard { .. } => {
            println!("Copied {tokens} tokens from {files} file{plural} to clipboard.");
        }
        Destination::Stdout | Destination::Discard => {}
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

    match args.mode() {
        Mode::ListLanguages => {
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

        Mode::GitDiff(n) => {
            let diff_output = if n == 0 {
                std::process::Command::new("git").args(["diff"]).output()?
            } else {
                let range = format!("HEAD~{n}..HEAD");
                std::process::Command::new("git")
                    .args(["diff", &range])
                    .output()?
            };
            if !diff_output.status.success() {
                let stderr = String::from_utf8_lossy(&diff_output.stderr);
                anyhow::bail!("git diff failed: {stderr}");
            }
            let diff_text = String::from_utf8_lossy(&diff_output.stdout);
            if diff_text.is_empty() {
                println!("No diff output.");
                return Ok(());
            }
            let tokens = token_counter::TokenCounter::new().count(&diff_text);
            let dest = Destination::Clipboard {
                echo: args.output.print,
            };
            dest.write_with(|w| {
                w.write_all(diff_text.as_bytes())
                    .map_err(anyhow::Error::from)
            })?;
            let label = if n == 0 {
                "git diff".to_string()
            } else {
                format!("git diff HEAD~{n}..HEAD")
            };
            println!(
                "Copied {} tokens ({label}) to clipboard.",
                token_counter::format_count(tokens)
            );
            return Ok(());
        }

        Mode::Aggregate => {}
    }

    // --st: resolve git-changed files, then fall through to aggregate.
    let st_paths: Option<Vec<String>> = if let Some(n) = args.source.st {
        let output = if n == 0 {
            std::process::Command::new("git")
                .args(["diff", "--name-only", "HEAD"])
                .output()?
        } else {
            let range = format!("HEAD~{n}..HEAD");
            std::process::Command::new("git")
                .args(["diff", "--name-only", &range])
                .output()?
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git diff --name-only failed: {stderr}");
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if text.trim().is_empty() {
            println!("No changed files.");
            return Ok(());
        }
        let paths: Vec<String> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        for p in &paths {
            println!("  {p}");
        }
        Some(paths)
    } else {
        None
    };

    let stdin_is_piped = !atty::is(atty::Stream::Stdin);
    let render = args.render;

    let mut tui_header: Option<cli::PathHeader> = None;

    let paths: Vec<String> = if let Some(p) = st_paths {
        p
    } else if args.source.tui {
        let outcome = tui::run_tui(render.relative, render.no_path)?;
        tui_header = Some(outcome.path_header);
        if outcome.paths.is_empty() {
            println!("No files or directories selected. Exiting.");
            return Ok(());
        }
        outcome.paths
    } else if stdin_is_piped {
        let stdin_paths = read_stdin_paths()?;
        let combined = dedup_paths(args.paths.iter().cloned().chain(stdin_paths).collect());
        if combined.is_empty() {
            anyhow::bail!(
                "No paths provided. Pipe a newline-delimited list of paths or pass them as arguments.\n\
                 Examples:\n  fd -e rs | cxt\n  cat file_list.txt | cxt\n  cxt src/ Cargo.toml"
            );
        }
        combined
    } else if args.paths.is_empty() {
        let outcome = tui::run_tui(render.relative, render.no_path)?;
        tui_header = Some(outcome.path_header);
        if outcome.paths.is_empty() {
            println!("No files or directories selected. Exiting.");
            return Ok(());
        }
        outcome.paths
    } else {
        args.paths.clone()
    };

    let paths = expand_braces(paths);

    if image_handler::check_image_mode(&paths)? {
        let dest = destination_from_args(&args);
        if !dest.requires_clipboard() {
            anyhow::bail!("Image mode requires clipboard access and is incompatible with --ci/--write/--print.");
        }
        let path = std::path::Path::new(&paths[0]);
        image_handler::copy_image_to_clipboard(path)?;
        return Ok(());
    }

    let allowed_extensions = args.select.extensions().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let header = tui_header.unwrap_or_else(|| render.header());
    let fmt = formatter::build_formatter(render.format, header);
    let mut aggregator = ContentAggregator::new(
        fmt,
        args.select.hidden,
        expand_braces(args.select.ignore.clone()),
        !args.select.no_sort,
        allowed_extensions,
    );

    let dest = destination_from_args(&args);

    if dest.requires_clipboard() {
        let cwd = std::env::current_dir().ok();
        for p in &paths {
            let display = cwd
                .as_ref()
                .and_then(|c| std::path::Path::new(p).strip_prefix(c).ok())
                .map(|rel| rel.display().to_string())
                .unwrap_or_else(|| p.clone());
            println!("  {display}");
        }
    }

    dest.write_with(|w| aggregator.aggregate_paths(&paths, w))?;
    print_binary_skip_warning(&aggregator);
    print_aggregate_summary(&aggregator, &dest);

    Ok(())
}
