# cxt - Content Aggregation Tool

A command-line tool written in Rust that aggregates the contents of specified files and directories into a single string, then directs it to the clipboard (default), a file, or standard output.

## Features

- **File and Directory Support**: Read individual files or recursively walk through directories
- **Multiple Output Destinations**: Copy to clipboard, write to file, or print to stdout
- **Flexible Path Formatting**: Use absolute paths, relative paths, or no path headers
- **Interactive File Conflict Resolution**: Choose to replace, append, or cancel when writing to existing files
- **Cross-Platform Clipboard Support**: Works on Linux (X11 & Wayland), macOS, and Windows
- **Robust Error Handling**: User-friendly error messages and graceful failure handling

## Installation

### From Source

```bash
git clone <repository-url>
cd cxt
cargo build --release
```

The binary will be available at `target/release/cxt`.

### Using Cargo

```bash
cargo install --git <repository-url>
```

## Usage

### Basic Usage

```bash
# Copy content from a single file to clipboard
cxt README.md

# Copy content from multiple files to clipboard
cxt file1.txt file2.txt file3.txt

# Copy content from a directory recursively to clipboard
cxt ./src/

# Copy content from mixed paths to clipboard
cxt README.md ./src/ config.json
```

### Output Destinations

#### Print to stdout
```bash
# Print content to terminal (also copies to clipboard)
cxt -p README.md

# Print content without copying to clipboard
cxt -n -p README.md
```

#### Write to file
```bash
# Write content to a file
cxt -w output.txt README.md

# Write content with relative paths
cxt -r -w context.txt ./src/
```

### Path Formatting Options

#### Relative paths
```bash
# Use relative paths in headers
cxt -r ./src/
```

#### No path headers
```bash
# Disable path headers (raw content only)
cxt -n README.md

# Combine with other options
cxt -n -p README.md
cxt -n -w output.txt ./src/
```

### Examples

```bash
# 1. Default: Absolute paths, copied to clipboard
cxt ./src

# 2. Relative paths, copied to clipboard
cxt -r ./src

# 3. No paths, printed to terminal
cxt -n -p ./README.md

# 4. Relative paths, written to a file
cxt -r -w context.txt ./src ./README.md

# 5. This will cause an error due to conflicting flags
cxt -r --no-path .

# 6. Show the help message
cxt --help
```

## Command Line Options

| Flag | Description |
|------|-------------|
| `-p, --print` | Print content to stdout |
| `-w, --write <FILE_PATH>` | Write content to specified file |
| `-r, --relative` | Use relative paths in headers |
| `-n, --no-path` | Disable file path headers |
| `-h, --help` | Show help message |
| `-V, --version` | Show version information |

## File Conflict Resolution

When writing to a file that already exists, `cxt` will prompt you to choose:

- **[R]eplace**: Overwrite the existing file
- **[A]ppend**: Add the content to the end of the file
- **[C]ancel**: Abort the operation

## Error Handling

The application provides clear error messages for common issues:

- Non-existent paths
- Permission errors
- Clipboard unavailability
- File system errors

## Technical Details

### Dependencies

- **clap**: Command-line argument parsing
- **arboard**: Cross-platform clipboard support
- **anyhow**: Error handling
- **walkdir**: Directory traversal
- **dialoguer**: Interactive prompts

### Architecture

The application is structured into several modules for maintainability and extensibility:

- `cli.rs`: Command-line interface and argument parsing
- `content_aggregator.rs`: File and directory content aggregation
- `output_handler.rs`: Output destination management
- `path_formatter.rs`: Path formatting and header generation

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details. 