#!/usr/bin/env bash
# rsuite bootstrap: ensure prerequisites, build + install the rsuite picker,
# then launch it. rsuite itself clones any missing component repos (clone URLs
# live in suite.toml), so a fresh machine needs only this one script.
set -euo pipefail

REPO_DIR="${RSUITE_DIR:-$HOME/projects/dashboard-suite}"
REPO_URL="${RSUITE_REPO:-https://github.com/JaneAdora/dashboard-suite}"

# git is required to clone the suite and for cargo to fetch the suite-term git dep.
if ! command -v git >/dev/null 2>&1; then
  echo "error: git is required but not found." >&2
  echo "       install it first (e.g. 'sudo apt install git' or 'brew install git') and re-run." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "rust toolchain not found — installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# Ensure ~/.cargo/bin is on PATH for THIS shell (wt/recall install there via a
# prefix override); harmless if cargo was already system-installed.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

if [ ! -d "$REPO_DIR" ]; then
  echo "cloning rsuite into $REPO_DIR..."
  git clone "$REPO_URL" "$REPO_DIR"
fi

cd "$REPO_DIR"
cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/rsuite "$HOME/.local/bin/rsuite"
echo "rsuite installed to ~/.local/bin/rsuite"

# PATH guidance: the suite installs to ~/.local/bin (and wt/recall to ~/.cargo/bin).
missing=""
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) missing="$HOME/.local/bin" ;; esac
case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) missing="${missing:+$missing and }$HOME/.cargo/bin" ;; esac
if [ -n "$missing" ]; then
  echo "note: add $missing to your PATH, then restart your shell (or 'source ~/.cargo/env')."
fi

# Launch the picker unless asked not to.
if [ "${1:-}" != "--no-run" ]; then
  exec "$HOME/.local/bin/rsuite" "$@"
fi
