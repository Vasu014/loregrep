#!/usr/bin/env bash
#
# Regenerate the TypeScript SCIP index for the pinned hono checkout.
#
#   ./evals/corpus/regen_typescript.sh
#
# Pipeline:  fetch.sh hono  ->  npm ci/install  ->  scip-typescript index
#            ->  evals/corpus/hono/index.scip
#
# TWO OPERATIONAL TRAPS THIS SCRIPT EXISTS TO AVOID
# -------------------------------------------------
# 1. THE NPM CACHE. On a machine where npm has ever been run under sudo, the
#    user-level cache (~/.npm/_cacache) contains root-owned entries and every
#    subsequent `npm install` dies with EACCES halfway through -- sometimes
#    AFTER writing a partial node_modules, which then indexes to a degraded
#    index rather than failing. We therefore point npm at a corpus-local cache
#    directory with --cache. `sudo chown -R` on the user's global cache is NOT
#    the fix here: this script must not need root and must not mutate state
#    outside evals/corpus/.
#
# 2. THE SILENTLY EMPTY INDEX. scip-typescript exits 0 when tsconfig resolution
#    finds nothing to compile: you get a well-formed .scip with a handful of
#    documents, and a golden built from it looks like a catastrophic loregrep
#    recall failure instead of an indexing failure. (scip-python has the same
#    failure mode; that is the precedent.) So the index is asserted on --
#    document count, .tsx coverage, definition-occurrence count and the
#    first-party package stamp -- and the script exits non-zero rather than
#    leaving a degraded index in place.
#
# hono's dependency tree is dev-only (vitest, tsx, esbuild, ...); the LIBRARY
# has no runtime dependencies. The install still matters: without node_modules,
# `tsc` cannot resolve the ambient types the sources reference and whole files
# degrade to `any`, which changes the rendered `documentation` blocks the
# converter classifies on.
#
# Re-runnable: destroys and rebuilds the index every time. node_modules and the
# .scip output are gitignored; only the golden derived from them is committed.
#
# This script never invokes loregrep -- the ground truth must stay independent
# of the system under test (EVAL_PLAN.md 4b.5).

set -euo pipefail

REPO_ID="hono"
CORPUS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$CORPUS_DIR/../.tools" && pwd)"
WORK_DIR="$CORPUS_DIR/$REPO_ID"
SRC_DIR="$WORK_DIR/src"
NPM_CACHE="$WORK_DIR/.npm-cache"
OUT="$WORK_DIR/index.scip"
LOG="$WORK_DIR/regen.log"

SCIP_TS="$TOOLS_DIR/node_modules/.bin/scip-typescript"
SCIP_CLI="$TOOLS_DIR/scip"

die() { printf '\nregen_typescript.sh: FAILED: %s\n' "$*" >&2; exit 1; }
log() { printf '==> %s\n' "$*" >&2; }

# --- preflight -------------------------------------------------------------
command -v python3 >/dev/null || die "python3 not found on PATH"
command -v node >/dev/null || die "node not found on PATH"
command -v npm >/dev/null || die "npm not found on PATH"
[ -x "$SCIP_TS" ] || die "scip-typescript not installed. Run: npm install --prefix $TOOLS_DIR"
[ -x "$SCIP_CLI" ] || die "scip CLI not found at $SCIP_CLI"

PINNED_SCIP_TS="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["toolchain"]["scip-typescript"]["version"])' "$CORPUS_DIR/corpus.lock")"
ACTUAL_SCIP_TS="$(node -p "require('$TOOLS_DIR/node_modules/@sourcegraph/scip-typescript/package.json').version")"
if [ "$PINNED_SCIP_TS" != "$ACTUAL_SCIP_TS" ]; then
  die "scip-typescript version drift: corpus.lock pins $PINNED_SCIP_TS but $ACTUAL_SCIP_TS is installed.
     An indexer bump is a fixture bump: update corpus.lock AND regenerate + re-triage every TypeScript golden."
fi

# --- 1. fetch --------------------------------------------------------------
log "fetching $REPO_ID at its pinned SHA"
"$CORPUS_DIR/fetch.sh" "$REPO_ID"
[ -f "$SRC_DIR/package.json" ] || die "$SRC_DIR/package.json missing -- fetch did not produce a usable checkout"
[ -f "$SRC_DIR/tsconfig.json" ] || die "$SRC_DIR/tsconfig.json missing -- scip-typescript has nothing to index"

# --- 2. install dependencies ----------------------------------------------
# --cache: see trap (1) above. --no-audit/--no-fund keep the log readable;
# --ignore-scripts keeps a corpus checkout from running arbitrary postinstall
# code on this machine.
log "installing npm dependencies (cache: $NPM_CACHE)"
: >"$LOG"
mkdir -p "$NPM_CACHE"
INSTALL_CMD=(npm install --prefix "$SRC_DIR" --cache "$NPM_CACHE"
             --no-audit --no-fund --ignore-scripts --loglevel=warn)
if [ -f "$SRC_DIR/package-lock.json" ]; then
  INSTALL_CMD=(npm ci --prefix "$SRC_DIR" --cache "$NPM_CACHE"
               --no-audit --no-fund --ignore-scripts --loglevel=warn)
fi
if ! ( cd "$SRC_DIR" && "${INSTALL_CMD[@]}" ) >>"$LOG" 2>&1; then
  tail -40 "$LOG" >&2
  die "npm install failed (full log: $LOG).
     If this is EACCES on a cache path OUTSIDE $NPM_CACHE, npm ignored --cache;
     check for a cache= line in a user or global .npmrc. Do not run this under sudo."
fi
[ -d "$SRC_DIR/node_modules/typescript" ] || \
  die "node_modules/typescript is missing after install -- scip-typescript would index against
     an incomplete type environment and silently emit degraded documentation blocks"
PKG_COUNT="$(find "$SRC_DIR/node_modules" -maxdepth 2 -name package.json | wc -l | tr -d ' ')"
log "installed $PKG_COUNT packages"

# --- 3. index --------------------------------------------------------------
log "running scip-typescript $ACTUAL_SCIP_TS on $REPO_ID"
log "(this is the oracle; loregrep is not involved -- EVAL_PLAN.md 4b.5)"
rm -f "$OUT" "$OUT.json"
START=$(date +%s)
# scip-typescript resolves tsconfig.json relative to the CWD; hono's root
# tsconfig is a project-references file, which is exactly what we want (it pulls
# in src/, the spec project and the per-runtime test projects).
( cd "$SRC_DIR" && "$SCIP_TS" index --output "$OUT" ) >>"$LOG" 2>&1 \
  || { tail -40 "$LOG" >&2; die "scip-typescript index failed (full log: $LOG)"; }
ELAPSED=$(( $(date +%s) - START ))
[ -s "$OUT" ] || die "scip-typescript produced no index at $OUT"
log "indexed in ${ELAPSED}s -> $OUT ($(wc -c <"$OUT" | tr -d ' ') bytes)"

# --- 4. assert the index is not degraded -----------------------------------
log "validating index"
"$SCIP_CLI" print --json "$OUT" >"$OUT.json" || die "scip print --json failed on $OUT"

python3 - "$OUT.json" "$REPO_ID" <<'PY' || die "index failed validation -- NOT writing a golden from it"
import json, sys, collections
idx = json.load(open(sys.argv[1]))
project = sys.argv[2]
docs = idx.get("documents", [])
paths = [d.get("relative_path", "") for d in docs]
tsx = [p for p in paths if p.endswith(".tsx")]
defs = 0
pkgs = collections.Counter()
for d in docs:
    for o in d.get("occurrences", []):
        if not o.get("symbol_roles", 0) & 0x1:
            continue
        defs += 1
        s = o.get("symbol", "")
        if s.startswith("local ") or not s.startswith("scip-typescript npm "):
            continue
        parts = s.split(" ", 4)
        if len(parts) >= 4:
            pkgs[parts[2]] += 1
print(f"    documents:            {len(docs)}  ({len(tsx)} .tsx)")
print(f"    definition occurrences: {defs}")
print(f"    top symbol packages:  {pkgs.most_common(5)}")

problems = []
# A "silently empty index" is the failure this whole block exists for: scip-
# typescript exits 0 when tsconfig resolution matched nothing. Thresholds are
# deliberately far below the observed values (512 docs / 34446 definitions) so
# they fire on a collapse, not on ordinary upstream churn.
if len(docs) < 100:
    problems.append(f"only {len(docs)} documents -- tsconfig resolution almost certainly "
                    f"matched nothing; this is the silently-empty-index failure")
if defs < 5000:
    problems.append(f"only {defs} definition occurrences -- suspiciously sparse")
if not tsx:
    problems.append("no .tsx documents -- hono ships its own JSX runtime, so the JSX "
                    "project reference did not resolve")
if not pkgs:
    problems.append("no first-party symbols at all")
elif pkgs.most_common(1)[0][0] != project:
    problems.append(f"the dominant symbol package is {pkgs.most_common(1)[0][0]!r}, not "
                    f"{project!r} -- the index is mostly node_modules")

if problems:
    for p in problems:
        print(f"    PROBLEM: {p}", file=sys.stderr)
    sys.exit(1)
PY

log "OK: $OUT"
log "JSON view: $OUT.json  (regenerate any time with: $SCIP_CLI print --json $OUT)"
log "Next: rebuild the golden -- see POLICY.md 11.11"
