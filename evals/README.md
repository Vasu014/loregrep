# evals

Evaluation harnesses for loregrep. The design and rationale live in [`EVAL_PLAN.md`](EVAL_PLAN.md).

- `fixtures/` — hand-written micro-repos whose answers are known by construction (Level 1).
- `retrieval/` — the Level 1 runner and the `known_failures.json` xfail ledger.
- `agent/` — Level 2 agent A/B pilot.
- `corpus/` — real repos indexed by third-party SCIP indexers, for population-scale parity (EVAL_PLAN.md §4b).
- `.tools/` — pinned third-party binaries (`scip` CLI, `scip-python`). Contents are gitignored; `package.json` / `package-lock.json` are tracked.

## Corpus

`evals/corpus/` holds the population-scale ground truth: real repositories indexed by
compiler-grade SCIP indexers (`rust-analyzer scip`, `scip-python`), converted to goldens that
loregrep is then scored against. Two repos, deliberately: `ripgrep` (Rust) and `flask` (Python).

### Source is fetched, not vendored

The corpus is **not** committed and **not** a git submodule. `corpus.lock` pins each repo at a
full 40-char commit SHA and `fetch.sh` clones it into `evals/corpus/<id>/src/`, which is
gitignored. The goldens are the artifact that matters; generation is manual and CI does not
gate on it, so there is no reason to tax every clone of this repo with a corpus checkout.

Tracked: `corpus.lock`, the scripts, `golden.schema.json`, and the goldens.
Ignored: `*/src/`, `*.scip`, `*.scip.json`, venvs, logs.

### fetch → regen → score

```bash
# 1. fetch — clones each pinned repo at its pinned SHA (idempotent; no-op if already correct)
./evals/corpus/fetch.sh              # all repos
./evals/corpus/fetch.sh flask        # just one

# 2. regen — run the third-party indexer and emit <id>/index.scip
./evals/corpus/regen_python.sh       # flask, via scip-python
#   (the Rust path, rust-analyzer scip, lands with the Rust converter)

# 3. inspect / convert — the scip CLI turns the protobuf into JSON
./evals/.tools/scip print --json evals/corpus/flask/index.scip > /tmp/flask.json

# 4. score — Level 1 runner in corpus mode (see EVAL_PLAN.md §4b.4)
python3 evals/retrieval/run.py --corpus flask
```

Steps 1–2 are the only ones that touch the network. After the first fetch everything is
offline, and scoring never runs an indexer.

### Toolchain pinning

`corpus.lock` has a `toolchain` section recording the exact versions of `scip`,
`rust-analyzer`, `scip-python`, node and python. **An indexer version bump is treated like a
fixture bump**: bump the lock, regenerate every affected golden, and re-triage the diff.
`regen_python.sh` refuses to run if the installed `scip-python` disagrees with the lock.

Installing the Python indexer (repo-local, so the machine stays clean):

```bash
npm install --prefix evals/.tools     # honours the tracked package-lock.json
```

### The scip-python trap

`scip-python` determines the package name and version of every symbol by shelling out to
`pip list` / `pip show` in whatever environment is on `PATH`. If the project under test is not
pip-installed into that environment, **scip-python exits 0, prints "Successfully wrote SCIP
index", and writes an index containing zero documents** — or, in milder cases, one whose
cross-module symbols are degraded. Nothing in the output says so.

`regen_python.sh` therefore always builds a venv, `pip install -e`s the pinned checkout into
it, puts the venv first on `PATH`, and then *asserts* on the resulting index (document count,
occurrence count, and that symbols actually carry the project's package name and real version)
before declaring success. Do not shortcut it.

### Independence invariant

loregrep appears in this pipeline exactly once: as the system under test inside the scorer.
Corpus fetching, SCIP generation, converters, symbol sampling and golden triage are built and
operated **loregrep-free** — correlated ground truth silently inflates recall. See
EVAL_PLAN.md §4b.5.
