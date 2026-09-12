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
printf 'Downloading tsk %s for %s...\n' "$version" "$target"
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL -o "$work/$archive" "$base/$archive" || fail "could not download $archive"
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL -o "$work/SHA256SUMS" "$base/SHA256SUMS" || fail 'could not download checksums'
expected=$(awk -v name="$archive" '$2 == name {print $1}' "$work/SHA256SUMS")
printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || fail 'missing or malformed checksum'
[ "$(printf '%s\n' "$expected" | awk 'END {print NR}')" = 1 ] || fail 'duplicate checksum'
actual=$(checksum "$work/$archive")
[ "$actual" = "$expected" ] || fail 'checksum mismatch; existing installation unchanged'
printf 'Verifying checksum... ok\n\nInstalling tsk...\n\n'
# Extract only the executable to stdout, never archive paths into the filesystem.
tar -xOzf "$work/$archive" tsk > "$work/tsk" || fail 'release archive does not contain tsk'
[ -s "$work/tsk" ] || fail 'release executable is empty'
mkdir -p "$install_dir"
staged=$(mktemp "$install_dir/.tsk.XXXXXX")
cat "$work/tsk" > "$staged"
chmod 755 "$staged"
mv -f "$staged" "$install_dir/tsk"
staged=
printf 'Installed:\n\n    tsk %s to %s/tsk\n' "$version" "$install_dir"

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
        printf '\n'
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

# Offer Herdr plugin registration when the host binary is already on PATH.
# Curl|sh often has a non-TTY stdin; prefer /dev/tty so an interactive terminal can still answer.
# CI and headless installs skip the ask so they never hang on a prompt.
# Probe /dev/tty in a child shell: a failed `exec <>/dev/tty` in this shell would exit under set -e.
# herdr_wrap selects the install-completed closing lines (not mid-stream coaching):
#   board         — Herdr absent
#   board_prefix  — setup ran successfully (prefix+t is available)
#   board_setup   — declined, CI/no-TTY skip, or setup failed (nudge tsk setup herdr)
herdr_wrap=board
# skills_wrap: empty | nudge — close with an `Agent skills:  tsk setup` row when agents were
# detected but the batch ask was declined, skipped (CI/no-TTY), or agents --yes failed.
skills_wrap=
tsk_bin=$install_dir/tsk
# An overridden destination can be a shared directory. Do not execute a newly
# published path there: the user can run `tsk setup` after choosing the directory.
post_install_setup=1
if [ -n "${TSK_INSTALL_DIR:-}" ]; then
    post_install_setup=0
fi

maybe_setup_herdr() {
    command -v herdr >/dev/null 2>&1 || return 0
    [ "$post_install_setup" = 1 ] || { herdr_wrap=board_setup; return 0; }
    [ -x "$tsk_bin" ] || return 0
    if [ -n "${CI:-}" ]; then
        herdr_wrap=board_setup
        return 0
    fi
    answer=
    setup_stdin=
    if [ -t 0 ]; then
        printf '\n' >&2
        printf 'Herdr detected. Set up the Herdr plugin now? [y/N] ' >&2
        read -r answer || true
    elif sh -c 'exec <>/dev/tty' 2>/dev/null; then
        printf '\n' >&2
        printf 'Herdr detected. Set up the Herdr plugin now? [y/N] ' >&2
        read -r answer </dev/tty || true
        setup_stdin=/dev/tty
    else
        herdr_wrap=board_setup
        return 0
    fi
    case $answer in
        y|Y|yes|YES)
            printf '\nRunning tsk setup herdr...\n\n'
            setup_status=0
            if [ -n "$setup_stdin" ]; then
                "$tsk_bin" setup herdr <"$setup_stdin" || setup_status=$?
            else
                "$tsk_bin" setup herdr || setup_status=$?
            fi
            if [ "$setup_status" -ne 0 ]; then
                printf '\ntsk setup herdr failed; install succeeded.\n' >&2
                herdr_wrap=board_setup
            else
                herdr_wrap=board_prefix
            fi
            ;;
        *)
            herdr_wrap=board_setup
            ;;
    esac
}

maybe_setup_agent_skills() {
    [ "$post_install_setup" = 1 ] || return 0
    [ -x "$tsk_bin" ] || return 0
    detected_ids=
    detected_ids=$("$tsk_bin" setup --detected-ids 2>/dev/null) || detected_ids=
    detected_ids=$(printf '%s' "$detected_ids" | tr -s '[:space:]' ' ' | sed 's/^ *//;s/ *$//')
    [ -n "$detected_ids" ] || return 0

    if [ -n "${CI:-}" ]; then
        skills_wrap=nudge
        return 0
    fi

    answer=
    setup_stdin=
    id_list=$(printf '%s' "$detected_ids" | sed 's/ /, /g')
    if [ -t 0 ]; then
        printf '\n' >&2
        printf 'Agents detected: %s. Install the tsk skill for them? [y/N] ' "$id_list" >&2
        read -r answer || true
    elif sh -c 'exec <>/dev/tty' 2>/dev/null; then
        printf '\n' >&2
        printf 'Agents detected: %s. Install the tsk skill for them? [y/N] ' "$id_list" >&2
        read -r answer </dev/tty || true
        setup_stdin=/dev/tty
    else
        skills_wrap=nudge
        return 0
    fi
    case $answer in
        y|Y|yes|YES)
            printf '\nRunning tsk setup agents...\n\n'
            setup_status=0
            if [ -n "$setup_stdin" ]; then
                "$tsk_bin" setup agents --yes <"$setup_stdin" || setup_status=$?
            else
                "$tsk_bin" setup agents --yes || setup_status=$?
            fi
            if [ "$setup_status" -ne 0 ]; then
                printf '\ntsk setup agents --yes failed; install succeeded.\n' >&2
                skills_wrap=nudge
            fi
            ;;
        *)
            skills_wrap=nudge
            ;;
    esac
}

maybe_setup_herdr
maybe_setup_agent_skills

printf '\nDone. Run tsk in a project directory to open the board'
if [ "$herdr_wrap" = board_prefix ]; then
    printf ', or press prefix+t in Herdr.\n'
else
    printf '.\n'
fi
herdr_row=
agent_row=
if [ "$post_install_setup" = 1 ]; then
    if [ "$herdr_wrap" = board_setup ]; then
        herdr_row='    Herdr plugin:  tsk setup herdr'
    fi
    if [ "$skills_wrap" = nudge ]; then
        agent_row='    Agent skills:  tsk setup'
    fi
elif [ -n "${TSK_UPDATE:-}" ]; then
    # Invoked by `tsk update`: the binary was replaced in place. A registered Herdr plugin
    # embeds the old version in its manifest, and installed agent skills may be stale.
    printf '\nUpdated in place. Refresh what you use:\n'
    if [ "$herdr_wrap" = board_setup ]; then
        herdr_row='    Herdr plugin:  tsk setup herdr'
    fi
    agent_row='    Agent skills:  tsk setup'
else
    # Custom TSK_INSTALL_DIR: the installer never ran setup, so say why and name the
    # full binary path, which may not be on PATH. The agent row is unconditional here:
    # detection would have required executing the published binary.
    printf '\nCustom install directory: setup was not run. When you are ready:\n'
    if [ "$herdr_wrap" = board_setup ]; then
        herdr_row="    Herdr plugin:  $tsk_bin setup herdr"
    fi
    agent_row="    Agent skills:  $tsk_bin setup"
fi
if [ -n "$herdr_row" ] || [ -n "$agent_row" ]; then
    printf '\n'
    if [ -n "$herdr_row" ]; then
        printf '%s\n' "$herdr_row"
    fi
    if [ -n "$agent_row" ]; then
        printf '%s\n' "$agent_row"
    fi
fi
