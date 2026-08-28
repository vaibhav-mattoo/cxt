use anyhow::Result;
use colored::*;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};

struct Block {
    file_path: String,
    search: String,
    replace: String,
}

/// Normalise a line for fuzzy matching by stripping comments and whitespace.
/// Returns an empty string if the line is empty or a comment.
fn normalize_for_match(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Common comment styles
    if trimmed.starts_with("//")
        || trimmed.starts_with("#")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("*")
        || trimmed.starts_with("--")
        || trimmed.starts_with("<!--")
        || trimmed.starts_with(";;;")
    {
        return String::new();
    }
    trimmed.to_string()
}

/// Read an aider-style SEARCH/REPLACE patch and interactively apply each hunk
/// with a fuzzy-match confirmation prompt.
///
/// `pb_value` resolution:
/// - empty string: read patch from the clipboard (no default file)
/// - path to an existing file: read patch from that file (no default file)
/// - any other string: read patch from the clipboard and use the string as
///   the default target file for hunks that lack an explicit path
pub fn run(pb_value: &str, debug_mode: bool) -> Result<()> {
    let (patch_content, default_file): (String, Option<&str>) = if pb_value.is_empty() {
        println!("📋 Reading and cleaning content from clipboard...");
        let clipboard =
            crate::clipboard::read_clipboard().map_err(|e| anyhow::anyhow!("{e}"))?;
        (sanitize_llm_output(&clipboard, debug_mode), None)
    } else if std::path::Path::new(pb_value).is_file() {
        println!("📋 Reading patch from file: {}", pb_value);
        let file_content = std::fs::read_to_string(pb_value).map_err(|e| {
            anyhow::anyhow!("Failed to read patch file '{}': {e}", pb_value)
        })?;
        (sanitize_llm_output(&file_content, debug_mode), None)
    } else {
        println!("📋 Reading and cleaning content from clipboard...");
        let clipboard =
            crate::clipboard::read_clipboard().map_err(|e| anyhow::anyhow!("{e}"))?;
        (sanitize_llm_output(&clipboard, debug_mode), Some(pb_value))
    };

    if !patch_content
        .lines()
        .any(|l| l.trim().starts_with("<<<<<<< SEARCH"))
    {
        anyhow::bail!("No valid SEARCH/REPLACE blocks found in clipboard");
    }

    let blocks = parse_search_replace(&patch_content);
    if blocks.is_empty() {
        anyhow::bail!("No valid SEARCH/REPLACE blocks found in patch");
    }
    println!("✅ Extracted {} Hunk(s)", blocks.len());

    if debug_mode {
        let mut debug_str = String::from("\n🔍 [Debug] Extracted Hunk info:\n");
        for (i, b) in blocks.iter().enumerate() {
            debug_str.push_str(&format!("  [{}] File: {}\n", i + 1, b.file_path));
            let search_lines: Vec<&str> = b.search.lines().collect();
            let preview = search_lines
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n        ");
            let ellipsis = if search_lines.len() > 3 {
                "\n        ..."
            } else {
                ""
            };
            debug_str.push_str(&format!(
                "        Search:\n        {}{}\n",
                preview, ellipsis
            ));
        }
        println!("{}", debug_str);
        fs::write("debug.log", debug_str).ok();
    }

    let total_hunks = blocks.len();
    let mut grouped: BTreeMap<String, Vec<Block>> = BTreeMap::new();
    for block in blocks {
        grouped
            .entry(block.file_path.clone())
            .or_default()
            .push(block);
    }

    let mut global_hunk_idx = 0;

    for (file_path, file_blocks) in grouped {
        let mut target_file = file_path.clone();

        if target_file == "unknown_file" {
            if let Some(def) = default_file {
                target_file = def.to_string();
            } else {
                let start_hunk = global_hunk_idx + 1;
                let end_hunk = global_hunk_idx + file_blocks.len();
                print!(
                    "\n🔍 No file path detected (contains Hunk {} to {}/{}), \
                     please enter file path (or press Enter to skip these Hunks): ",
                    start_hunk, end_hunk, total_hunks
                );
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                target_file = input.trim().to_string();
                if target_file.is_empty() {
                    println!("⏭️  Skipped Hunk {} to {}", start_hunk, end_hunk);
                    global_hunk_idx += file_blocks.len();
                    continue;
                }
            }
        }

        println!("\n========================================");
        println!("📄 Processing file: {}", target_file);
        println!("========================================");

        let mut current_content = match fs::read_to_string(&target_file) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                eprintln!("❌ Failed to read source file {}: {}", target_file, e);
                continue;
            }
        };
        let had_trailing_newline = current_content.ends_with('\n');
        let mut file_changed = false;

        for b in file_blocks {
            global_hunk_idx += 1;

            let original_lines: Vec<&str> = current_content.lines().collect();
            let search_lines: Vec<&str> = b.search.lines().collect();
            let replace_lines: Vec<&str> = b.replace.lines().collect();

            // Handle new file creation or replacing entire empty file content
            if search_lines.is_empty() {
                if !original_lines.is_empty() {
                    eprintln!(
                        "\n❌ Failed to apply Hunk {}: SEARCH block is empty but file is not empty",
                        global_hunk_idx
                    );
                    continue;
                }

                let new_content = b.replace.clone();
                let color_code = "New File".green().to_string();

                println!("\n────────────────────────────────────────");
                println!(
                    "🧩 Hunk {}/{}  [Match: {}]",
                    global_hunk_idx, total_hunks, color_code
                );
                println!("────────────────────────────────────────");

                show_hunk_diff(&[], &replace_lines);

                print!(
                    "\nApply this Hunk? {}  Hunks [{}/{}]  [{}] [Y/n] ",
                    target_file.green(),
                    global_hunk_idx,
                    total_hunks,
                    color_code
                );
                io::stdout().flush().ok();
                let mut ans = String::new();
                io::stdin().read_line(&mut ans).ok();
                let ans = ans.trim().to_lowercase();

                if ans == "y" || ans.is_empty() {
                    current_content = new_content;
                    file_changed = true;
                    println!("✅ Hunk {} staged", global_hunk_idx);
                } else {
                    println!("⏭️  Hunk {} skipped", global_hunk_idx);
                }
                continue;
            }

            let (index, similarity) = find_best_match(&original_lines, &search_lines);

            if let Some((start, end)) = index {
                if similarity >= 0.7 {
                    let mut new_lines = original_lines.clone();
                    new_lines.splice(start..end, replace_lines.iter().cloned());
                    let new_content = new_lines.join("\n");

                    let match_percent = (similarity * 100.0).round() as i32;
                    let color_code = if match_percent == 100 {
                        "100.0%".green().to_string()
                    } else if match_percent >= 70 {
                        format!("{}%", match_percent).yellow().to_string()
                    } else {
                        format!("{}%", match_percent)
                            .truecolor(255, 165, 0)
                            .to_string()
                    };

                    println!("\n────────────────────────────────────────");
                    println!(
                        "🧩 Hunk {}/{}  [Match: {}]",
                        global_hunk_idx, total_hunks, color_code
                    );
                    println!("────────────────────────────────────────");

                    show_hunk_diff(&search_lines, &replace_lines);

                    print!(
                        "\nApply this Hunk? {}  Hunks [{}/{}]  [{}] [Y/n] ",
                        target_file.green(),
                        global_hunk_idx,
                        total_hunks,
                        color_code
                    );
                    io::stdout().flush().ok();
                    let mut ans = String::new();
                    io::stdin().read_line(&mut ans).ok();
                    let ans = ans.trim().to_lowercase();

                    if ans == "y" || ans.is_empty() {
                        current_content = new_content;
                        file_changed = true;
                        println!("✅ Hunk {} staged", global_hunk_idx);
                    } else {
                        println!("⏭️  Hunk {} skipped", global_hunk_idx);
                    }
                } else {
                    eprintln!(
                        "\n❌ Failed to apply Hunk {}: Similarity below threshold ({}%)",
                        global_hunk_idx,
                        (similarity * 100.0).round() as i32
                    );
                }
            } else {
                eprintln!(
                    "\n❌ Failed to apply Hunk {}: No match found",
                    global_hunk_idx
                );
            }
        }

        if file_changed {
            if had_trailing_newline && !current_content.ends_with('\n') {
                current_content.push('\n');
            }
            match fs::write(&target_file, &current_content) {
                Ok(_) => println!("\n💾 Wrote all approved Hunks to {}", target_file),
                Err(e) => eprintln!("\n❌ Failed to write file {}: {}", target_file, e),
            }
        } else {
            println!("\n🚫 {} unchanged", target_file);
        }
    }

    Ok(())
}

/// Strip markdown code fences and extract only SEARCH/REPLACE blocks.
/// Attempts to infer the file path from the comment line preceding each
/// `<<<<<<< SEARCH` marker; falls back to `unknown_file`.
fn sanitize_llm_output(text: &str, debug_mode: bool) -> String {
    let mut log = String::from("[Sanitize] Starting cleanup...\n");
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().starts_with("```"))
        .collect();
    log.push_str(&format!(
        "[Sanitize] {} lines remaining after markdown removal\n",
        lines.len()
    ));

    let mut clean_lines = Vec::new();
    let mut capturing = false;

    for i in 0..lines.len() {
        let line = lines[i];
        if line.trim().starts_with("<<<<<<< SEARCH") {
            capturing = true;
            log.push_str(&format!(
                "[Sanitize] Line {}: Found SEARCH block start\n",
                i + 1
            ));

            let prev_line = if i > 0 { lines[i - 1] } else { "" };
            if prev_line.starts_with("// ")
                || prev_line.starts_with("# ")
                || prev_line.starts_with("File: ")
            {
                clean_lines.push(prev_line.to_string());
                log.push_str(&format!("[Sanitize] Extracted file path: {}\n", prev_line));
            } else {
                clean_lines.push("// unknown_file".to_string());
                log.push_str("[Sanitize] No file path extracted, using unknown_file\n");
            }
        }

        if capturing {
            clean_lines.push(line.to_string());
            if line.trim().starts_with(">>>>>>> REPLACE") {
                capturing = false;
                clean_lines.push(String::new());
                log.push_str(&format!(
                    "[Sanitize] Line {}: Found REPLACE block end\n",
                    i + 1
                ));
            }
        }
    }

    if debug_mode {
        log.push_str("\n[Sanitize] Cleanup complete.\n");
        let _ = fs::write("debug.log", log);
    }
    clean_lines.join("\n")
}

/// Parse sanitised text into `Block` structs.
/// File paths are inferred from `// path`, `# path`, `File: path`, or a
/// bare `name.ext` line preceding each SEARCH/REPLACE group.
fn parse_search_replace(diff_content: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current_file = String::new();
    let mut in_search = false;
    let mut in_replace = false;
    let mut search_lines = Vec::new();
    let mut replace_lines = Vec::new();

    for line in diff_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<<<<<<< SEARCH") {
            in_search = true;
            in_replace = false;
            search_lines.clear();
        } else if trimmed == "=======" {
            in_search = false;
            in_replace = true;
            replace_lines.clear();
        } else if trimmed.starts_with(">>>>>>> REPLACE") {
            if !current_file.is_empty() {
                blocks.push(Block {
                    file_path: current_file.clone(),
                    search: search_lines.join("\n"),
                    replace: replace_lines.join("\n"),
                });
            }
            in_search = false;
            in_replace = false;
        } else if !in_search && !in_replace {
            if line.starts_with("// ") || line.starts_with("# ") {
                current_file = line[2..].trim().to_string();
            } else if trimmed.starts_with("File: ") {
                current_file = trimmed[6..].trim().to_string();
            } else if trimmed.matches('.').count() == 1 && !trimmed.contains(' ') {
                current_file = trimmed.to_string();
            }
        } else if in_search {
            search_lines.push(line.to_string());
        } else if in_replace {
            replace_lines.push(line.to_string());
        }
    }
    blocks
}

/// Sliding-window fuzzy match: finds the position in `original_lines` whose
/// content best matches `search_lines`, using line-level similarity ratio.
/// Ignores empty lines and comments for a "smart" match.
/// Returns `(Some((start, end)), similarity)` or `(None, 0.0)` if no window fits.
fn find_best_match(original_lines: &[&str], search_lines: &[&str]) -> (Option<(usize, usize)>, f64) {
    if search_lines.is_empty() || search_lines.len() > original_lines.len() {
        return (None, 0.0);
    }

    let norm_orig: Vec<(usize, String)> = original_lines
        .iter()
        .enumerate()
        .map(|(i, l)| (i, normalize_for_match(l)))
        .filter(|(_, l)| !l.is_empty())
        .collect();

    let norm_search: Vec<String> = search_lines
        .iter()
        .map(|l| normalize_for_match(l))
        .filter(|l| !l.is_empty())
        .collect();

    if norm_search.is_empty() || norm_search.len() > norm_orig.len() {
        return find_best_match_raw(original_lines, search_lines);
    }

    let mut best_match = None;
    let mut best_sim = 0.0;
    let search_str = norm_search.join("\n");

    for i in 0..=(norm_orig.len() - norm_search.len()) {
        let chunk_str = norm_orig[i..i + norm_search.len()]
            .iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let diff = TextDiff::from_lines(&chunk_str, &search_str);
        let mut changes = 0;
        let mut total = 0;
        for change in diff.iter_all_changes() {
            total += 1;
            if change.tag() != ChangeTag::Equal {
                changes += 1;
            }
        }

        let sim = if total == 0 { 1.0 } else { 1.0 - (changes as f64 / total as f64) };
        if sim > best_sim {
            best_sim = sim;
            let start = norm_orig[i].0;
            let end = norm_orig[i + norm_search.len() - 1].0 + 1;
            best_match = Some((start, end));
        }
        if sim == 1.0 {
            break;
        }
    }

    (best_match, best_sim)
}

fn find_best_match_raw(original_lines: &[&str], search_lines: &[&str]) -> (Option<(usize, usize)>, f64) {
    let mut best_match = None;
    let mut best_sim = 0.0;
    let search_str = search_lines.join("\n");

    for i in 0..=(original_lines.len() - search_lines.len()) {
        let chunk_str = original_lines[i..i + search_lines.len()].join("\n");
        let diff = TextDiff::from_lines(&chunk_str, &search_str);
        let mut changes = 0;
        let mut total = 0;
        for change in diff.iter_all_changes() {
            total += 1;
            if change.tag() != ChangeTag::Equal {
                changes += 1;
            }
        }
        let sim = if total == 0 { 1.0 } else { 1.0 - (changes as f64 / total as f64) };
        if sim > best_sim {
            best_sim = sim;
            best_match = Some((i, i + search_lines.len()));
        }
        if sim == 1.0 {
            break;
        }
    }
    (best_match, best_sim)
}

/// Pretty-print a coloured diff between the SEARCH and REPLACE blocks so the
/// user can review what will change before confirming.
fn show_hunk_diff(search: &[&str], replace: &[&str]) {
    let search_str = search.join("\n");
    let replace_str = replace.join("\n");
    let diff = TextDiff::from_lines(&search_str, &replace_str);

    for op in diff.ops() {
        for change in diff.iter_changes(&op) {
            let (sign, color) = match change.tag() {
                ChangeTag::Insert => ("+", Color::Green),
                ChangeTag::Delete => ("-", Color::Red),
                ChangeTag::Equal => ("=", Color::BrightBlack),
            };

            for line in change.value().lines() {
                println!("{} {}", sign.color(color), line.color(color));
            }
        }
    }
}
