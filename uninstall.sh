#!/bin/sh
# Uninstaller for cxt

set -e

# Defaults (should match your install script)
BIN_DIR_DEFAULT="${HOME}/.local/bin"
MAN_DIR_DEFAULT="${HOME}/.local/share/man"
SUDO_DEFAULT="sudo"

BIN_DIR="${BIN_DIR:-$BIN_DIR_DEFAULT}"
MAN_DIR="${MAN_DIR:-$MAN_DIR_DEFAULT}"
SUDO="${SUDO:-$SUDO_DEFAULT}"

# Allow overrides via command line
while [ "$#" -gt 0 ]; do
    case "$1" in
        --bin-dir) BIN_DIR="$2"; shift 2 ;;
        --bin-dir=*) BIN_DIR="${1#*=}"; shift 1 ;;
        --man-dir) MAN_DIR="$2"; shift 2 ;;
        --man-dir=*) MAN_DIR="${1#*=}"; shift 1 ;;
        --sudo) SUDO="$2"; shift 2 ;;
        --sudo=*) SUDO="${1#*=}"; shift 1 ;;
        -h|--help)
            echo "Usage: uninstall.sh [--bin-dir DIR] [--man-dir DIR] [--sudo CMD]"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

try_sudo() {
    if "$@" >/dev/null 2>&1; then
        return 0
    fi

    need_sudo
    "${SUDO}" "$@"
}

need_sudo() {
    if ! check_cmd "${SUDO}"; then
        err "\
could not find the command \`${SUDO}\` needed to get permissions for uninstall.

If you are on Windows, please run your shell as an administrator, then rerun this script.
Otherwise, please run this script as root, or install \`sudo\`."
    fi

    if ! "${SUDO}" -v; then
        err "sudo permissions not granted, aborting uninstallation"
    fi
}

check_cmd() {
    command -v -- "$1" >/dev/null 2>&1
}

err() {
    echo "Error: $1" >&2
    exit 1
}

BIN_PATH="$BIN_DIR/cxt"
BIN_PATH_WIN="$BIN_DIR/cxt.exe"
MAN_PATH="$MAN_DIR/man1/cxt.1"

echo "Uninstalling cxt..."

# Remove binary (Linux/macOS)
if [ -f "$BIN_PATH" ]; then
    try_sudo rm -f "$BIN_PATH"
    echo "Removed $BIN_PATH"
fi

# Remove binary (Windows/MSYS)
if [ -f "$BIN_PATH_WIN" ]; then
    try_sudo rm -f "$BIN_PATH_WIN"
    echo "Removed $BIN_PATH_WIN"
fi

# Remove man page
if [ -f "$MAN_PATH" ]; then
    try_sudo rm -f "$MAN_PATH"
    echo "Removed $MAN_PATH"
fi

# Optionally, remove empty man1 and man directories
if [ -d "$MAN_DIR/man1" ] && [ ! "$(ls -A "$MAN_DIR/man1")" ]; then
    try_sudo rmdir "$MAN_DIR/man1"
    echo "Removed empty $MAN_DIR/man1"
fi
if [ -d "$MAN_DIR" ] && [ ! "$(ls -A "$MAN_DIR")" ]; then
    try_sudo rmdir "$MAN_DIR"
    echo "Removed empty $MAN_DIR"
fi

echo "cxt has been uninstalled."

exit 0
