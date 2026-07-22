#!/usr/bin/env bash
#
# Regenerate the Python SCIP index for the pinned flask checkout.
#
#   ./evals/corpus/regen_python.sh
#
# Pipeline:  fetch.sh flask  ->  venv  ->  pip install -e src  ->  scip-python index
#            ->  evals/corpus/flask/index.scip
#
# THE OPERATIONAL TRAP THIS SCRIPT EXISTS TO AVOID
# ------------------------------------------------
# scip-python derives the `package name` and `package version` components of every
# SCIP symbol string by SHELLING OUT TO `pip list` / `pip show` in whatever
# environment is on PATH. Two ways to get this wrong, both silent:
#
#   1. Not installing the project under test into that environment. scip-python
#      still produces an index, but one where cross-module symbols degrade and
#      cross-file resolution drops. The index looks fine; goldens built from it
#      are quietly wrong.
#   2. Forgetting to put the venv first on PATH. scip-python then reads the
#      SYSTEM pip and indexes against the wrong package set, with no warning.
#
# So: we build a venv, `pip install -e` the pinned checkout into it, and export
# VIRTUAL_ENV + PATH so scip-python's pip subprocess resolves to the venv. Then
# we assert on the resulting index (document count, occurrence count, absence of
# the degraded package marker) and exit non-zero rather than shipping a degraded
# index.
#
# Re-runnable: destroys and rebuilds the venv and the index every time. The venv
# and the .scip output are gitignored; only the golden derived from them is
# committed.
#
# This script never invokes loregrep -- the ground truth must stay independent of
# the system under test (EVAL_PLAN.md 4b.5).

set -euo pipefail

REPO_ID="flask"
CORPUS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$CORPUS_DIR/../.tools" && pwd)"
WORK_DIR="$CORPUS_DIR/$REPO_ID"
SRC_DIR="$WORK_DIR/src"
VENV_DIR="$WORK_DIR/.venv"
OUT="$WORK_DIR/index.scip"
LOG="$WORK_DIR/regen.log"

SCIP_PYTHON="$TOOLS_DIR/node_modules/.bin/scip-python"
SCIP_CLI="$TOOLS_DIR/scip"

die() { printf '\nregen_python.sh: FAILED: %s\n' "$*" >&2; exit 1; }
log() { printf '==> %s\n' "$*" >&2; }

# --- preflight -------------------------------------------------------------
command -v python3 >/dev/null || die "python3 not found on PATH"
[ -x "$SCIP_PYTHON" ] || die "scip-python not installed. Run: npm install --prefix $TOOLS_DIR"
[ -x "$SCIP_CLI" ] || die "scip CLI not found at $SCIP_CLI"

PINNED_SCIP_PYTHON="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["toolchain"]["scip-python"]["version"])' "$CORPUS_DIR/corpus.lock")"
ACTUAL_SCIP_PYTHON="$(node -p "require('$TOOLS_DIR/node_modules/@sourcegraph/scip-python/package.json').version")"
if [ "$PINNED_SCIP_PYTHON" != "$ACTUAL_SCIP_PYTHON" ]; then
  die "scip-python version drift: corpus.lock pins $PINNED_SCIP_PYTHON but $ACTUAL_SCIP_PYTHON is installed.
     An indexer bump is a fixture bump: update corpus.lock AND regenerate + re-triage every Python golden."
fi

# --- 1. fetch --------------------------------------------------------------
log "fetching $REPO_ID at its pinned SHA"
"$CORPUS_DIR/fetch.sh" "$REPO_ID"
[ -f "$SRC_DIR/pyproject.toml" ] || die "$SRC_DIR/pyproject.toml missing -- fetch did not produce a usable checkout"

# --- 2. venv ---------------------------------------------------------------
log "rebuilding venv at $VENV_DIR"
: >"$LOG"
rm -rf "$VENV_DIR"
python3 -m venv "$VENV_DIR" || die "could not create venv"
VPY="$VENV_DIR/bin/python"
"$VPY" -m pip install --quiet --upgrade pip >>"$LOG" 2>&1 || die "pip self-upgrade failed (see $LOG)"

# --- 3. install the project under test into that venv ----------------------
# This is the step whose omission silently degrades the index. Editable, so the
# indexed source tree and the installed distribution are the same files.
log "pip install -e $SRC_DIR  (required for correct scip-python symbol packages)"
if ! "$VPY" -m pip install -e "$SRC_DIR" >>"$LOG" 2>&1; then
  tail -30 "$LOG" >&2
  die "editable install of $REPO_ID failed. Without it scip-python emits a degraded index; refusing to continue."
fi

# Prove the package is importable and has real metadata.
"$VPY" - "$REPO_ID" <<'PY' || die "installed package is not importable / has no version metadata"
import importlib, importlib.metadata as md, sys
name = sys.argv[1]
mod = importlib.import_module(name)
ver = md.version(name)
if not ver or ver.startswith("0.0.0"):
    sys.exit(f"{name} reports a placeholder version {ver!r} -- scip-python would emit degraded symbols")
print(f"    {name} {ver} importable from {mod.__file__}")
PY

# --- 4. index --------------------------------------------------------------
PROJECT_VERSION="$("$VPY" -c 'import importlib.metadata as m;print(m.version("'"$REPO_ID"'"))')"
log "running scip-python $ACTUAL_SCIP_PYTHON on $REPO_ID $PROJECT_VERSION"
log "(this is the oracle; loregrep is not involved -- EVAL_PLAN.md 4b.5)"
rm -f "$OUT" "$OUT.json"
START=$(date +%s)
(
  # scip-python resolves `pip` off PATH; the venv MUST win.
  export VIRTUAL_ENV="$VENV_DIR"
  export PATH="$VENV_DIR/bin:$PATH"
  unset PYTHONHOME PYTHONPATH
  cd "$SRC_DIR" && \
  "$SCIP_PYTHON" index . \
    --project-name "$REPO_ID" \
    --project-version "$PROJECT_VERSION" \
    --cwd "$SRC_DIR" \
    --output "$OUT" >>"$LOG" 2>&1
) || { tail -40 "$LOG" >&2; die "scip-python index failed (full log: $LOG)"; }
ELAPSED=$(( $(date +%s) - START ))
[ -s "$OUT" ] || die "scip-python produced no index at $OUT"
log "indexed in ${ELAPSED}s -> $OUT ($(wc -c <"$OUT" | tr -d ' ') bytes)"

# --- 5. assert the index is not degraded -----------------------------------
log "validating index"
"$SCIP_CLI" print --json "$OUT" >"$OUT.json" || die "scip print --json failed on $OUT"

"$VPY" - "$OUT.json" "$REPO_ID" <<'PY' || die "index failed validation -- NOT writing a golden from it"
import json, sys, collections
idx = json.load(open(sys.argv[1]))
project = sys.argv[2]
docs = idx.get("documents", [])
occs = sum(len(d.get("occurrences", [])) for d in docs)
syms = sum(len(d.get("symbols", [])) for d in docs)
print(f"    documents:   {len(docs)}")
print(f"    occurrences: {occs}")
print(f"    symbols:     {syms}")

problems = []
if len(docs) < 20:
    problems.append(f"only {len(docs)} documents -- expected the whole package, indexing likely bailed early")
if occs < 5000:
    problems.append(f"only {occs} occurrences -- suspiciously sparse")

# Degradation marker: scip-python stamps the package name/version into every
# non-local symbol. If the project was not pip-installed, its own symbols come
# out under a placeholder package instead of `<project> <version>`.
own = [
    o["symbol"]
    for d in docs
    for o in d.get("occurrences", [])
    if o.get("symbol", "").startswith("scip-python python ")
]
pkgs = collections.Counter(" ".join(s.split(" ")[2:4]) for s in own)
print(f"    top symbol packages: {pkgs.most_common(5)}")
if not any(k.split(" ")[0] == project for k in pkgs):
    problems.append(
        f"no symbols carry package name {project!r} -- the editable install did not take effect "
        f"and cross-module resolution is degraded"
    )
for k in pkgs:
    if k.endswith(" 0.0.0") and k.startswith(project):
        problems.append(f"{project} symbols carry placeholder version 0.0.0 -- degraded index")

if problems:
    for p in problems:
        print(f"    PROBLEM: {p}", file=sys.stderr)
    sys.exit(1)
PY

log "OK: $OUT"
log "JSON view: $OUT.json  (regenerate any time with: $SCIP_CLI print --json $OUT)"
