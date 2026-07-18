use anyhow::Result;
use clap::Parser;
use std::io::{self, Write};

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

/// Read newline-delimited paths from stdin, stripping CR and skipping blank lines.
/// Does NOT trim spaces — spaces are valid in file names.
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

/// Deduplicate while preserving first-seen order.
fn dedup_paths(paths: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths.into_iter().filter(|p| seen.insert(p.clone())).collect()
}

/// Expand brace expressions in each string, e.g. `"src/{a,b}.rs"` → `["src/a.rs", "src/b.rs"]`.
/// Strings without `{` are returned as-is. Invalid brace syntax is passed through unchanged.
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

fn eprintln_binary_skip_summary(aggregator: &ContentAggregator) {
    let n = aggregator.skipped_binary_count();
    if n > 0 {
        eprintln!(
            "({n} binary file{} skipped — add -i patterns to suppress this warning)",
            if n == 1 { "" } else { "s" }
        );
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

    if let Some(n) = args.df {
        let diff_output = if n == 0 {
            std::process::Command::new("git")
                .args(["diff"])
                .output()?
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
        let mut output_handler = OutputHandler::new();
        let mut cw = output_handler.get_clipboard_writer()?;
        let counter = token_counter::TokenCounter::new();
        let tokens = counter.count(&diff_text);
        if args.print {
            let stdout = io::stdout();
            let mut stdout_lock = stdout.lock();
            {
                let mut tee = TeeWriter {
                    a: &mut stdout_lock,
                    b: &mut cw,
                };
                tee.write_all(diff_text.as_bytes())?;
            }
        } else {
            cw.write_all(diff_text.as_bytes())?;
        }
        cw.finish()?;
        let diff_label = if n == 0 {
            "git diff".to_string()
        } else {
            format!("git diff HEAD~{n}..HEAD")
        };
        println!(
            "Copied {} tokens ({}) to clipboard.",
            token_counter::format_count(tokens),
            diff_label
        );
        return Ok(());
    }
    if let Some(n) = args.st {
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
        let paths: Vec<String> = text.lines().filter(|l| !l.is_empty()).map(String::from).collect();
        for p in &paths {
            println!("  {p}");
        }
        let fmt = formatter::build_formatter(args.format, args.no_path, args.relative, args.aider);
        let mut aggregator = ContentAggregator::new(
            fmt,
            args.hidden,
            args.ignore.clone().into_iter().collect::<Vec<_>>(),
            !args.no_sort,
            std::collections::HashSet::new(),
        );
        let mut output_handler = OutputHandler::new();
        let mut cw = output_handler.get_clipboard_writer()?;
        if args.print {
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
        eprintln_binary_skip_summary(&aggregator);
        println!(
            "Copied {} tokens from {} file{} to clipboard.",
            token_counter::format_count(aggregator.token_count()),
            aggregator.file_count(),
            if aggregator.file_count() == 1 { "" } else { "s" }
        );
        return Ok(());
    }
    let stdin_is_piped = !atty::is(atty::Stream::Stdin);
    let mut args = args;
    let paths: Vec<String> = if args.tui {
        // --tui explicitly requested: always launch interactive picker, ignore stdin.
        let outcome = tui::run_tui(args.relative, args.no_path)?;
        args.relative = outcome.relative;
        args.no_path = outcome.no_path;
        if outcome.paths.is_empty() {
            println!("No files or directories selected. Exiting.");
            return Ok(());
        }
        outcome.paths
    } else if stdin_is_piped {
        // Stdin is a pipe: read newline-delimited paths from it.
        let stdin_paths = read_stdin_paths()?;
        // Combine CLI args (higher precedence / listed first) with stdin paths.
        let combined = dedup_paths(
            args.paths.iter().cloned().chain(stdin_paths).collect(),
        );
        if combined.is_empty() {
            anyhow::bail!(
                "No paths provided. Pipe a newline-delimited list of paths or pass them as arguments.\n\
                 Examples:\n  fd -e rs | cxt\n  cat file_list.txt | cxt\n  cxt src/ Cargo.toml"
            );
        }
        combined
    } else if args.paths.is_empty() {
        // No stdin pipe, no CLI args: fall back to interactive TUI.
        let outcome = tui::run_tui(args.relative, args.no_path)?;
        args.relative = outcome.relative;
        args.no_path = outcome.no_path;
        if outcome.paths.is_empty() {
            println!("No files or directories selected. Exiting.");
            return Ok(());
        }
        outcome.paths
    } else {
        args.paths.clone()
    };

    // Expand brace expressions in paths, e.g. "src/{main,lib}.rs" → ["src/main.rs", "src/lib.rs"].
    // TUI/stdin paths are already resolved, but brace-free strings pass through unchanged.
    let paths = expand_braces(paths);

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

    let fmt = formatter::build_formatter(args.format, args.no_path, args.relative, args.aider);
    let mut aggregator = ContentAggregator::new(
        fmt,
        args.hidden,
        expand_braces(args.ignore.clone()),
        !args.no_sort,
        allowed_extensions,
    );

    let mut output_handler = OutputHandler::new();

    // Scenario 1: stream directly to a file — O(1) memory
    if let Some(file_path) = &args.write {
        if args.compress {
            let out_path = if file_path.ends_with(".gz") {
                file_path.clone()
            } else {
                format!("{file_path}.gz")
            };
            let file = std::fs::File::create(&out_path)?;
            let mut encoder = flate2::write::GzEncoder::new(
                file,
                flate2::Compression::default(),
            );
            aggregator.aggregate_paths(&paths, &mut encoder)?;
            encoder.finish()?;
            eprintln_binary_skip_summary(&aggregator);
            println!(
                "Wrote {} tokens from {} files to {} (gzip-compressed).",
                token_counter::format_count(aggregator.token_count()),
                aggregator.file_count(),
                out_path,
            );
            return Ok(());
        } else {
            let mut file = std::fs::File::create(file_path)?;
            aggregator.aggregate_paths(&paths, &mut file)?;
            println!(
                "Wrote {} tokens from {} files to {}.",
                token_counter::format_count(aggregator.token_count()),
                aggregator.file_count(),
                file_path
            );
        }
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
        let cwd = std::env::current_dir().ok();
        for p in &paths {
            let display = cwd
                .as_ref()
                .and_then(|c| std::path::Path::new(p).strip_prefix(c).ok())
                .map(|rel| rel.display().to_string())
                .unwrap_or_else(|| p.clone());
            println!("  {display}");
        }
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

    eprintln_binary_skip_summary(&aggregator);
    Ok(())
}
