#!/bin/sh
# The official cxt installer
# Supports Linux, macOS, BSD, Windows (MSYS2/Git Bash/Cygwin), all major architectures.

main() {
    if [ "${KSH_VERSION-}" = 'Version JM 93t+ 2010-03-05' ]; then
        err 'the installer does not work with this ksh93 version; please try bash'
    fi

    set -u
    parse_args "$@"

    local _arch
    _arch="${ARCH:-$(ensure get_architecture)}"
    assert_nz "${_arch}" "arch"
    echo "Detected architecture: ${_arch}"

    local _bin_name
    case "${_arch}" in
    *windows*) _bin_name="cxt.exe" ;;
    *) _bin_name="cxt" ;;
    esac

    local _tmp_dir
    _tmp_dir="$(mktemp -d)" || err "mktemp: could not create temporary directory"
    cd "${_tmp_dir}" || err "cd: failed to enter directory: ${_tmp_dir}"

    local _package
    _package="$(ensure download_cxt "${_arch}")"
    assert_nz "${_package}" "package"
    echo "Downloaded package: ${_package}"
    case "${_package}" in
    *.tar.gz)
        need_cmd tar
        ensure tar -xf "${_package}"
        ;;
    *.zip)
        need_cmd unzip
        ensure unzip -oq "${_package}"
        ;;
    *)
        err "unsupported package format: ${_package}"
        ;;
    esac

    ensure try_sudo mkdir -p -- "${BIN_DIR}"
    ensure try_sudo cp -- "${_bin_name}" "${BIN_DIR}/${_bin_name}"
    ensure try_sudo chmod +x "${BIN_DIR}/${_bin_name}"
    echo "Installed cxt to ${BIN_DIR}"

    # Install manpages if present
    if [ -d "man/man1" ]; then
        ensure try_sudo mkdir -p -- "${MAN_DIR}/man1"
        ensure try_sudo cp -- "man/man1/"* "${MAN_DIR}/man1/"
        echo "Installed manpages to ${MAN_DIR}"
    fi

    echo ""
    echo "cxt is installed!"
    if ! echo ":${PATH}:" | grep -Fq ":${BIN_DIR}:"; then
        echo "Note: ${BIN_DIR} is not on your \$PATH. cxt will not work unless it is added to \$PATH."
    fi
}

parse_args() {
    BIN_DIR_DEFAULT="${HOME}/.local/bin"
    MAN_DIR_DEFAULT="${HOME}/.local/share/man"
    SUDO_DEFAULT="sudo"

    BIN_DIR="${BIN_DIR_DEFAULT}"
    MAN_DIR="${MAN_DIR_DEFAULT}"
    SUDO="${SUDO_DEFAULT}"

    while [ "$#" -gt 0 ]; do
        case "$1" in
        --arch) ARCH="$2" && shift 2 ;;
        --arch=*) ARCH="${1#*=}" && shift 1 ;;
        --bin-dir) BIN_DIR="$2" && shift 2 ;;
        --bin-dir=*) BIN_DIR="${1#*=}" && shift 1 ;;
        --man-dir) MAN_DIR="$2" && shift 2 ;;
        --man-dir=*) MAN_DIR="${1#*=}" && shift 1 ;;
        --sudo) SUDO="$2" && shift 2 ;;
        --sudo=*) SUDO="${1#*=}" && shift 1 ;;
        -h | --help) usage && exit 0 ;;
        *) err "Unknown option: $1" ;;
        esac
    done
}

usage() {
    local _text_heading _text_reset
    _text_heading="$(tput bold || true 2>/dev/null)$(tput smul || true 2>/dev/null)"
    _text_reset="$(tput sgr0 || true 2>/dev/null)"

    local _arch
    _arch="$(get_architecture || true)"

    echo "\
${_text_heading}cxt installer${_text_reset}
Vaibhav Mattoo <vaibhavmattoo1@gmail.com>
https://github.com/vaibhav-mattoo/cxt

Fetches and installs cxt. If cxt is already installed, it will be updated to the latest version.

${_text_heading}Usage:${_text_reset}
  install.sh [OPTIONS]

${_text_heading}Options:${_text_reset}
      --arch     Override the architecture identified by the installer [current: ${_arch}]
      --bin-dir  Override the installation directory [default: ${BIN_DIR_DEFAULT}]
      --man-dir  Override the manpage installation directory [default: ${MAN_DIR_DEFAULT}]
      --sudo     Override the command used to elevate to root privileges [default: ${SUDO_DEFAULT}]
  -h, --help     Print help"
}

download_cxt() {
    local _arch="$1"

    if check_cmd curl; then
        _dld=curl
    elif check_cmd wget; then
        _dld=wget
    else
        need_cmd 'curl or wget'
    fi
    need_cmd grep

    local _releases_url="https://api.github.com/repos/vaibhav-mattoo/cxt/releases/latest"
    local _releases
    case "${_dld}" in
    curl) _releases="$(curl -sL "${_releases_url}")" ||
        err "curl: failed to download ${_releases_url}" ;;
    wget) _releases="$(wget -qO- "${_releases_url}")" ||
        err "wget: failed to download ${_releases_url}" ;;
    *) err "unsupported downloader: ${_dld}" ;;
    esac
    (echo "${_releases}" | grep -q 'API rate limit exceeded') &&
        err "you have exceeded GitHub's API rate limit. Please try again later, or use a different installation method: https://github.com/vaibhav-mattoo/cxt/#installation"

    local _package_url
    _package_url="$(echo "${_releases}" | grep "browser_download_url" | cut -d '"' -f 4 | grep -- "${_arch}")" ||
        err "cxt has not yet been packaged for your architecture (${_arch}), please file an issue: https://github.com/vaibhav-mattoo/cxt/issues"

    local _ext
    case "${_package_url}" in
    *.tar.gz) _ext="tar.gz" ;;
    *.zip) _ext="zip" ;;
    *) err "unsupported package format: ${_package_url}" ;;
    esac

    local _package="cxt.${_ext}"
    case "${_dld}" in
    curl) _releases="$(curl -sLo "${_package}" "${_package_url}")" || err "curl: failed to download ${_package_url}" ;;
    wget) _releases="$(wget -qO "${_package}" "${_package_url}")" || err "wget: failed to download ${_package_url}" ;;
    *) err "unsupported downloader: ${_dld}" ;;
    esac

    echo "${_package}"
}

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
could not find the command \`${SUDO}\` needed to get permissions for install.

If you are on Windows, please run your shell as an administrator, then rerun this script.
Otherwise, please run this script as root, or install \`sudo\`."
    fi

    if ! "${SUDO}" -v; then
        err "sudo permissions not granted, aborting installation"
    fi
}

# The below functions are unchanged from zoxide's script and support all platforms.
get_architecture() {
    local _ostype _cputype _bitness _arch _clibtype
    _ostype="$(uname -s)"
    _cputype="$(uname -m)"
    _clibtype="musl"

    if [ "${_ostype}" = Linux ]; then
        if [ "$(uname -o || true)" = Android ]; then
            _ostype=Android
        fi
    fi

    if [ "${_ostype}" = Darwin ] && [ "${_cputype}" = i386 ]; then
        if sysctl hw.optional.x86_64 | grep -q ': 1'; then
            _cputype=x86_64
        fi
    fi

    if [ "${_ostype}" = SunOS ]; then
        if [ "$(/usr/bin/uname -o || true)" = illumos ]; then
            _ostype=illumos
        fi
        if [ "${_cputype}" = i86pc ]; then
            _cputype="$(isainfo -n)"
        fi
    fi

    case "${_ostype}" in
    Android)
        _ostype=linux-android
        ;;
    Linux)
        check_proc
        _ostype=unknown-linux-${_clibtype}
        _bitness=$(get_bitness)
        ;;
    FreeBSD)
        _ostype=unknown-freebsd
        ;;
    NetBSD)
        _ostype=unknown-netbsd
        ;;
    DragonFly)
        _ostype=unknown-dragonfly
        ;;
    Darwin)
        _ostype=apple-darwin
        ;;
    illumos)
        _ostype=unknown-illumos
        ;;
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        _ostype=pc-windows-msvc
        ;;
    *)
        err "unrecognized OS type: ${_ostype}"
        ;;
    esac

    case "${_cputype}" in
    i386 | i486 | i686 | i786 | x86)
        _cputype=i686
        ;;
    xscale | arm)
        _cputype=arm
        if [ "${_ostype}" = "linux-android" ]; then
            _ostype=linux-androideabi
        fi
        ;;
    armv6l)
        _cputype=arm
        if [ "${_ostype}" = "linux-android" ]; then
            _ostype=linux-androideabi
        else
            _ostype="${_ostype}eabihf"
        fi
        ;;
    armv7l | armv8l)
        _cputype=armv7
        if [ "${_ostype}" = "linux-android" ]; then
            _ostype=linux-androideabi
        else
            _ostype="${_ostype}eabihf"
        fi
        ;;
    aarch64 | arm64)
        _cputype=aarch64
        ;;
    x86_64 | x86-64 | x64 | amd64)
        _cputype=x86_64
        ;;
    mips)
        _cputype=$(get_endianness mips '' el)
        ;;
    mips64)
        if [ "${_bitness}" -eq 64 ]; then
            _ostype="${_ostype}abi64"
            _cputype=$(get_endianness mips64 '' el)
        fi
        ;;
    ppc)
        _cputype=powerpc
        ;;
    ppc64)
        _cputype=powerpc64
        ;;
    ppc64le)
        _cputype=powerpc64le
        ;;
    s390x)
        _cputype=s390x
        ;;
    riscv64)
        _cputype=riscv64gc
        ;;
    *)
        err "unknown CPU type: ${_cputype}"
        ;;
    esac

    if [ "${_ostype}" = unknown-linux-musl ] && [ "${_bitness}" -eq 32 ]; then
        case ${_cputype} in
        x86_64)
            if is_host_amd64_elf; then {
                err "x32 userland is unsupported"
            }; else
                _cputype=i686
            fi
            ;;
        mips64)
            _cputype=$(get_endianness mips '' el)
            ;;
        powerpc64)
            _cputype=powerpc
            ;;
        aarch64)
            _cputype=armv7
            if [ "${_ostype}" = "linux-android" ]; then
                _ostype=linux-androideabi
            else
                _ostype="${_ostype}eabihf"
            fi
            ;;
        riscv64gc)
            err "riscv64 with 32-bit userland unsupported"
            ;;
        *) ;;
        esac
    fi

    if [ "${_ostype}" = "unknown-linux-musleabihf" ] && [ "${_cputype}" = armv7 ]; then
        if ensure grep '^Features' /proc/cpuinfo | grep -q -v neon; then
            _cputype=arm
        fi
    fi

    _arch="${_cputype}-${_ostype}"
    echo "${_arch}"
}

get_bitness() {
    need_cmd head
    local _current_exe_head
    _current_exe_head=$(head -c 5 /proc/self/exe)
    if [ "${_current_exe_head}" = "$(printf '\177ELF\001')" ]; then
        echo 32
    elif [ "${_current_exe_head}" = "$(printf '\177ELF\002')" ]; then
        echo 64
    else
        err "unknown platform bitness"
    fi
}

get_endianness() {
    local cputype="$1"
    local suffix_eb="$2"
    local suffix_el="$3"
    need_cmd head
    need_cmd tail
    local _current_exe_endianness
    _current_exe_endianness="$(head -c 6 /proc/self/exe | tail -c 1)"
    if [ "${_current_exe_endianness}" = "$(printf '\001')" ]; then
        echo "${cputype}${suffix_el}"
    elif [ "${_current_exe_endianness}" = "$(printf '\002')" ]; then
        echo "${cputype}${suffix_eb}"
    else
        err "unknown platform endianness"
    fi
}

is_host_amd64_elf() {
    need_cmd head
    need_cmd tail
    local _current_exe_machine
    _current_exe_machine=$(head -c 19 /proc/self/exe | tail -c 1)
    [ "${_current_exe_machine}" = "$(printf '\076')" ]
}

check_proc() {
    if ! test -L /proc/self/exe; then
        err "unable to find /proc/self/exe. Is /proc mounted? Installation cannot proceed without /proc."
    fi
}

need_cmd() {
    if ! check_cmd "$1"; then
        err "need '$1' (command not found)"
    fi
}

check_cmd() {
    command -v -- "$1" >/dev/null 2>&1
}

ensure() {
    if ! "$@"; then err "command failed: $*"; fi
}

assert_nz() {
    if [ -z "$1" ]; then err "found empty string: $2"; fi
}

err() {
    echo "Error: $1" >&2
    exit 1
}

{
    main "$@" || exit 1
}
