#!/bin/sh
# Uninstaller for cxt

set -e

# Defaults (should match your install script)
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
MAN_DIR="${MAN_DIR:-$HOME/.local/share/man}"

# Allow overrides via command line
while [ "$#" -gt 0 ]; do
    case "$1" in
        --bin-dir) BIN_DIR="$2"; shift 2 ;;
        --bin-dir=*) BIN_DIR="${1#*=}"; shift 1 ;;
        --man-dir) MAN_DIR="$2"; shift 2 ;;
        --man-dir=*) MAN_DIR="${1#*=}"; shift 1 ;;
        -h|--help)
            echo "Usage: uninstall.sh [--bin-dir DIR] [--man-dir DIR]"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

BIN_PATH="$BIN_DIR/cxt"
BIN_PATH_WIN="$BIN_DIR/cxt.exe"
MAN_PATH="$MAN_DIR/man1/cxt.1"

echo "Uninstalling cxt..."

# Remove binary (Linux/macOS)
if [ -f "$BIN_PATH" ]; then
    rm -f "$BIN_PATH"
    echo "Removed $BIN_PATH"
fi

# Remove binary (Windows/MSYS)
if [ -f "$BIN_PATH_WIN" ]; then
    rm -f "$BIN_PATH_WIN"
    echo "Removed $BIN_PATH_WIN"
fi

# Remove man page
if [ -f "$MAN_PATH" ]; then
    rm -f "$MAN_PATH"
    echo "Removed $MAN_PATH"
fi

# Optionally, remove empty man1 and man directories
if [ -d "$MAN_DIR/man1" ] && [ ! "$(ls -A "$MAN_DIR/man1")" ]; then
    rmdir "$MAN_DIR/man1"
    echo "Removed empty $MAN_DIR/man1"
fi
if [ -d "$MAN_DIR" ] && [ ! "$(ls -A "$MAN_DIR")" ]; then
    rmdir "$MAN_DIR"
    echo "Removed empty $MAN_DIR"
fi

echo "cxt has been uninstalled."

exit 0
