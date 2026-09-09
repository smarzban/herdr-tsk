#!/bin/sh
# Install a published tsk release, never a build of main. No sudo or shell edits.
set -eu

fail() { printf 'tsk install: %s\n' "$*" >&2; exit 1; }
if [ "$#" -eq 1 ] && [ "$1" = --help ]; then
    printf '%s\n' 'Usage: sh install.sh' 'TSK_VERSION=vX.Y.Z pins a stable release (default: latest published).' 'TSK_INSTALL_DIR=/absolute/path overrides ~/.local/bin.'
    exit 0
fi
[ "$#" -eq 0 ] || fail 'unexpected arguments; use --help'
for command in curl tar uname awk grep mktemp chmod mv mkdir; do
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
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *) printf 'Add this directory to PATH in your shell configuration: %s\n' "$install_dir" ;;
esac
