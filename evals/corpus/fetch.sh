#!/usr/bin/env bash
#
# Fetch the SCIP-parity eval corpus at the SHAs pinned in corpus.lock.
#
#   ./evals/corpus/fetch.sh            # fetch every repo in the lock
#   ./evals/corpus/fetch.sh flask      # fetch just one
#
# Source is fetched, not vendored and not a submodule: each repo lands in
# evals/corpus/<id>/src/, which is gitignored. Only the lock, the scripts and
# the goldens are tracked.
#
# Idempotent: if <id>/src is already checked out at the pinned SHA, this is a
# no-op and no network is touched. A clone is staged in a temp dir and moved
# into place only once it is at the right commit, so an interrupted run never
# leaves a half-fetched tree behind.

set -euo pipefail

CORPUS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCK="$CORPUS_DIR/corpus.lock"

die() { printf 'fetch.sh: %s\n' "$*" >&2; exit 1; }
log() { printf '==> %s\n' "$*" >&2; }

[ -f "$LOCK" ] || die "missing lock file: $LOCK"
command -v git >/dev/null || die "git not found on PATH"
command -v python3 >/dev/null || die "python3 not found on PATH (used to read the JSON lock)"

WANT_ID="${1:-}"

# Emit "id<TAB>url<TAB>commit" for the requested repos.
# (Written for bash 3.2, which macOS still ships: no readarray, no mapfile.)
ROWS_FILE="$(mktemp -t corpus-rows)"
STAGING=""
cleanup() { rm -f "$ROWS_FILE"; [ -n "$STAGING" ] && rm -rf "$STAGING"; return 0; }
trap cleanup EXIT

python3 - "$LOCK" "$WANT_ID" >"$ROWS_FILE" <<'PY' || die "could not read corpus.lock"
import json, sys
lock = json.load(open(sys.argv[1]))
want = sys.argv[2] if len(sys.argv) > 2 else ""
repos = lock["repos"]
if want:
    repos = [r for r in repos if r["id"] == want]
    if not repos:
        ids = ", ".join(r["id"] for r in lock["repos"])
        sys.exit(f"unknown repo id {want!r}; known ids: {ids}")
for r in repos:
    commit = r["commit"]
    if len(commit) != 40 or not all(c in "0123456789abcdef" for c in commit):
        sys.exit(f"{r['id']}: commit must be a full 40-char sha1, got {commit!r}")
    print("\t".join((r["id"], r["url"], commit)))
PY

[ -s "$ROWS_FILE" ] || die "no repos selected"

ROWS=()
while IFS= read -r line; do
  [ -n "$line" ] && ROWS[${#ROWS[@]}]="$line"
done <"$ROWS_FILE"

for row in "${ROWS[@]}"; do
  id="${row%%	*}"
  rest="${row#*	}"
  url="${rest%%	*}"
  commit="${rest#*	}"
  dest="$CORPUS_DIR/$id/src"

  if [ -d "$dest/.git" ]; then
    have="$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)"
    if [ "$have" = "$commit" ]; then
      log "$id: already at $commit (nothing to do)"
      continue
    fi
    log "$id: at ${have:-<unknown>}, want $commit -- refetching"
  elif [ -e "$dest" ]; then
    log "$id: $dest exists but is not a git checkout -- replacing"
  fi

  staging="$CORPUS_DIR/$id/.src.staging.$$"
  STAGING="$staging"
  rm -rf "$staging"
  mkdir -p "$(dirname "$staging")"

  log "$id: cloning $url @ $commit"
  git init --quiet "$staging"
  git -C "$staging" remote add origin "$url"
  # Shallow fetch of exactly the pinned commit. Falls back to a full fetch for
  # servers with uploadpack.allowAnySHA1InWant disabled.
  if ! git -C "$staging" fetch --quiet --depth 1 origin "$commit" 2>/dev/null; then
    log "$id: server refused a by-sha shallow fetch, falling back to full fetch"
    git -C "$staging" fetch --quiet origin
  fi
  git -C "$staging" checkout --quiet --detach "$commit"

  got="$(git -C "$staging" rev-parse HEAD)"
  [ "$got" = "$commit" ] || die "$id: checked out $got but wanted $commit"

  rm -rf "$dest"
  mv "$staging" "$dest"
  STAGING=""
  log "$id: fetched to $dest @ $commit"
done

log "done"
