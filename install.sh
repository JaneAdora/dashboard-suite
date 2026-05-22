#!/usr/bin/env bash
# rsuite bootstrap: ensure rust, build + install rsuite, then launch the picker.
# Component repos (roam/glance/wt/recall/launchers) are expected under ~/projects
# (override with $RSUITE_PROJECTS); rsuite skips any it can't find.
set -euo pipefail

REPO_DIR="${RSUITE_DIR:-$HOME/projects/dashboard-suite}"
REPO_URL="${RSUITE_REPO:-https://github.com/JaneAdora/dashboard-suite}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "rust toolchain not found — installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

if [ ! -d "$REPO_DIR" ]; then
  echo "cloning rsuite into $REPO_DIR..."
  git clone "$REPO_URL" "$REPO_DIR"
fi

cd "$REPO_DIR"
cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/rsuite "$HOME/.local/bin/rsuite"
echo "rsuite installed to ~/.local/bin/rsuite"

case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "note: add ~/.local/bin to your PATH" ;;
esac

# Launch the picker unless asked not to.
if [ "${1:-}" != "--no-run" ]; then
  exec "$HOME/.local/bin/rsuite" "$@"
fi
