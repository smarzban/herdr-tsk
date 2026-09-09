#!/bin/sh
# Install a published tsk release, never a build of main. No sudo or task-data changes.
set -eu

fail() { printf 'tsk install: %s\n' "$*" >&2; exit 1; }
if [ "$#" -eq 1 ] && [ "$1" = --help ]; then
    printf '%s\n' 'Usage: sh install.sh' 'TSK_VERSION=vX.Y.Z pins a stable release (default: latest published).' 'TSK_INSTALL_DIR=/absolute/path overrides ~/.local/bin.'
    exit 0
fi
[ "$#" -eq 0 ] || fail 'unexpected arguments; use --help'
for command in curl tar uname awk grep sed mktemp chmod mv mkdir; do
    command -v "$command" >/dev/null 2>&1 || fail "missing required command: $command"
done
if command -v sha256sum >/dev/null 2>&1; then
    checksum() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
    checksum() { shasum -a 256 "$1" | awk '{print $1}'; }
else
    fail 'sha256sum or shasum is required'
fi

case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) target=aarch64-apple-darwin ;;
    Darwin/x86_64) target=x86_64-apple-darwin ;;
    Linux/aarch64|Linux/arm64) target=aarch64-unknown-linux-musl ;;
    Linux/x86_64) target=x86_64-unknown-linux-musl ;;
    *) fail 'supported platforms: macOS and Linux, ARM64 or x86-64' ;;
esac

repo=https://github.com/smarzban/herdr-tsk
version=${TSK_VERSION:-}
if [ -z "$version" ]; then
    latest=$(curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL -o /dev/null -w '%{url_effective}' "$repo/releases/latest") || fail 'could not resolve latest published release'
    case "$latest" in
        "$repo/releases/tag/"*) version=${latest##*/} ;;
        *) fail 'unexpected latest-release redirect' ;;
    esac
fi
printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || fail 'TSK_VERSION must be a stable release tag such as v1.2.3'

install_dir=${TSK_INSTALL_DIR:-${HOME:?HOME is required}/.local/bin}
case "$install_dir" in /*) ;; *) fail 'TSK_INSTALL_DIR must be an absolute path' ;; esac
case "$install_dir" in
    *:*|*'
'*) fail 'installation directory cannot contain a colon or newline (PATH separators)' ;;
esac
[ ! -L "$install_dir/tsk" ] || fail 'destination is a symlink; use its package manager or a different TSK_INSTALL_DIR'
[ ! -d "$install_dir/tsk" ] || fail 'destination is a directory'
work=$(mktemp -d "${TMPDIR:-/tmp}/tsk-install.XXXXXX")
staged=
cleanup() { rm -rf "$work"; if [ -n "$staged" ]; then rm -f "$staged"; fi; }
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
archive="tsk-$version-$target.tar.gz"
base="$repo/releases/download/$version"
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL -o "$work/$archive" "$base/$archive" || fail "could not download $archive"
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL -o "$work/SHA256SUMS" "$base/SHA256SUMS" || fail 'could not download checksums'
expected=$(awk -v name="$archive" '$2 == name {print $1}' "$work/SHA256SUMS")
printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || fail 'missing or malformed checksum'
[ "$(printf '%s\n' "$expected" | awk 'END {print NR}')" = 1 ] || fail 'duplicate checksum'
actual=$(checksum "$work/$archive")
[ "$actual" = "$expected" ] || fail 'checksum mismatch; existing installation unchanged'
# Extract only the executable to stdout, never archive paths into the filesystem.
tar -xOzf "$work/$archive" tsk > "$work/tsk" || fail 'release archive does not contain tsk'
[ -s "$work/tsk" ] || fail 'release executable is empty'
mkdir -p "$install_dir"
staged=$(mktemp "$install_dir/.tsk.XXXXXX")
cat "$work/tsk" > "$staged"
chmod 755 "$staged"
mv -f "$staged" "$install_dir/tsk"
staged=
printf 'Installed tsk %s to %s/tsk\n' "$version" "$install_dir"

# Only append to safe regular startup files, never source/evaluate user config.
# Bash reads .bashrc for interactive shells and the first available login file.
# Zsh reads .zshrc for both login and non-login interactive shells.
append_path_to() {
    rc=$1
    case "$rc" in /*) ;; *) return 1 ;; esac
    [ ! -L "$rc" ] || return 1
    if [ -e "$rc" ]; then
        [ -f "$rc" ] && [ -r "$rc" ] && [ -w "$rc" ] || return 1
        if grep -F -x "$path_line" "$rc" >/dev/null; then return 0; fi
    fi
    mkdir -p "${rc%/*}" || return 1
    (umask 077; printf '\n# tsk PATH\n%s\n' "$path_line" >> "$rc") || return 1
    printf 'Configured PATH in %s\n' "$rc"
}
add_path_to() {
    if append_path_to "$1"; then return 0; fi
    printf 'Skipped PATH setup in %s (not a writable regular file or write failed).\n' "$1" >&2
    return 1
}
configure_path() {
    case "${SHELL##*/}" in
        zsh)
            zsh_dir=${ZDOTDIR-${HOME:-}}
            case "$zsh_dir" in /*) ;; *) return 1 ;; esac
            add_path_to "$zsh_dir/.zshrc"
            ;;
        bash)
            case "${HOME:-}" in /*) ;; *) return 1 ;; esac
            login_rc=$HOME/.profile
            for candidate in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
                if [ -e "$candidate" ] || [ -L "$candidate" ]; then login_rc=$candidate; break; fi
            done
            path_complete=1
            add_path_to "$HOME/.bashrc" || path_complete=0
            add_path_to "$login_rc" || path_complete=0
            [ "$path_complete" = 1 ]
            ;;
        *) return 1 ;;
    esac
}
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
        # Single-quote the literal path, including embedded quotes, so neither the
        # printed export nor the startup line can execute path metacharacters.
        quoted_dir="'$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")'"
        path_export="export PATH=$quoted_dir:\"\$PATH\""
        path_line="case \":\${PATH:-}:\" in *:$quoted_dir:*) ;; *) $path_export ;; esac"
        SHELL=${SHELL:-}
        if configure_path; then
            printf 'Reopen your terminal, or run this in the current shell:\n  %s\n' "$path_export"
        else
            printf 'Could not update all shell startup files; any successful edits were kept. Please configure PATH manually for the remaining files. Automatic setup supports Bash and Zsh.\n' >&2
            printf 'For Bash, Zsh or sh, run:\n  %s\n' "$path_export"
            printf 'For other shells, add %s to PATH using your shell configuration.\n' "$install_dir"
        fi
        ;;
esac
