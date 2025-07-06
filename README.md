# cxt - Context Extractor

[![Crates.io](https://img.shields.io/crates/v/cxt)](https://crates.io/crates/cxt)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Homebrew](https://img.shields.io/badge/homebrew-v0.1.0-blue?logo=homebrew)](https://formulae.brew.sh/formula/cxt)
[![AUR version](https://img.shields.io/aur/version/cxt?logo=arch-linux)](https://aur.archlinux.org/packages/cxt)
[![Build Status](https://github.com/vaibhav-mattoo/cxt/actions/workflows/ci.yml/badge.svg)](https://github.com/vaibhav-mattoo/cxt/actions)
[![Last Commit](https://img.shields.io/github/last-commit/vaibhav-mattoo/cxt)](https://github.com/vaibhav-mattoo/cxt/commits)

A command-line tool that aggregates file and directory contents into your clipboard, perfect for providing project context to AI chatbots in your browser like ChatGPT, Perplexity etc.

## Use Case

When you're working in the terminal and need to quickly share your project's code structure and contents with an AI assistant, `cxt` makes it effortless. Instead of manually copying files one by one, you can select multiple files and directories, and `cxt` will aggregate all their contents with clear path headers, giving the AI full context of your project structure.

## Installation

### Universal Install Script

The easiest way to install `cxt` on any system:

```bash
curl -sSfL https://raw.githubusercontent.com/vaibhav-mattoo/cxt/main/install.sh | sh
```

This script will automatically detect your system and install the appropriate binary.

Remember to add `~/.local/bin` to your `$PATH` as the script says, by adding `export PATH="$HOME/.local/bin:$PATH"` in the end of your shell config(~/.bashrc, ~/.zshrc etc).

### From Cargo

```bash
cargo install cxt
```

### Using homebrew

You can install `cxt` through brew on Linux or macOS by:

```bash
brew tap vaibhav-mattoo/cxt
brew install cxt
```

### On Arch Linux (AUR)

You can install `cxt` directly from the AUR:

```bash
yay -S cxt
# or
paru -S cxt
```

### From Source

```bash
git clone https://github.com/vaibhav-mattoo/cxt.git
cd cxt
cargo install --path .
```

## Quick Start

### Interactive Mode
Launch the interactive file selector:

```bash
cxt --tui
```

Navigate with arrow keys or `hjkl`, select files/directories with `Space`, and confirm with `c`.

### Command Line Mode
Copy specific files to clipboard:

```bash
cxt file1.txt file2.py src/
```

## Usage Examples

### Basic Usage

```bash
# Copy a single file to clipboard
cxt main.rs

# Copy multiple files and directories
cxt src/ tests/ README.md Cargo.toml

# Print to stdout and copy to clipboard
cxt -p src/

# Write to a file and copy to clipboard
cxt -w output.txt src/
```

### Path Formatting Options

```bash
# Use relative paths in headers
cxt -r src/

# Disable path headers entirely
cxt -n src/

# Include hidden files when walking directories
cxt --hidden src/
```

### Combining Options

```bash
# Print to stdout with relative paths and copy to clipboard
cxt -p -r src/ tests/

# Write to file with no path headers
cxt -w output.txt -n src/

# Ignore a directory while copying a project
cxt -i bin src/
```

## Interactive TUI Mode

The Terminal User Interface (TUI) provides an intuitive way to browse and select files:

### Navigation
- **Arrow keys** or **hjkl**: Move cursor
- **Space**: Select/unselect file or directory
- **Enter** or **l** or **→**: Open directory
- **Backspace** or **h** or **←**: Go to parent directory
- **c**: Confirm selection and exit
- **q**: Quit without selection

### TUI Features
- **Visual selection**: Selected items are highlighted
- **Directory expansion**: Selecting a directory includes all files within it
- **Path toggles**: Press `r` to toggle relative paths, `n` to toggle no path headers

## Command Line Options

### Output Options
- `-p, --print`: Print content to stdout (also copies to clipboard)
- `-w, --write <FILE>`: Write content to specified file
- `-t, --tui`: Launch interactive TUI file selector

### Path Formatting
- `-r, --relative`: Use relative paths in headers
- `-n, --no-path`: Disable file path headers
- `--hidden`: Include hidden files when walking directories
- `-i, --ignore <PATH>`: Ignore a file or directory (only one allowed)

### Examples

```bash
# Interactive selection
cxt --tui

# Print with relative paths
cxt -p -r src/

# Write to file, no path headers
cxt -w context.txt -n src/ tests/

# Include hidden files
cxt --hidden src/
```

## Output Format

By default, `cxt` adds path headers to distinguish between files:

```
--- File: /path/to/file1.txt ---
Content of file1.txt

--- File: /path/to/file2.py ---
Content of file2.py
```

With `--relative`, paths are shown relative to current directory:

```
--- File: file1.txt ---
Content of file1.txt

--- File: src/file2.py ---
Content of file2.py
```

With `--no-path`, only raw content is included:

```
Content of file1.txt
Content of file2.py
```

## Use Cases

Perfect for providing project context to AI assistants and sharing code snippets with colleagues:

```bash
# Quick project overview
cxt --tui

# Specific files for debugging
cxt -r main.rs error.log

# Full project structure
cxt -r src/ tests/ README.md
```

## License

MIT License - see LICENSE file for details. 

## Uninstall

To uninstall `cxt`, you can run the command:

```bash
curl -sSfL https://raw.githubusercontent.com/vaibhav-mattoo/cxt/main/uninstall.sh | sh
```

If you installed the software using a package manager, remove it using the package manager’s uninstall command.
