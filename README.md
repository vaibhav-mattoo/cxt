# cxt: Context Extractor

[![Crates.io](https://img.shields.io/crates/v/cxt)](https://crates.io/crates/cxt)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Homebrew Tap](https://img.shields.io/badge/homebrew--tap-v0.1.7-blue?logo=homebrew)](https://github.com/vaibhav-mattoo/homebrew-cxt)
[![AUR version](https://img.shields.io/aur/version/cxt?logo=arch-linux)](https://aur.archlinux.org/packages/cxt)
[![Last Commit](https://img.shields.io/github/last-commit/vaibhav-mattoo/cxt)](https://github.com/vaibhav-mattoo/cxt/commits)

Aggregate files and directories quickly into a single clipboard-ready context block, tuned for feeding code to AI assistants.



https://github.com/user-attachments/assets/a8953c2f-f888-46a2-a514-4317ab511b04



---

## Installation

**Install script (all platforms)**

```bash
curl -sSfL https://raw.githubusercontent.com/vaibhav-mattoo/cxt/main/install.sh | sh
```

If prompted, add `~/.local/bin` to your PATH: `export PATH="$HOME/.local/bin:$PATH"`

**Cargo**

```bash
cargo install cxt
```

**Homebrew (macOS / Linux)**

```bash
brew tap vaibhav-mattoo/cxt && brew install cxt
```

**AUR (Arch Linux)**

```bash
yay -S cxt   # or: paru -S cxt
```

**From source**

```bash
git clone https://github.com/vaibhav-mattoo/cxt.git
cd cxt && cargo install --path .
```

---

## Quick Start

```bash
# for tui (recommended)
cxt

# or cli
cxt src/ README.md        # aggregate paths → clipboard
```

---

## Usage

### Input sources

```bash
# Explicit paths (shell globs are expanded automatically)
cxt src/ tests/ README.md Cargo.toml

# any newline-delimited list of paths
fd -e rs | cxt
git diff --name-only HEAD | cxt
cat file_list.txt | cxt

# Interactive TUI picker
cxt
cxt --tui

# Single image copies the image itself to clipboard (not its path)
cxt screenshot.png
```

### Output destination

```bash
cxt src/                          # clipboard (default)
cxt -p src/                       # stdout + clipboard
cxt -w context.xml src/           # write to file
cxt -w snapshot.gz --compress src/ # write gzip-compressed file
                                    # Decompress: gunzip snapshot.gz
                                    # View:       zcat snapshot.gz | less
```

### Output format

```bash
cxt --format xml src/       # default : XML with <file path="…"> tags
cxt --format markdown src/  # Markdown with ## headings and fenced code blocks
```

**XML (default)**

```xml
<context>
<file path="/abs/path/to/main.rs">
fn main() { … }
</file>
</context>
```

**Markdown**

```
## File: src/main.rs

```rust
fn main() { … }
```

```

### Path headers

```bash
cxt src/           # absolute paths (default)
cxt -r src/        # relative to current directory
cxt -n src/        # no headers, raw content only
cxt --hidden src/  # include hidden / dot files
```

### Filtering

**Language and extension filters**

```bash
cxt --lang rust src/           # Rust files only  →  rs, toml
cxt --lang python .            # Python files only → py, pyi, pyw
cxt --lang js --lang ts src/   # multiple languages
cxt --ext rs,toml src/         # specific extensions
cxt --lang rust --ext md src/  # combine --lang and --ext
cxt --lang help                # list all supported languages and their extensions
```

Supported languages include: `rust`, `python`, `javascript`, `typescript`, `go`, `java`, `c`, `cpp`, `csharp`, `ruby`, `swift`, `kotlin`, `shell`, `html`, `css`, `sql`, `markdown`, `yaml`, `json`, `toml`, `nix`, `terraform`, `graphql`, `dockerfile`, and more.

**Ignore paths and glob patterns** (`-i` is repeatable)

```bash
cxt -i target/ src/                   # ignore exact path
cxt -i "*.min.js" src/                # ignore by filename glob
cxt -i "**/__pycache__" .             # ignore by path glob
cxt -i node_modules/ -i "*.lock" .   # combine multiple ignores

```

> **Binary files** are detected automatically and skipped with a warning.

---

## TUI Mode

Launch with `cxt` or `cxt --tui` to browse and select files interactively.

| Key | Action |
|-----|--------|
| `↑` / `↓` / `j` / `k` | Move cursor |
| `→` / `l` / `Enter` | Expand directory |
| `←` / `h` / `Backspace` | Collapse / go to parent directory |
| `Space` | Select / unselect file or directory |
| `/ or ctrl-f` | Enter fuzzy search |
| `?` | Toggle keybinding help overlay |
| `r` | Toggle relative path headers |
| `n` | Toggle no path headers |
| `c` | Confirm selection → copy to clipboard |
| `q` / `Ctrl-c` | Quit |

---

## All Options

| Flag | Description |
|------|-------------|
| `-p, --print` | Print to stdout (also copies to clipboard) |
| `-w, --write <FILE>` | Write output to a file |
| `--compress` | Gzip-compress output, requires `--write` |
| `--format <xml\|markdown>` | Output format (default: `xml`) |
| `-r, --relative` | Use relative paths in headers |
| `-n, --no-path` | Omit file path headers |
| `--hidden` | Include hidden / dot files |
| `-i, --ignore <PATH>` | Ignore a path or glob pattern, repeatable |
| `--ext <EXT[,EXT…]>` | Include only files with these extensions, repeatable |
| `--lang <LANG[,LANG…]>` | Include only files for this language, repeatable |
| `--no-sort` | Non-deterministic output order (faster for large trees) |
| `-t, --tui` | Launch interactive TUI file picker |

---

## Uninstall

```bash
curl -sSfL https://raw.githubusercontent.com/vaibhav-mattoo/cxt/main/uninstall.sh | sh
```

For package manager installs, use the respective remove command (`cargo uninstall cxt`, `brew uninstall cxt`, `yay -R cxt`).

---

## License

MIT : see [LICENSE](LICENSE).
