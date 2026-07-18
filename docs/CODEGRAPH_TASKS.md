# CodeGraph Evolution — Task Backlog

Decomposed from `docs/CODEGRAPH_EVOLUTION.md` (the source of truth for rationale; this file is
the working backlog). Verified against `main` (a88d789) and `graph-tools` (5e8acf9).

**Do not re-plan.** If a task seems wrong, check the evolution doc's cited section before
changing scope. The one-line stance for every task: *heuristic resolution with explicit
uncertainty* — never silently guess; report `Ambiguous`/`Unresolved` with candidates.

---

## Summary

| Phase | Tasks | Effort mix | Theme |
| --- | --- | --- | --- |
| P0 | 5 | 5×S | Land `graph-tools` honestly: fix staleness bug, de-merge traversal, flag ambiguity |
| P1 | 6 | 3×S, 3×M | Complete the nodes: `TypeKind`, `owner`, `supertypes` |
| P2 | 6 | 2×S, 4×M | Module graph from resolved imports (best value/effort in the whole plan) |
| P3 | 7 | 1×S, 6×M | Resolved call graph: symbol table, statused edges (the actual "CodeGraph") |
| P4 | 4 | 1×S, 3×M | Type edges from already-captured strings |
| **Total** | **28** | | plus a Parked list that is deliberately not tasks |

**Critical path** (each blocks the next):

```
P0-1 (call_graph staleness fix)
  → P0-2 (rebase graph-tools onto a88d789)
  → P0-3 ((file_path, name)-keyed BFS)
  → P0-4 (ambiguity in tool output)
  → P0-5 (collision fixture + gold)
  → P1-1 (schema fields: kind/owner/supertypes)
  → P1-2 (Rust analyzer fills them)
  → P2-1 (graph.rs scaffold + status enum)
  → P2-4 (Rust mod tree — the long pole)
  → P3-1 (symbol table) → P3-2 (edge list) → P3-3/P3-4 (resolution policies)
  → P3-5 (tools traverse resolved edges) → P3-7 (eval flip + A/B re-run)
```

P1-3/P1-4 (Py/TS analyzers), P2-2/P2-3 (Py/TS resolvers), P2-5/P2-6, P1-5/P1-6, P3-6 and all
of P4 hang off this chain but can proceed in parallel once their listed dependencies land.

---

## Phase 0 — Land the graph tools honestly (S: days)

No data-model change, no cache impact. Everything here happens in `src/storage/memory.rs`,
`src/internal/ai_tools.rs`, and `evals/`.

### P0-1 — Fix `call_graph` staleness/duplication on file re-add/remove
**What/why:** `memory.rs::remove_file_by_index` (main `src/storage/memory.rs:1065-1086`)
cleans five indexes but never touches `call_graph`; `add_file` on an existing path then calls
`update_indexes_for_file` (`memory.rs:1132-1146`) which **appends** the file's call sites
again. Any re-add (watch mode, re-scan) duplicates call sites; removal leaves stale ones.
`find_callers`, `trace_callers`, `analyze_impact` all read this map — this is the trust
blocker and task #1.
**Files/functions:** `src/storage/memory.rs::remove_file_by_index` — add a pass that removes
entries from `self.call_graph` where `CallSite.file_path == file_path` (drop emptied keys).
Land on `main` (the bug exists on main) so the P0-2 rebase inherits it.
**Acceptance:**
- New regression tests in `memory.rs` `#[cfg(test)]`: (a) `add_file(f)` → `add_file(f)` again
  → `find_function_callers(name).len()` unchanged; (b) `remove_file(f)` →
  `find_function_callers` returns no `CallSite` with `f`'s path; (c) memory: no empty
  `Vec<CallSite>` values left keyed in `call_graph`.
- `cargo test` green; existing `find_callers` gold cases in
  `evals/fixtures/rust-basic/gold/cases.json` still pass via `evals/retrieval/run.py`.
**Dependencies:** none. **Effort:** S.

### P0-2 — Rebase `graph-tools` onto main (a88d789) and clean the branch
**What/why:** `graph-tools` branched from b274d92 and predates a88d789 ("fix 3 eval-surfaced
bugs"). Merged as-is it reverts the `language`-filter fix in `src/internal/ai_tools.rs` and
the directory double-count fix in `memory.rs` (its own gold cases still carry the doubled
`file_count` as known failures). It also commits eval result artifacts.
**Files/functions:** rebase branch; verify `src/internal/ai_tools.rs::search_functions`
language filtering and `memory.rs` directory counting match main's post-a88d789 behavior;
delete committed `evals/agent/results/*.jsonl`; update
`evals/fixtures/rust-basic/gold/cases.json` /`evals/retrieval/known_failures.json` to drop
the doubled-`file_count` known-failure entries (they should now genuinely pass); keep the
branch's `pipeline_a/b/c` fixtures, `transitive_callers`, `caller_function` containment
population (`memory.rs` branch `:1215-1227`), and `run_pilot.py`.
**Acceptance:** `git merge-base main graph-tools` == main HEAD (a88d789); Layer-1 retrieval
run passes `rust-basic/search_functions/language-filter-noop` and
`rust-basic/get_repository_tree/file-count` (previously known failures); no `results/*.jsonl`
tracked; `cargo test` green including the branch's
`test_caller_function_populated_for_nested_call`.
**Dependencies:** P0-1. **Effort:** S.

### P0-3 — Re-key `transitive_callers` BFS by `(file_path, name)` with ambiguity detection
**What/why:** Branch `memory.rs::transitive_callers` (branch `:513-566`) walks caller *names*
— its own doc comment admits same-named functions merge. `CallSite` already carries
`file_path` and `caller_function` is resolved intra-file by containment, so the caller's
identity `(file_path, name)` is already in hand; the code just throws the file away when it
re-enters BFS via `find_function_callers(name)`. In Rust, `new`/`load`/`fmt` collisions are
the common case, not the corner case.
**Files/functions:** `src/storage/memory.rs::transitive_callers` — `visited` becomes
`HashSet<(String, String)>` of `(file_path, function_name)`; frontier carries the pair. When
expanding a caller name, look up `function_index[name].len()`: if `> 1`, the expansion is
tagged ambiguous (all defining files contribute as candidates rather than one merged node).
Extend `TransitiveCaller` with an `ambiguous: bool` (or `resolution` field) — additive,
in-memory only.
**Acceptance:** Unit test: two functions named `load` in different files with disjoint caller
chains — tracing through one does NOT pull in the other's callers as `exact`; the crossing is
present but flagged ambiguous. Existing `rust-basic/trace_callers/*` gold cases unchanged
(no collisions in the current fixture). Cycle-termination test still passes.
**Dependencies:** P0-2. **Effort:** S.

### P0-4 — Surface ambiguity in `trace_callers` / `analyze_impact` tool output
**What/why:** An agent handles "2 candidates, here they are" gracefully; a silently merged
count poisons the A/B result. Adopt the resolution-status vocabulary in tool JSON from day
one, before any resolver exists.
**Files/functions:** `src/internal/ai_tools.rs::trace_callers` and `::analyze_impact` (branch
`:340-420`): each caller entry gains `"resolution": "exact" | "name_ambiguous"`; responses
gain an `ambiguous_names` summary array (name + definition count + candidate files, candidates
capped at ~5 with a total count, per Open Decisions §9). `analyze_impact` counts split so
`transitive_functions` is not inflated by merged names.
`ai_tools.rs::get_tool_schemas` descriptions updated to say exactly what the numbers mean and
that ambiguous entries are candidates, not confirmed callers.
**Acceptance:** Tool-shape unit tests in `ai_tools.rs` `#[cfg(test)]` asserting the new
fields exist and `resolution` values are from the closed set; schema description test
mentions ambiguity. Existing `analyze_impact` gold counts on the collision-free fixture
unchanged.
**Dependencies:** P0-3. **Effort:** S.

### P0-5 — Same-name collision fixture + gold cases (eval coverage)
**What/why:** The plan requires the ambiguity behavior to be pinned by Layer-1 gold, not just
unit tests — gold asserts the flag, never a merged count.
**Files/functions:** `evals/fixtures/rust-basic/src/` — add two types each defining a `load`
method, feeding two *different* call chains (mirroring `pipeline_a/b/c` style); new cases in
`evals/fixtures/rust-basic/gold/cases.json` for `trace_callers` and `analyze_impact` on
`load` asserting `resolution: "name_ambiguous"` entries and the `ambiguous_names` summary;
`find_callers` case unaffected (it is per-call-site, not merged).
**Acceptance:** `evals/retrieval/run.py` passes the new cases deterministically;
`evals/retrieval/known_failures.json` gains no new entries. This fixture is deliberately the
one that flips to `Resolved` in P3-7 — name the case IDs so the flip is a one-line gold edit.
**Dependencies:** P0-4. **Effort:** S.

---

## Phase 1 — Complete the nodes (M: ~1 week)

`TreeNode`-shape changes → cache invalidation is automatic (cache version ==
`CARGO_PKG_VERSION`, checked at `src/storage/persistence.rs:137-141`; old caches regenerate).
Use serde defaults on every new field so *reading* old caches never arises. **No migration
code** (Open Decisions §9). Do NOT rename `StructSignature` → `TypeSignature` (Python
bindings + gold-case compat).

### P1-1 — Schema: `TypeKind`, `StructSignature.kind`/`supertypes`, `FunctionSignature.owner`
**What/why:** Enums and traits are currently *absent* from `extract_structs`
(`search_structs("ConfigError")` on an enum returns empty — a silent wrong answer on the
shipped surface); functions are flat with their impl/class owner discarded. One field each
fixes the model.
**Files/functions:** `src/types/struct_def.rs::StructSignature` — add
`kind: TypeKind` (`enum TypeKind { Struct, Enum, Trait, Class, AbstractClass, Interface,
TypeAlias }`, `#[serde(default)]` → `Struct`) and `supertypes: Vec<String>`
(`#[serde(default)]`); `src/types/function.rs::FunctionSignature` — add
`owner: Option<String>` (`#[serde(default)]`). Builder methods to match existing style.
**Acceptance:** `cargo test` green with all analyzers untouched (defaults apply);
round-trip serde test old-JSON-without-fields → deserializes with defaults;
`persistence.rs::test_version_mismatch` still enforces regeneration.
**Dependencies:** P0-2 (build on the landed branch). **Effort:** S.

### P1-2 — Rust analyzer: enum/trait extraction + `owner` from enclosing `impl_item`
**What/why:** `rust.rs::extract_structs` (`src/analyzers/rust.rs:392-412`) matches only
`struct_item`; `enum_item` appears only in the exports query (`rust.rs:572`). Method owner is
computed transiently for `is_static` (`rust.rs:358-380`) then discarded.
**Files/functions:** `src/analyzers/rust.rs::extract_structs` — add `enum_item` and
`trait_item` tree-sitter queries setting `kind: Enum` / `Trait`; capture trait supertraits
into `supertypes`. `rust.rs::extract_functions` — where the `impl_item` ancestor walk already
happens for `is_static`, also read the impl's type identifier into
`FunctionSignature.owner`. Also record `impl Trait for Type` pairs into the Type's
`supertypes` while touching the same queries (cheap now, consumed by P4-3).
**Acceptance:** Analyzer unit tests: an enum and a trait are returned by `extract_structs`
with correct `kind`; `Loader::load`-style method has `owner == Some("Loader")`; free function
has `owner == None`; `impl Display for Config` yields `"Display"` in `Config.supertypes`.
**Dependencies:** P1-1. **Effort:** M.

### P1-3 — Python analyzer: class `owner` + stop dropping superclasses
**What/why:** Python detects "inside a class" as a bool only (`python.rs:140-175`) and
captures superclasses (`@inheritance`, `python.rs:485-512`) then drops them because
`StructSignature` had no field — the field now exists.
**Files/functions:** `src/analyzers/python.rs` — in `extract_functions`, store the enclosing
`class_definition` name into `owner`; in `extract_structs`, set `kind: Class` and route the
already-captured `@inheritance` names into `supertypes`.
**Acceptance:** Unit tests: method of `class Foo(Base)` has `owner == Some("Foo")`;
`Foo.supertypes == ["Base"]`; module-level function `owner == None`.
**Dependencies:** P1-1. **Effort:** S.

### P1-4 — TypeScript analyzer: `kind` from existing captures + `owner` + extends/implements
**What/why:** TS surfaces class / abstract class / interface / type alias all as
`StructSignature` with no kind (`typescript.rs:430-467` already distinguishes them internally
via `@class/@interface/@type_alias` capture names — the distinction is computed then thrown
away).
**Files/functions:** `src/analyzers/typescript.rs::extract_structs` — map capture →
`TypeKind::{Class, AbstractClass, Interface, TypeAlias}`; capture `extends`/`implements`
clauses into `supertypes`. `extract_functions` — enclosing `class_declaration` name into
`owner`.
**Acceptance:** Unit tests: interface vs class vs type alias get distinct `kind`s; a class
`implements IThing extends Base` yields both in `supertypes`; method owner set.
**Dependencies:** P1-1. **Effort:** M.

### P1-5 — Surface `kind`/`owner` through RepoMap and the tool layer
**What/why:** The new fields must reach agent-visible output: qualified display names
(`Loader::load` instead of bare `load`), kind-aware struct search, and the
`is_enum: false // TreeNode doesn't distinguish` TODO at `memory.rs:728` resolved.
**Files/functions:** `src/storage/memory.rs::find_structs_with_options` /
`find_functions_with_options` — optional `kind` / `owner` filters;
`memory.rs::generate_file_skeleton` (`:707`, the `is_enum` TODO at `:728`) — derive from
`kind`. `src/internal/ai_tools.rs::search_structs` / `search_functions` — optional `kind`
and `owner` input params in `get_tool_schemas`; `find_callers` / `trace_callers` /
`analyze_impact` output rendered as `Owner::name` when owner is present.
**Acceptance:** Tool tests: `search_structs {pattern: "ConfigError", kind: "enum"}` returns
the enum; `trace_callers` output shows `Loader::load`; skeleton JSON marks enums as enums.
**Dependencies:** P1-2 (Rust minimum; P1-3/P1-4 for their languages). **Effort:** M.

### P1-6 — Eval coverage: enum/trait search + owner attribution gold (eval coverage)
**What/why:** The plan names these gold cases explicitly: enum search (`ConfigError`), trait
search, method-owner attribution, same-name-different-owner disambiguation.
**Files/functions:** `evals/fixtures/rust-basic/src/` — add a `ConfigError`-style enum and a
trait with impls (may share the P0-5 collision types); new cases in
`evals/fixtures/rust-basic/gold/cases.json`. Python/TS fixture equivalents deferred until
those fixtures exist (tracked by EVAL_PLAN P2 — note in the cases, don't build fixtures here).
**Acceptance:** `run.py` deterministic pass: enum found by `search_structs`; trait found;
two same-named methods distinguished by `owner` in output.
**Dependencies:** P1-5. **Effort:** S.

---

## Phase 2 — Module graph from resolved imports (M) — **best value/effort in the plan**

Pure derived state: computed from existing `ImportStatement`s after scan/load. **Zero
`TreeNode` change, zero cache impact, nothing serialized** — persistence keeps rebuilding
everything via `add_file` (`persistence.rs:144-151`). Ship order per the 80/20 table: TS
first (near-trivial, validates the tool UX), Python second, Rust mod-tree last (long pole).

### P2-1 — `src/storage/graph.rs` scaffold: module graph + resolution-status type
**What/why:** Cross-file resolution can't live in per-file analyzers (keeps the
`LanguageAnalyzer` trait untouched for the language-addition path). This module consumes
`&[TreeNode]` post-add and owns all derived graph state from here through P4.
**Files/functions:** New `src/storage/graph.rs` (registered in `src/storage/mod.rs`):
`enum ImportTarget { File(usize /*file index*/), External(String), Unresolved(String) }`;
`ModuleGraph { forward: Vec<Vec<(ImportTarget, ..)>>, reverse: Vec<Vec<usize>> }`; a
`build_module_graph(&[TreeNode], resolvers) -> ModuleGraph` entry point dispatching per
`TreeNode.language`; rebuild hooks after bulk scan and after `RepoMap` load (rebuild-all —
per-file patching is explicitly parked). Unknown languages → every import `Unresolved`.
**Acceptance:** Unit test with a hand-built two-file `TreeNode` set and a stub resolver:
forward and reverse adjacency consistent; unresolved imports carried, never dropped;
rebuilding after a file replace leaves no stale reverse edges (the P0-1 lesson, by
construction).
**Dependencies:** P1-1 (lands after Phase 1 merges; no schema need, ordering only).
**Effort:** M.

### P2-2 — TypeScript import resolver (relative-specifier probing)
**What/why:** Relative specifiers are file paths — `./x` → probe `x.ts` / `x.tsx` /
`x/index.ts`. Bare specifiers → `External`. tsconfig path aliases → `Unresolved(specifier)`
in v1 (parked; the status makes the gap visible in evals).
**Files/functions:** `graph.rs::resolve_ts_import(specifier, from_file, known_files) ->
ImportTarget` over `ImportStatement.module_path` strings.
**Acceptance:** Unit tests: `./x`, `../a/b`, `./dir` (index probing), `react` → `External`,
`@app/thing` → `Unresolved`. No filesystem access — probe against the scanned file set.
**Dependencies:** P2-1. **Effort:** S.

### P2-3 — Python import resolver (dotted paths, packages, relative levels)
**What/why:** Dotted module path → file path under the scanned roots; packages via
`__init__.py`; relative imports by level counting (`from ..a import b`). Site-packages →
`External`.
**Files/functions:** `graph.rs::resolve_py_import(...) -> ImportTarget`; uses
`ImportStatement` fields (module path + relative level if captured; extend the Python
analyzer's import extraction only if the level isn't already in the string).
**Acceptance:** Unit tests: absolute dotted import, package `__init__.py` target, `from . import
x`, `from ..pkg import y`, `numpy` → `External`. Unresolvable stays `Unresolved`, never
guessed.
**Dependencies:** P2-1. **Effort:** M.

### P2-4 — Rust import resolver: `mod` tree + `use`-path mapping
**What/why:** The long pole. Build the module tree from `mod foo;` declarations plus
`mod.rs`/`foo.rs` file conventions to map `crate::a::b` / `super::x` / `self::y` paths to
files. `use` items are already parsed as strings. External crates → `External`. `pub use`
re-export chains → `Unresolved` in v1 (parked, per Open Decisions §9). This also yields
canonical `crate::mod::Item` names for free — keep the mod-tree structure accessible for P3.
**Files/functions:** `graph.rs::build_rust_mod_tree(&[TreeNode])` +
`resolve_rust_import(...) -> ImportTarget`.
**Acceptance:** Unit tests: `crate::config::Config` → `src/config.rs`; nested
`mod a { }` inline vs `a/mod.rs` vs `a.rs`; `super::` from a submodule;
`std::collections::HashMap` → `External`; a `pub use` chain target → `Unresolved`. This repo
(`loregrep` itself) resolves its own `crate::` imports with zero panics (smoke test).
**Dependencies:** P2-1. **Effort:** M (largest in phase).

### P2-5 — Tools: `find_importers`, `get_dependency_graph`, upgraded `get_dependencies`
**What/why:** The agent-visible payoff: reverse dependency closure ("who imports X,
transitively") is grep-impossible through aliases and transitivity; cycles fall out of the
same graph.
**Files/functions:** `src/internal/ai_tools.rs::get_tool_schemas` + `execute_tool` — new
tools `find_importers { file, transitive? }` (reverse closure over `ModuleGraph.reverse`)
and `get_dependency_graph { scope }` (subgraph + cycle report);
`ai_tools.rs::get_dependencies` upgraded to emit resolved file path + status per import
(**additive** fields; keep raw `module_path` — its shape is in shipped gold cases).
`memory.rs::get_file_dependencies` keeps returning strings for API compat.
**Acceptance:** Tool tests: transitive importer set correct on a 3-file chain; a synthetic
cycle reported; `get_dependencies` output still contains the old fields byte-for-byte plus
new ones; existing `rust-basic/get_dependencies/main` gold passes unmodified.
**Dependencies:** P2-2 (minimum one resolver; extend as P2-3/P2-4 land). **Effort:** M.

### P2-6 — Eval coverage: per-language import gold + cycle + unresolved-alias (eval coverage)
**What/why:** Plan names the cases: TS `./x` probing, Python package imports, Rust
`crate::`/`super::`, an unresolvable-alias case asserting `Unresolved` (not a guess), a
synthetic cycle. Layer 2 A/B task: "what breaks if this module's interface changes".
**Files/functions:** `evals/fixtures/rust-basic/` additions for Rust cases; minimal new
`evals/fixtures/ts-basic/` and `evals/fixtures/py-basic/` fixtures (these unblock P1-6's
deferred cases too); gold in each fixture's `gold/cases.json`; A/B task config in
`evals/agent/` (pattern: existing `run_pilot.py` tasks).
**Acceptance:** Layer-1 deterministic pass on all listed cases; the tsconfig-alias case
asserts `status == "unresolved"`; Layer-2 task runs baseline-vs-loregrep and records results
(win not required to merge — the measurement is the deliverable).
**Dependencies:** P2-5. **Effort:** M.

---

## Phase 3 — Resolved call graph (L: 2–3 weeks) — the actual "CodeGraph"

All in-memory, all in `src/storage/graph.rs`. Symbol IDs are **ephemeral `u32` per index
build — never serialized, never escape a build** (persistence rebuilds from `TreeNode`s;
`(path,name,line)` keys churn under edits). Incremental rule: replace the changed file's
`TreeNode`, rebuild all derived state; per-file patching is parked behind a benchmark.
Tool names and response shapes do not change — outputs gain fields. The name-keyed
`call_graph: HashMap<String, Vec<CallSite>>` becomes a derived compatibility view feeding
`find_callers` (its schema is in shipped gold).

### P3-1 — Symbol table build
**What/why:** The substrate: every function/method/type in the repo gets an ephemeral
identity so edges can point at definitions instead of names.
**Files/functions:** `graph.rs`: `Symbol { id: SymbolId(u32), kind, name, owner:
Option<String>, file: usize, span }`; `symbols: Vec<Symbol>` built per index run from
`&[TreeNode]` (functions + structs; dedup key `(file, kind, name, span)`); lookup maps
`name → SmallVec<SymbolId>` and `(owner, method_name) → SymbolId`.
**Acceptance:** Unit tests: two same-named functions in different files get distinct IDs;
`(owner, method)` lookup distinguishes `Loader::load` from `Config::load`; rebuild after a
`TreeNode` replace produces a consistent table (IDs may differ — nothing may hold old ones).
**Dependencies:** P1-2 (needs `owner`), P2-1. **Effort:** M.

### P3-2 — Edge list + `CalleeRef` + adjacency
**What/why:** The proposal's arena/edge/adjacency design lands here as an implementation
detail, not a migration.
**Files/functions:** `graph.rs`: `CallEdge { caller: SymbolId, callee: CalleeRef, site:
(file, line, col) }` with `CalleeRef = Resolved(SymbolId) | Ambiguous(SmallVec<SymbolId>) |
External(String) | Unresolved(String)`; outgoing/incoming adjacency vectors; edges built from
`TreeNode.function_calls` + the branch's containment-derived `caller_function`.
**Acceptance:** Unit tests: adjacency symmetric with the edge list; call sites with no
enclosing function attach to a per-file module-level pseudo-caller or are retained with a
`None` caller (pick one, test it); rebuild leaves no stale edges after file replace.
**Dependencies:** P3-1. **Effort:** M.

### P3-3 — Free-function resolution policy (per language, one result type)
**What/why:** Three small per-language policies, NOT one resolver abstraction. Policy (all
languages): same-file definition ≻ imported definition (via P2 import tables / Rust mod tree)
≻ unique global ≻ `Ambiguous(candidates)`. Nothing deeper.
**Files/functions:** `graph.rs::resolve_free_call_{rust,py,ts}` (or one function
parameterized by the language's import table), consuming P2's `ModuleGraph` + P3-1 lookup
maps; each independently testable.
**Acceptance:** Unit tests per language: same-file win; import win over a same-named
definition elsewhere; two unimported globals → `Ambiguous` with both candidates; unknown
name → `Unresolved`. Never a silent single guess when candidates > 1.
**Dependencies:** P3-2, P2-2/P2-3/P2-4 (each language's policy needs its resolver).
**Effort:** M.

### P3-4 — Method resolution: `self` via owner, param-typed receivers, else Ambiguous
**What/why:** The 80/20 of receiver resolution — and the hard stop. `receiver_type` on
`FunctionCall` holds the receiver *expression text* (`self`, `cfg`, `foo.bar`), not a type
(`rust.rs:630-660`, same in `python.rs`/`typescript.rs`) — so: (1) `self.foo()` → the
enclosing function's `owner` type; (2) receiver matches a parameter name of the enclosing
function → its declared `param_type` (`src/types/function.rs::Parameter.param_type`, already
captured); (3) everything else → `Ambiguous` over all `(owner, method_name)` matches. **No
dataflow, no inference** (parked).
**Files/functions:** `graph.rs::resolve_method_call(...)` using the P3-1
`(owner, method) → SymbolId` map + `FunctionSignature.parameters`.
**Acceptance:** Unit tests: `self.load()` inside `impl Loader` resolves to `Loader::load`;
`fn f(cfg: Config) { cfg.load() }` resolves to `Config::load`; `let x = make(); x.run()` →
`Ambiguous` listing every type defining `run`.
**Dependencies:** P3-3. **Effort:** M.

### P3-5 — Switch `trace_callers`/`analyze_impact` to resolved edges; confidence-split counts
**What/why:** The payoff: collisions no longer merge; responses report counts split by
confidence. Same two tool names, output gains fields — existing gold shapes keep passing.
**Files/functions:** `memory.rs::transitive_callers` re-implemented over (or delegated to)
`graph.rs` adjacency, BFS on `SymbolId`; `ai_tools.rs::trace_callers`/`analyze_impact`
responses gain e.g. `"resolved": 14, "via_ambiguous": 3` and per-caller
`resolution: "resolved" | "ambiguous"`, candidates capped at ~5 + total count.
`memory.rs::find_function_callers` now reads the derived compatibility view rebuilt from the
edge list.
**Acceptance:** All existing `trace_callers`/`analyze_impact` gold cases pass; P0-5 collision
cases show the un-merged, confidence-split counts; `find_callers` gold byte-compatible.
**Dependencies:** P3-4. **Effort:** M.

### P3-6 — New tools: `find_definition` and honest `find_references`
**What/why:** With a symbol table these are one lookup each: `find_definition { name,
from_file }` applies the same resolution policy from a file's perspective;
`find_references` = call sites (edge list) + import sites (P2 module graph). "Honest" =
statused, never guessed.
**Files/functions:** `ai_tools.rs::get_tool_schemas` + `execute_tool` + two handlers;
`graph.rs` query helpers.
**Acceptance:** Tool tests: `find_definition` from a file that imports one of two same-named
symbols returns that one as `resolved` with the other absent; `find_references` on a type
returns both its call-adjacent and import sites, each tagged with site kind.
**Dependencies:** P3-5. **Effort:** S.

### P3-7 — Eval flip + dynamic-dispatch pin + Layer-2 A/B re-run (eval coverage)
**What/why:** The plan's proof obligations: the P0-5 collision fixture flips from "flagged
ambiguous" to "resolved via import"; a param-typed receiver case; a deliberately
unresolvable dynamic-dispatch case pinned in `known_failures.json` as **by-design**
`Ambiguous`; re-run the original callers/impact A/B — the metric is unchanged win at higher
correctness.
**Files/functions:** `evals/fixtures/rust-basic/gold/cases.json` (edit P0-5 case
expectations), new fixture code for param-typed receiver + trait-object dispatch,
`evals/retrieval/known_failures.json` (the pin), `evals/agent/run_pilot.py` task re-run.
**Acceptance:** Layer-1 all green with the flipped expectations; the dispatch case appears in
`known_failures.json` with a "by design" note, not as a gold pass; A/B results committed to
the eval log location (not the repo's tracked results — see P0-2).
**Dependencies:** P3-5 (and P3-6 if its tools get gold now). **Effort:** M.

---

## Phase 4 — Type edges on the cheap (M: ~1 week)

No new data model — edges derived from strings already captured (`param_type`,
`return_type`, `field_type`, P1's `supertypes`). Derived-only: no cache impact.

### P4-1 — Type-string normalization matcher
**What/why:** `Option<Config>`, `&Config`, `Vec<Config>`, `Config[]`, `Optional[Config]`
must all match a usage query for `Config`. A contained matcher; over-matching is statused,
not guessed.
**Files/functions:** `graph.rs::type_string_mentions(type_str, target) -> Match {Exact,
Wrapped, None}` (strip refs/generics/arrays per language conventions).
**Acceptance:** Table-driven unit tests over the wrapper forms of all three languages;
negative: `ConfigError` does not match `Config`.
**Dependencies:** P1-1 (ordering; no hard dep). **Effort:** S.

### P4-2 — `find_type_usages` tool from signature/field strings
**What/why:** "Where does `Config` appear in signatures/fields" — precision grep can't match
(no comments/strings, kind-classified: param vs return vs field). Near-zero parsing cost —
the strings already exist on `FunctionSignature`/`StructField`.
**Files/functions:** `graph.rs` type-usage index (built with the other derived state);
`ai_tools.rs` new tool `find_type_usages { type }` returning usages tagged
`param|return|field` with file:line.
**Acceptance:** Tool test on a fixture type used in all three positions; gold case with a
**negative control**: the type name appearing only in a comment yields no usage.
**Dependencies:** P4-1. **Effort:** M.

### P4-3 — `find_implementations` from `supertypes` + Rust `impl Trait for Type`
**What/why:** implements/extends edges from what P1 already stores. The genuine wedge over
grep is aliased/imported base names and composition with the call graph.
**Files/functions:** `graph.rs` implements-edge index over `StructSignature.supertypes`
(P1-2/3/4) resolved through P2 import tables where possible (aliased base names);
`ai_tools.rs` new tool `find_implementations { trait_or_interface }`.
**Acceptance:** Gold: a trait with 2 impls, one referencing the trait through an import
alias — the case grep misses — both returned; unresolvable supertype names surface as
`Unresolved`, listed not dropped.
**Dependencies:** P1-2/P1-3/P1-4, P2-5 (alias resolution), P4-1. **Effort:** M.

### P4-4 — Interface-change impact: implements-edges × caller traversal (eval coverage)
**What/why:** The composition the plan calls the real value of §3.5: "what breaks if this
trait/interface changes" = implementations' methods fed into P3's `trace_callers`.
**Files/functions:** `ai_tools.rs::analyze_impact` — accept a type/trait target (additive
input field), fan out over implementing methods via P4-3's index, aggregate with existing
confidence-split counts.
**Acceptance:** Gold on the P4-3 trait fixture: impact set = union of both impls' transitive
callers, counts split by confidence; a Layer-2 A/B "interface change impact" task recorded.
**Dependencies:** P4-3, P3-5. **Effort:** M.

---

## Parked / explicitly NOT now

These are recorded so nobody relitigates the compiler (evolution doc §8 + Phase 5). They are
**not tasks**; each needs eval evidence of demand before a line is written. Until then the
uniform stance is: **report `Unresolved`/`Ambiguous` with candidates, pin known cases in
`evals/retrieval/known_failures.json` as by-design, and say so in tool descriptions** —
loregrep may say "I'm not sure (here are the candidates)"; it must never silently guess.

- **Scope/shadowing model** — import edges + status subsume the useful subset. Zero model.
- **Type inference / dataflow** — receiver resolution stops at `self` + declared parameter
  types (P3-4). `let x = make_thing(); x.run()` stays `Ambiguous`. rust-analyzer exists.
- **Dynamic-dispatch resolution** — trait objects, generics, Python duck typing, TS unions →
  `Ambiguous` with candidates, pinned in `known_failures.json` (P3-7 does the pinning).
- **Macro/decorator expansion** — macro-generated calls are invisible; documented as a known
  limitation in tool descriptions, not half-solved.
- **Persisted derived graph / precomputed reachability** — everything derived rebuilds from
  `Vec<TreeNode>` on load (`persistence.rs:144-151`); BFS is sub-millisecond at target scale.
  Revisit only with a profile showing rebuild cost dominating.
- **Rust `pub use` re-export chains** — `Unresolved` in P2 v1; the long tail.
- **tsconfig path aliases** — `Unresolved(specifier)` in P2 v1; the status makes the gap
  visible in evals, which then justify (or don't) a v2.
- **Cross-dependency resolution** — `std`, crates.io, site-packages, `node_modules` are
  `External`: terminal, never traversed.
- **Body-level reference extraction** — field access, constructors-as-expressions, trait
  bounds at use sites: new extraction surface × 3 languages, modest value over grep.
- **Watch-mode per-file delta patching of adjacency** — rebuild-all-derived is correct by
  construction (the P0-1 bug is exactly what hand-patched derived state produces); patch
  later only behind a benchmark.
- **A unified "resolver" abstraction** — three per-language policies behind one result enum,
  forever.
- **`StructSignature` → `TypeSignature` rename** — serde/API compat (Python bindings, gold
  cases); `kind` carries the semantics.
