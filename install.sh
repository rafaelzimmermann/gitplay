#!/usr/bin/env bash
#
# gitplay installer — build and install gitplay into a directory on your PATH.
#
# Usage:
#   ./install.sh                       # auto-detect install directory
#   ./install.sh --dir <PATH>          # install into a specific directory
#   INSTALL_DIR=<PATH> ./install.sh    # same, via environment variable
#   ./install.sh --uninstall           # remove gitplay from the install directory
#
# See ./install.sh --help for full details.

set -euo pipefail

GITPLAY_NAME="gitplay"
RUSTUP_HINT="curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
info()  { printf '%s\n' "==> $*"; }
warn()  { printf '%s\n' "warning: $*" >&2; }
error() { printf '%s\n' "error: $*" >&2; }
die()   { error "$*"; exit 1; }

usage() {
  cat <<'EOF'
gitplay installer — build and install gitplay from a repository checkout.

USAGE:
    ./install.sh [OPTIONS]

OPTIONS:
    -d, --dir <PATH>      Install directory for the gitplay binary.
                          Takes precedence over the INSTALL_DIR environment
                          variable and PATH autodetection.
    -u, --uninstall       Remove the installed gitplay binary and clean up
                          known symlinks, then exit.
    -h, --help            Show this help message and exit.

ENVIRONMENT:
    INSTALL_DIR=<PATH>    Default install directory (overridden by --dir).
                          When neither is set, the script auto-detects:
                            1. The directory of an existing gitplay on PATH
                            2. $HOME/.cargo/bin  (if on PATH)
                            3. $HOME/.local/bin  (if on PATH)
                            4. /usr/local/bin    (if root or writable)
                            5. $HOME/.local/bin  (fallback, with PATH advice)

EXAMPLES:
    ./install.sh
    ./install.sh --dir /usr/local/bin
    INSTALL_DIR=~/.local/bin ./install.sh
    ./install.sh --uninstall
EOF
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
ARG_DIR=""
ARG_UNINSTALL="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -d|--dir)
      [[ $# -ge 2 ]] || die "--dir requires a <PATH> argument. Try --help."
      ARG_DIR="$2"
      shift 2
      ;;
    --dir=*)
      ARG_DIR="${1#--dir=}"
      shift
      ;;
    -u|--uninstall)
      ARG_UNINSTALL="true"
      shift
      ;;
    *)
      die "unknown option: '$1'. Try --help for usage."
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Install directory resolution
#   Priority: --dir flag > INSTALL_DIR env > PATH autodetection
# ---------------------------------------------------------------------------
resolve_install_dir() {
  # CLI flag takes precedence over the environment variable.
  if [[ -n "$ARG_DIR" ]]; then
    printf '%s' "$ARG_DIR"
    return
  fi
  if [[ -n "${INSTALL_DIR:-}" ]]; then
    printf '%s' "$INSTALL_DIR"
    return
  fi

  # An existing gitplay on PATH wins: updating in place avoids two installs
  # where the old one still shadows the new one (e.g. ~/.local/bin/gitplay
  # resolved before ~/.cargo/bin/gitplay).
  if command -v "$GITPLAY_NAME" >/dev/null 2>&1; then
    local resolved
    resolved="$(command -v "$GITPLAY_NAME")"
    printf '%s' "$(dirname "$resolved")"
    return
  fi

  case ":$PATH:" in
    *":$HOME/.cargo/bin:"*)
      printf '%s' "$HOME/.cargo/bin"
      return
      ;;
  esac

  case ":$PATH:" in
    *":$HOME/.local/bin:"*)
      printf '%s' "$HOME/.local/bin"
      return
      ;;
  esac

  if [[ "$(id -u)" -eq 0 ]] || [[ -w /usr/local/bin ]]; then
    printf '%s' "/usr/local/bin"
    return
  fi

  # Fallback: warn the user and suggest PATH setup.
  warn "\$HOME/.local/bin is not on your PATH."
  warn "Add it to your shell profile, e.g.:"
  warn "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.profile && source ~/.profile"
  printf '%s' "$HOME/.local/bin"
}

INSTALL_DIR="$(resolve_install_dir)"

# ---------------------------------------------------------------------------
# Uninstall
# ---------------------------------------------------------------------------
uninstall() {
  info "Uninstalling ${GITPLAY_NAME} from ${INSTALL_DIR}"
  local removed="false"

  if [[ -f "$INSTALL_DIR/$GITPLAY_NAME" ]] || [[ -L "$INSTALL_DIR/$GITPLAY_NAME" ]]; then
    rm -f -- "$INSTALL_DIR/$GITPLAY_NAME"
    info "Removed $INSTALL_DIR/$GITPLAY_NAME"
    removed="true"
  else
    warn "No $GITPLAY_NAME binary found in $INSTALL_DIR — nothing to remove."
  fi

  # Clean up known symlinks created by other install methods
  # (e.g. linking gitplay into /usr/local/bin or ~/bin).
  local link_dir link_path
  for link_dir in "/usr/local/bin" "$HOME/bin" "$HOME/.local/bin"; do
    link_path="$link_dir/$GITPLAY_NAME"
    if [[ -L "$link_path" ]]; then
      rm -f -- "$link_path"
      info "Removed dangling symlink $link_path"
      removed="true"
    fi
  done

  if [[ "$removed" == "true" ]]; then
    info "gitplay uninstalled successfully."
  else
    info "Nothing left to uninstall."
  fi
}

if [[ "$ARG_UNINSTALL" == "true" ]]; then
  uninstall
  exit 0
fi

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust via rustup first:
    $RUSTUP_HINT"
command -v rustc >/dev/null 2>&1 || die "rustc not found. Install Rust via rustup first:
    $RUSTUP_HINT"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

info "Building gitplay (release, locked) in $REPO_DIR"
cargo build --release --locked --manifest-path "$REPO_DIR/Cargo.toml"

BUILT_BINARY="$REPO_DIR/target/release/$GITPLAY_NAME"
[[ -f "$BUILT_BINARY" ]] || die "build did not produce $BUILT_BINARY"

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------
mkdir -p -- "$INSTALL_DIR"

info "Installing gitplay to $INSTALL_DIR"
if command -v install >/dev/null 2>&1; then
  install -m 755 "$BUILT_BINARY" "$INSTALL_DIR/$GITPLAY_NAME"
else
  cp -- "$BUILT_BINARY" "$INSTALL_DIR/$GITPLAY_NAME"
  chmod 755 -- "$INSTALL_DIR/$GITPLAY_NAME"
fi

# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------
INSTALLED_VERSION="$("$INSTALL_DIR/$GITPLAY_NAME" --version)"
info "Installed: $INSTALLED_VERSION"
info "Location: $INSTALL_DIR/$GITPLAY_NAME"

cat <<EOF

gitplay installed successfully! 🎬

Quick start:
    gitplay                    watch the current repository
    gitplay --pace 30          roll diffs more slowly
    gitplay --cumulative       diff against git HEAD
    gitplay --help             full CLI reference

Tip: if 'gitplay' is not found, ensure $INSTALL_DIR is on your PATH.
EOF
