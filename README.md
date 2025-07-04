# cxt - Context Extractor

A command-line tool that aggregates file and directory contents into your clipboard, perfect for providing project context to AI chatbots in your browser like ChatGPT, Perplexity etc.

## Use Case

When you're working in the terminal and need to quickly share your project's code structure and contents with an AI assistant, `cxt` makes it effortless. Instead of manually copying files one by one, you can select multiple files and directories, and `cxt` will aggregate all their contents with clear path headers, giving the AI full context of your project structure.

## Installation

### From Source

```bash
git clone https://github.com/yourusername/cxt.git
cd cxt
cargo install --path .
```

### From Cargo

```bash
cargo install cxt
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

## Requirements

- Rust 1.74.0 or later
- For clipboard support:
  - **Linux**: 
    - **Wayland**: Any of these clipboard managers: `wl-clipboard`(default), `copyq`, `clipman`, `cliphist`, `gpaste`, `clipse`
    - **X11**: Any of these clipboard managers: `xclip` (default) `copyq`, `gpaste`, `klipper`
    - **Universal**: Any of the above clipboard managers which work on both X11 and Wayland
  - **macOS**: Built-in clipboard support
  - **Windows**: Built-in clipboard support

## License

MIT License - see LICENSE file for details. 
