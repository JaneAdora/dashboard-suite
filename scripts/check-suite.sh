#!/usr/bin/env bash
# Run the standard health checks across the dashboard widget suite.
#
# Defaults assume the suite repos live under ~/projects. Override with:
#   RSUITE_PROJECTS=/path/to/projects scripts/check-suite.sh
#
# Optional overrides:
#   SUITE_REPOS="suite-term glance wt" scripts/check-suite.sh
#   SUITE_CHECKS="fmt test" scripts/check-suite.sh

set -u
set -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
META_REPO="$(cd -- "$SCRIPT_DIR/.." >/dev/null 2>&1 && pwd)"
PROJECTS_DIR="${RSUITE_PROJECTS:-$(dirname -- "$META_REPO")}"
LOG_DIR="${SUITE_CHECK_LOG_DIR:-$META_REPO/target/suite-checks}"

DEFAULT_REPOS="suite-term glance wt recall roam atlas mandalas launchers"
DEFAULT_CHECKS="fmt test clippy"

read -r -a REPOS <<< "${SUITE_REPOS:-$DEFAULT_REPOS}"
read -r -a CHECKS <<< "${SUITE_CHECKS:-$DEFAULT_CHECKS}"

mkdir -p "$LOG_DIR"

usage() {
  sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
}

has_check() {
  local wanted="$1"
  local check
  for check in "${CHECKS[@]}"; do
    [[ "$check" == "$wanted" ]] && return 0
  done
  return 1
}

run_check() {
  local repo="$1"
  local check="$2"
  local dir="$PROJECTS_DIR/$repo"
  local log="$LOG_DIR/$repo-$check.log"
  local status=0

  printf '  %-8s ' "$check"

  if [[ ! -d "$dir" ]]; then
    printf 'MISSING (%s)\n' "$dir"
    printf 'missing repo: %s\n' "$dir" > "$log"
    return 1
  fi

  case "$check" in
    fmt)
      (cd "$dir" && cargo fmt --all -- --check) > "$log" 2>&1
      status=$?
      ;;
    test)
      if [[ "$repo" == "launchers" ]]; then
        (cd "$dir" && cargo test --workspace) > "$log" 2>&1
      else
        (cd "$dir" && cargo test) > "$log" 2>&1
      fi
      status=$?
      ;;
    clippy)
      if [[ "$repo" == "launchers" ]]; then
        (cd "$dir" && cargo clippy --workspace --all-targets -- -D warnings) > "$log" 2>&1
      else
        (cd "$dir" && cargo clippy --all-targets -- -D warnings) > "$log" 2>&1
      fi
      status=$?
      ;;
    *)
      printf 'UNKNOWN\n'
      printf 'unknown check: %s\n' "$check" > "$log"
      return 1
      ;;
  esac

  if [[ "$status" -eq 0 ]]; then
    printf 'PASS\n'
  else
    printf 'FAIL  (%s)\n' "$log"
  fi
  return "$status"
}

main() {
  if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
  fi

  local failed=0
  local repo check

  printf 'Dashboard suite checks\n'
  printf 'projects: %s\n' "$PROJECTS_DIR"
  printf 'logs:     %s\n' "$LOG_DIR"
  printf 'repos:    %s\n' "${REPOS[*]}"
  printf 'checks:   %s\n\n' "${CHECKS[*]}"

  for check in "${CHECKS[@]}"; do
    case "$check" in
      fmt|test|clippy) ;;
      *)
        printf 'unknown check in SUITE_CHECKS: %s\n' "$check" >&2
        exit 2
        ;;
    esac
  done

  for repo in "${REPOS[@]}"; do
    printf '==> %s\n' "$repo"
    for check in fmt test clippy; do
      if has_check "$check"; then
        run_check "$repo" "$check" || failed=1
      fi
    done
    printf '\n'
  done

  if [[ "$failed" -eq 0 ]]; then
    printf 'suite checks passed\n'
  else
    printf 'suite checks failed; inspect logs under %s\n' "$LOG_DIR"
  fi

  return "$failed"
}

main "$@"
