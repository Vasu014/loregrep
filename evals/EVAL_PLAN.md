# EVAL_PLAN.md — loregrep evaluation harness

> Authored by a planning pass over the repo; grounded in the actual code. Answers one question:
> **does giving a coding agent loregrep's structural tools reduce tokens / turns / wall-clock at
> equal-or-better task success, versus a baseline agent with only grep/read?** Agent-level metrics
> are statistical (agents are stochastic); a separate layer is fully deterministic.

## 0. Verified interface facts this plan is built on

Confirmed by reading source; Layer 1 should break loudly on drift.

- Entry point: `loregrep exec-tool <tool> --params '<json>' --path <dir>` (`src/internal/cli_types.rs`; `--params` defaults `{}`, `--path` `.`).
- Stdout is exactly one pretty-printed JSON `ToolResult`: `{"success": bool, "data": {...}, "error": null|string}` (`src/internal/ai_tools.rs`, printed in `src/internal/cli.rs`). Diagnostics → stderr. Exit 1 when `success:false`.
- Index cache: `<path>/.loregrep/index.cache`, reused opportunistically on later `exec-tool` calls. Fixtures must delete this dir for cold-latency runs and gitignore it.
- Payload shapes (from `ai_tools.rs`):
  - `search_functions` → `data.results`: `FunctionSignature[]` (`name`, `file_path`, `parameters`, `return_type`, `is_public`, `is_async`, `is_const`, `is_static`, `is_extern`, `start_line`, `end_line`, `generics`), plus `data.count`, `data.pattern`.
  - `search_structs` → `data.results`: `StructSignature[]` (`name`, `file_path`, `fields`, `is_public`, `is_tuple_struct`, `start_line`, `end_line`, `generics`).
  - `find_callers` → `data.callers`: `CallSite[]` (`file_path`, `line_number`, `column`, `function_name`, `caller_function`).
  - `get_dependencies` → `data.dependencies`: import `module_path` strings.
  - `analyze_file` → top-level `language`, `functions`, `structs`, `imports`, `exports`, `function_calls` (optionally `content`).
  - `get_repository_tree` → `data.repository_tree` + `data.metadata`.
- **Behavioral quirks the gold set must encode** (as expectations or `known_failure` entries):
  1. The `language` filter on `search_functions`/`search_structs` is parsed but **never applied** — ship a `known_failure` case so the ledger documents it and flips to passing when fixed.
  2. `find_callers` is an **exact key lookup** in the call graph (no pattern). Gold uses exact names; include a method-call attribution probe (`self.foo()`/`obj.foo()`).
  3. `find_functions`/`find_structs` try exact-name first, fall back to pattern only on a miss — cover both branches.
  4. `get_dependencies`/`analyze_file` look files up by the stored path string — discover the stored convention empirically (§4.3 path normalization), don't assume.
- Registered analyzers: Rust, Python, TypeScript/TSX only. A `.go` file is a genuine "unsupported language" probe.
- Layer 2 arm-B treatment: the skill at `skills/loregrep/SKILL.md`. The pi extension is untested against a live runtime — don't build Layer 2 on it.

## 1. Phased implementation plan

| Phase | Deliverable | Depends on |
|---|---|---|
| **P0** | `evals/` scaffolding; `rust-basic` fixture + gold (all 6 tools); `.gitignore` for `.loregrep/` | — |
| **P1** | Layer 1 runner (`evals/retrieval/run.py`) + JSONL + summary + non-zero exit on regression; wire into CI | P0 |
| **P2** | `python-basic`, `ts-basic`, `mixed-small` fixtures + gold; cold/warm latency; `known_failures.json` seeded with the language-filter bug | P1 |
| **P3** | Layer 2 MVP: Claude Code headless, 2 arms, 3 code-QA tasks on pinned loregrep repo, N=5, JSONL | P1 |
| **P4** | Full task suite (10–15 tasks incl. negative controls), oracle scripts, `analyze.py` (Wilcoxon, Cliff's delta, bootstrap CIs, Pareto) | P3 |
| **P5 (v2)** | Arm C (semantic baseline), cross-model replication, larger subjects | P4 |

Layer 1 is a hard prerequisite: if retrieval quality is bad, an agent A/B measures noise.

## 2. Directory layout

```
evals/
  EVAL_PLAN.md   README.md
  fixtures/                          # Layer 1: pinned hand-written sample repos
    rust-basic/   { src/..., gold/cases.json, .gitignore }
    python-basic/ { pkg/..., gold/cases.json, .gitignore }
    ts-basic/     { src/..., gold/cases.json, .gitignore }
    mixed-small/  { rust+py+ts+one .go probe, gold/cases.json, .gitignore }
  retrieval/
    run.py                           # the runner (Python 3, stdlib only)
    known_failures.json              # xfail ledger: id + reason + issue
    results/                         # gitignored JSONL
  agent/
    tasks/<task-id>/{task.json, oracle.sh, setup.sh?}
    subjects/{loregrep@<sha>/, fetch.sh}
    run_ab.py   arms/{baseline/, loregrep/}   analyze.py   results/
```

Add `evals/` to `Cargo.toml` `exclude`.

## 3. Schemas

### 3.1 Gold case (`fixtures/<fixture>/gold/cases.json`, array of cases)

```json
{
  "id": "rust-basic/find_callers/parse_config",
  "tool": "find_callers",
  "params": { "function_name": "parse_config" },
  "extract": "data.callers",
  "match_on": ["file_path", "line_number"],
  "expect": {
    "mode": "exact_set",
    "items": [
      { "file_path": "src/main.rs",   "line_number": 14 },
      { "file_path": "src/loader.rs", "line_number": 41 }
    ]
  },
  "tags": ["rust", "callers", "cross-file"],
  "notes": "A commented-out call and a string-literal mention must NOT appear."
}
```

- `extract`: dot-path to the array/scalar to score (`data.callers`, `data.results`, `data.dependencies`, `data.functions`, …).
- `match_on`: projection of each item for set comparison (extra fields ignored, so schema growth doesn't break gold). Omit for string arrays (deps).
- `expect.mode`: `exact_set` (P=R=1 to pass) · `superset` (all `items` present; pass if recall=1 and precision ≥ `min_precision`, default 1) · `error` (`success:false`, `expect.error_contains`).
- Gold `file_path` is **repo-relative**; runner normalizes result paths to the fixture root.
- `known_failures.json` entries `{id, reason, issue}` are reported but don't fail CI; an unexpectedly *passing* known-failure DOES fail CI (forces ledger cleanup) — like pytest `xfail`.

### 3.2 Layer 1 results JSONL (one line/case)

```json
{"schema":"loregrep-eval-retrieval/1","run_id":"<ts>-<rand>","case_id":"...","fixture":"rust-basic","tool":"find_callers","git_sha":"...","binary":"target/release/loregrep","passed":true,"known_failure":false,"precision":1.0,"recall":1.0,"f1":1.0,"false_positives":[],"false_negatives":[],"latency_ms_cold":412,"latency_ms_warm":38,"exit_code":0,"stderr_tail":""}
```

Plus a `loregrep-eval-retrieval-summary/1` line (per-tool aggregate P/R, pass counts, total wall time).

### 3.3 Layer 2 results JSONL (one line/agent run)

```json
{"schema":"loregrep-eval-agent/1","experiment_id":"...","run_id":"task03-armB-rep4","task_id":"...","arm":"loregrep","rep":4,"subject":"loregrep@<sha>","model":"<pinned id>","driver":"claude-code","wall_clock_s":143.2,"num_turns":11,"tool_calls":{"Bash":6,"Read":3,"Grep":0,"loregrep_exec_tool":4},"tokens":{"input":48210,"output":3110,"cache_read":122400},"cost_usd":0.41,"oracle_pass":true,"agent_exit_code":0,"timed_out":false,"transcript_path":"...","notes":""}
```

`tool_calls.loregrep_exec_tool` counted by scanning the stream-json transcript for Bash commands matching `\bloregrep\s+exec-tool\b`.

## 4. Layer 1 — retrieval-quality runner

### 4.1 Invocation: a standalone Python 3 (stdlib-only) script shelling out to the built binary
Not a `cargo test`, not a crate bin. Rationale: agents consume the **CLI surface** (arg parsing, stdout purity, exit codes, `.loregrep` cache) — an in-process test misses exactly the class of regression (stray `println!` polluting stdout) that breaks agents. Stdlib-only keeps CI dependency-free.

```
python3 evals/retrieval/run.py [--binary target/release/loregrep] [--fixture rust-basic] [--case <glob>] [--json-out ...] [--no-latency]
```
CI: `cargo build --release && python3 evals/retrieval/run.py` → non-zero on any non-known-failure failure.

### 4.2 Fixtures (hand-written, committed, never auto-formatted)
~12–20 small files each, line numbers reviewable by hand, with deliberate traps distinguishing structural search from grep:
- a name that also appears in a comment, a string literal, and a doc example (grep FPs to exclude);
- two same-named functions in different files (disambiguate by `file_path`);
- exact-vs-prefix names (`parse` vs `parse_config`) → exact-match-first branch;
- a regex-pattern case (`^handle_.*`) → fallback branch;
- cross-file call chains for `find_callers` (free fn from `main` + from a method) + one commented-out call;
- structs/classes with generics, tuple structs (Rust), dataclasses (Py), interfaces + tsx components (TS);
- an import graph for `get_dependencies`;
- `mixed-small` adds a `.go` file → the `analyze_file` `error` case.

Coverage: every tool × every applicable fixture language; ≥30 cases in P2. This is the **contribution gate** — a new-language analyzer PR must add `fixtures/<lang>-basic/` with gold covering all 6 tools.

### 4.3 Runner algorithm
Per fixture: `rm -rf .loregrep` (cold); run first case cold (`latency_ms_cold`), rerun warm (`latency_ms_warm`), rest warm. Per case: `subprocess.run([binary,"exec-tool",tool,"--params",json.dumps(params),"--path",fixture_dir],capture_output=True,timeout=60)`; parse stdout JSON; record exit code + stderr tail. **Path normalization**: relativize each result `file_path` to the fixture root (POSIX). Project onto `match_on`; set-compare per `expect.mode`; compute P/R/F1; record FP/FN verbatim. Emit JSONL + human summary; exit 1 on unexpected failure/pass. Latency never gates pass/fail.

## 5. Layer 2 — statistical agent A/B

### 5.1 Driver: Claude Code headless (`claude -p "<prompt>" --output-format stream-json`)
It *is* the deployment target; reports `usage`/`num_turns`/`duration_ms`/`total_cost_usd`; stream-json gives a transcript for tool-call counting. (Direct API loop = evaluating a bespoke agent nobody uses + reimplementing grep/read = confound. pi = untested runtime.) Record the Claude Code version per row; temperature isn't controllable — accept it, control via reps + interleaving.

### 5.2 Arms (fresh temp workspace per run: subject copy + arm overlay, deleted after)
- **A (baseline)**: no skill; `loregrep` not on PATH; deny rule `Bash(loregrep*)`; tools Bash/Read/Grep/Glob/Edit.
- **B (loregrep)**: + `.claude/skills/loregrep/SKILL.md`, binary on PATH, allow `Bash(loregrep *)`; index pre-warmed once per (task, arm-B) template (see §6.7).
- **C (semantic)**: v2. `arm` is a free string so adding it is additive.
Prompts byte-identical across arms.

### 5.3 Task suite (v1: 12 tasks, automated oracles), frozen/committed before the first run (pre-registration)
Subjects: `loregrep@<sha>`, `test-repos/serde` (vendored), one pinned mid Python + one TS by SHA.
1. **Code-QA (4)** — "list every call site of X as `file:line` → ANSWER.txt"; oracle: exact-set vs gold. + "list public fns in module M", "which files import Y".
2. **Fix failing test (3)** — `setup.sh` plants a cross-file bug; oracle: target test passes AND canary still passes.
3. **Mechanical refactor (2)** — "rename X→Y everywhere"; oracle: grep clean + build/tests pass.
4. **Call-site edit (1)** — add logging before each call of X; oracle checks each call site + `cargo check`.
5. **Negative controls (2)** — single-file named-function fix; a Go/markdown-only subtree. Expect no arm difference; a "win" here signals a prompt-effect artifact.

### 5.4 Runner (`run_ab.py`)
For each (task × arm × rep) in randomized interleaved order: materialize workspace + `setup.sh`; `claude -p ... --output-format stream-json --model <pinned> --max-turns 50` with wall timer + hard timeout; save transcript; extract usage/turns/duration/cost; count tool calls (loregrep by command regex); run `oracle.sh` (network-free, `timeout 300`); append JSONL; delete workspace. Resumable (skip completed triples). `--pilot` = N=2 on 2 tasks.

### 5.5 Metrics & stats
Paired within task. N=10 full / N=5 pilot. **Unit of analysis = task** (per-task median of each metric) to avoid pseudo-replication.
- Cost: Wilcoxon signed-rank on per-task median (B−A) for tokens/turns/wall-clock; Hodges–Lehmann shift + Cliff's delta + 95% bootstrap CIs over tasks. With ~12 tasks, say so rather than chase p-values.
- Success: per-task rates; paired rate diff + bootstrap CI. **Pre-registered rule: cost claims only if arm-B success is non-inferior to A within 5pp; else the headline is the success regression.**
- Headline = **Pareto** (success rate, median tokens) and (success, median wall-clock), per task + pooled. Never report tokens without the adjacent success column.
- Negative controls reported separately.
`analyze.py` may use scipy/numpy (pinned in `evals/agent/requirements.txt`).

### 5.6 Threats & controls
prompt-effect confound (negative controls; record skill token overhead; optional placebo-skill arm A′) · selection bias (freeze tasks; include non-friendly tasks) · model/harness drift (pin IDs; interleave; one time window; never merge `experiment_id`s) · stochasticity (reps, CIs, paired non-parametric) · cache contamination (fresh workspace; report cache tokens separately) · oracle validity (deterministic committed scripts, validated once vs a known-good solution).

## 6. Open decisions (defaults)
1. Layer 1 runner language → **Python stdlib**. 2. Binary → `target/release/loregrep`, overridable. 3. Gold paths → repo-relative + runner normalization; confirm stored format empirically first. 4. `get_repository_tree` scoring → **superset** on selected keys. 5. Language-filter bug → ship as `known_failure` with an issue link. 6. Layer 2 driver → **Claude Code headless**. 7. Arm-B index → **pre-warm** (record cold first-call latency as a footnote); honest cold-every-rep alternative is one flag away. 8. Reps → **N=10 full / N=5 pilot** (≈240 runs; check pilot cost). 9. Model → pin one mid-tier ID for v1. 10. Non-inferiority margin → **5pp**, pre-registered. 11. Subjects → small vendored, others by SHA via `fetch.sh`; never float a branch.

## 7. Smallest useful first slice (one sitting, real signal)
1. `fixtures/rust-basic/` — ~8 files incl. grep-trap patterns (commented-out call, string-literal mention, same-name-two-files, exact-vs-prefix name).
2. `gold/cases.json` — **8 cases**: one per tool (6) + exact-vs-pattern pair for `search_functions` + one `find_callers` trap case.
3. `retrieval/run.py` implementing §4.3 minus latency niceties (single cold pass ok).
4. Run it → verifies the exec-tool contract end-to-end (stdout purity, cache), documents the language-filter no-op with a failing case, pins `find_callers` exact-match + comment/string exclusion (the core value claim).
5. Then a **manual Layer 2 taste test** before building `run_ab.py`: 2 code-QA tasks × 2 arms × 3 reps by hand with `claude -p --output-format json`, eyeballed. If token/turn deltas aren't visible at N=3 on the friendliest terrain, that redirects the whole Layer 2 investment before it's built.
