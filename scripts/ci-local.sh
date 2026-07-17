#!/usr/bin/env bash
#
# ci-local.sh — run the CI checks locally, in parallel, for THIS machine's arch.
#
# What this covers: everything in .github/workflows/rust-ci.yml (fmt, test, release
# build, clippy, and the Python-bindings pytest job). It does NOT reproduce the
# cross-OS / cross-arch wheel matrix (Windows, macOS-x86, musllinux, aarch64) — that
# needs those OSes and stays on GitHub Actions.
#
# Design: three parallel "lanes", each with its own CARGO_TARGET_DIR so cargo's
# build-directory lock doesn't serialize them:
#   Lane 1 (target/ci-dev)     : fmt -> test -> check(python) -> clippy   (share dev artifacts)
#   Lane 2 (target/ci-release) : cargo build --release
#   Lane 3 (target/ci-python)  : maturin develop + pytest  (real binding runtime test)
# Cost: ~3 target dirs of disk. `cargo clean`-free reset: rm -rf target/ci-*
#
# Usage: scripts/ci-local.sh        (from anywhere)
# Exit:  non-zero if any BLOCKING check fails (clippy is informational, like CI).

set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

LOG="target/ci-local"
mkdir -p "$LOG"
: > "$LOG/results.tsv"

# check <status-kind> <name> <logfile> <cmd...>
#   status-kind: "block" (failure fails the run) or "info" (reported, never fails)
check() {
  local kind="$1" name="$2" lf="$3"; shift 3
  local start=$SECONDS
  if "$@" >"$lf" 2>&1; then
    printf 'PASS\t%s\t%s\t%ss\t%s\n' "$kind" "$name" "$((SECONDS-start))" "$lf" >> "$LOG/results.tsv"
  else
    printf 'FAIL\t%s\t%s\t%ss\t%s\n' "$kind" "$name" "$((SECONDS-start))" "$lf" >> "$LOG/results.tsv"
  fi
}

echo "==> Running local CI in parallel (logs in $LOG/)"
echo "    (cross-OS/arch wheel builds are GitHub-only; not run here)"

# ---- Lane 1: dev-profile checks, sequential within the lane (artifact reuse) ----
(
  export CARGO_TARGET_DIR=target/ci-dev
  check block fmt          "$LOG/fmt.log"      cargo fmt --all --check
  check block test         "$LOG/test.log"     cargo test
  check block check-python "$LOG/checkpy.log"  cargo check --lib --features python
  check info  clippy       "$LOG/clippy.log"   cargo clippy --all-targets
) & L1=$!

# ---- Lane 2: release build ----
(
  export CARGO_TARGET_DIR=target/ci-release
  check block release "$LOG/release.log" cargo build --release
) & L2=$!

# ---- Lane 3: Python bindings runtime (needs maturin) ----
(
  if command -v maturin >/dev/null 2>&1; then
    export CARGO_TARGET_DIR=target/ci-python
    check block python "$LOG/python.log" bash -c '
      set -e
      python3 -m venv "'"$LOG"'/venv"
      . "'"$LOG"'/venv/bin/activate"
      pip -q install --upgrade pip
      pip -q install maturin pytest pytest-asyncio
      maturin develop --features python
      pytest python/tests -q'
  else
    printf 'SKIP\tblock\tpython\t0s\t(maturin not installed: pip install maturin)\n' >> "$LOG/results.tsv"
  fi
) & L3=$!

wait "$L1" "$L2" "$L3"

# ---- Summary ----
echo ""
echo "==> Results"
fail=0
while IFS=$'\t' read -r status kind name dur logf; do
  case "$status" in
    PASS) icon="✅" ;;
    FAIL) icon="❌"; [ "$kind" = block ] && fail=1 ;;
    SKIP) icon="⚠️ " ;;
  esac
  printf '  %s  %-14s %-6s %s\n' "$icon" "$name" "$dur" "$( [ "$status" = PASS ] && echo "" || echo "-> $logf" )"
done < <(sort "$LOG/results.tsv")

echo ""
if [ "$fail" -eq 0 ]; then
  echo "==> All blocking checks passed. ✅"
else
  echo "==> Blocking failures above. Inspect the referenced logs. ❌"
fi
exit "$fail"
