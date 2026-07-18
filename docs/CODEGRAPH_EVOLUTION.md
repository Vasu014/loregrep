# RepoMap → CodeGraph: Evaluation & Evolution Plan

Status: reviewed against actual code on `main` (a88d789) and branch `graph-tools` (5e8acf9), 2026-07.
Every claim below cites the file/line it was verified against.

---

## 1. Verdict

**The diagnosis is right; the prescription is over-built.**

The proposal's core observation is correct and verifiable: `RepoMap` is a *name-keyed search
index over parsed files*, not a graph. Every cross-file structure is keyed by bare string name
(`function_index: HashMap<String, Vec<usize>>`, `call_graph: HashMap<String, Vec<CallSite>>`,
`src/storage/memory.rs:169-177`), and the just-built graph tools (`trace_callers`,
`analyze_impact` on `graph-tools`) traverse by name, explicitly documented as "an accepted
simplification for v1" in the branch's own doc comment. Since loregrep's validated wedge is
*correct* grep-impossible graph queries, name-merging is not a nit — it is the thing that will
lose agent trust first. In Rust specifically, almost every type defines `new`, `default`,
`from`, `load`, `fmt`; method-name collision is the common case, not the corner case.

But the proposal then reaches for compiler-shaped machinery — a general symbol resolver, a
scope model, receiver-type resolution, a full containment tree, persisted reverse-reachability
— whose last 20% costs 5x and whose first 80% is achievable with per-language heuristics plus
an explicit "I'm not sure" channel. The one idea in the proposal that should be adopted
*verbatim* is the resolution-status enum (`Resolved/Ambiguous/External/Unresolved`): an agent
handles "2 candidates, here they are" gracefully; a silently merged wrong answer poisons the
A/B result that justifies the whole product.

Two things the proposal **missed** that reading the code surfaced:

1. **A live correctness bug, worse than the consistency issue it describes.**
   `remove_file_by_index` (`src/storage/memory.rs:1065-1086`) cleans five indexes but never
   touches `call_graph`. `add_file` on an existing path calls `remove_file_by_index` then
   `update_indexes_for_file`, which *appends* call sites again (`memory.rs:1132-1146`). So any
   file re-add (watch mode, re-scan without cold start) **duplicates that file's call sites and
   leaves stale ones after removal**. `find_callers`, `trace_callers` and `analyze_impact` all
   read this map. This must be fixed before any of them is trusted, and it costs an afternoon.
2. **Nodes are missing before edges are wrong.** Rust `extract_structs` matches only
   `struct_item` (`src/analyzers/rust.rs:398-412`); `enum_item` and `trait_item` are never
   extracted as types (enums appear only in the exports query, `rust.rs:572`). A
   `search_structs("ConfigError")` on an enum-based error type — i.e., most Rust error types —
   returns an empty result today. That is a silent wrong answer on the *existing* tool surface
   and ranks above most of the proposed graph work.

Also of practical note: `graph-tools` branched from b274d92 and **predates** main's a88d789
("fix 3 eval-surfaced bugs"). The raw diff appears to revert the `language`-filter fix in
`src/internal/ai_tools.rs` and the directory double-count fix in `memory.rs` (the branch's own
gold cases still carry the doubled `file_count` as a known failure). The branch must be rebased
onto main before merge, or it will genuinely revert shipped fixes.

---

## 2. Ground truth (what the code actually is)

| Fact | Where |
| --- | --- |
| Core store: `files: Vec<TreeNode>`; six `HashMap<String, Vec<usize>>` name→file-index maps | `src/storage/memory.rs:161-188` |
| Removal reindexes **every** entry of every index (O(total index entries) per removal) | `memory.rs:1211-1269` |
| `call_graph` keyed by bare callee name; written in `update_indexes_for_file`; **never cleaned on remove** | `memory.rs:177, 1132-1146, 1065-1086` |
| `caller_function` is `None` on main (TODO); populated by innermost line-range containment on `graph-tools` | `memory.rs:1139`; branch diff of `update_indexes_for_file` |
| `trace_callers` BFS walks caller *names*; same-named functions merge (documented) | branch `transitive_callers` doc comment |
| `FunctionCall.receiver_type` holds the receiver **expression text** (`self`, `cfg`, `foo.bar`), not a type — the `@receiver` capture is passed straight to `with_method_call` | `src/analyzers/rust.rs:630-660`, `src/types/function.rs:156-186`; same pattern `python.rs:746`, `typescript.rs:761` |
| Functions are flat: impl context used only to set `is_static`; owner type discarded | `rust.rs:358-380` |
| Python detects "inside a class" as a bool only; class name discarded; superclasses captured (`@inheritance`) then dropped — `StructSignature` has no field for them | `python.rs:140-175, 485-491` |
| TS surfaces class / abstract class / interface / type alias all as `StructSignature` with no kind field; methods flat | `typescript.rs:326, 430-443` |
| Imports are strings end-to-end: `ImportStatement.module_path`, `import_index` keyed by raw string, `get_file_dependencies` returns strings | `src/types/struct_def.rs:119-167`, `memory.rs:172, 471-480` |
| `is_enum: false // TreeNode doesn't distinguish` TODO | `memory.rs:728` |
| Persistence serializes **only** `Vec<TreeNode>` + metadata; load rebuilds all indexes/call graph via `add_file`; version-mismatch forces regeneration | `src/storage/persistence.rs:31-35, 138-151, 473-480` |
| Incremental scaffolding exists (`IncrementalUpdateInfo`: new/modified/deleted by content hash) but no delta application path yet | `persistence.rs:178-230`; `tasks.md:157-165, 385-388` |
| Eval harness: deterministic Layer-1 gold cases per tool + agent A/B; branch already added trace/impact gold cases and a cross-file `chain_a→chain_b→chain_c→parse_config` fixture | `evals/EVAL_PLAN.md`, `evals/fixtures/rust-basic/gold/cases.json` (branch) |

The persistence fact is the load-bearing one for this whole plan: **derived state is never
serialized**. Any graph we build from `TreeNode`s is free to change shape without cache-format
churn; only changes to `TreeNode` itself require a version bump, and the existing version check
already invalidates old caches. This kills the proposal's "persist the graph / persist
reverse-reachability" idea before it starts — see §5.

---

## 3. The ten claims, point by point

Format: **Real?** (cited) → **Agent value** → **Right-sized?**

### 3.1 Stable node identity (`FileId`/`SymbolId`) — "the single highest-leverage change"

**Real.** All cited structures are name/index-keyed; `reindex_after_removal` exists precisely
because `Vec<usize>` indices are unstable (`memory.rs:1211-1269`).

**Agent value: high, but indirect.** IDs ship zero agent-visible value by themselves; what
ships value is *non-merging traversal and disambiguated output*, which IDs enable.

**Challenge — this is not the first move, and the proposed key is wrong for persistence.**
Two separate claims are bundled here:

- *Traversal correctness* does **not** need an ID refactor. `CallSite` already carries
  `file_path`, and `caller_function` is resolved intra-file by containment, so the caller's
  identity is already `(file_path, name)` — the branch code just throws the file away when it
  re-enters BFS via `find_function_callers(name)`. Keying BFS nodes by `(file_path, name)` and
  flagging multi-definition names (a one-line lookup in `function_index`) is a days-sized
  change to the existing branch. That is the honest first win.
- `(file_path, name, start_line)` is a poor *durable* key — any edit above a function shifts
  `start_line`, so incremental deltas would see every symbol below an edit as delete+add. But
  since persistence rebuilds from `TreeNode`s, IDs never need to survive a process. Recommend:
  ephemeral `u32 SymbolId` assigned at index build, with `(FileId, kind, name, span)` as the
  build-time dedup key. Never serialize IDs. Revisit durable identity only if cross-session
  graph diffing becomes a feature.

**Verdict: right idea, wrong sequencing.** Do the cheap `(file_path, name)` qualification in
Phase 0; introduce real `SymbolId`s in the phase that actually needs a symbol table (resolved
call edges, Phase 3).

### 3.2 Qualified names + resolver with explicit resolution status

**Real gap.** Nothing today maps a name at a use site to a definition; `find_callers` is an
exact string lookup (noted in `EVAL_PLAN.md` §0.2).

**Agent value: the status enum is the single best *idea* in the proposal.** Adopt
`Resolved / Ambiguous(candidates) / External / Unresolved` on every edge and every tool
response, from Phase 0 onward (even before there is a resolver, "ambiguous: 3 definitions of
`load` exist" is honest and useful).

**Right-sized? The enum yes; a *general* resolver no.** "Calls/imports/types → definitions"
as one abstraction is a big lift because the three languages resolve nothing alike (§4). Build
three small per-language functions behind one result type, not one resolver. Canonical
qualified names: cheap for Python (dotted path from file location) and TS (file path *is* the
namespace), medium for Rust (requires the `mod` tree; fold into Rust import resolution).

### 3.3 Type/receiver resolution (`obj.foo()` → `Foo::bar` vs `Baz::bar`)

**Real problem, but the proposal's premise is factually wrong about the data.** There is no
"captured `receiver_type`" to use — the field holds the receiver *expression text*
(`rust.rs:630-660`). Getting actual receiver types means local type inference: let-bindings,
field chains, return types. That is compiler work with sharply diminishing returns, and it is
the biggest rabbit hole in the proposal.

**The 80/20** (worth doing, in Phase 3, after containment exists):
1. `self.foo()` → the enclosing impl/class owner type. Requires recording the owner
   (§3.6) — cheap, and `self` receivers are a large fraction of method calls in idiomatic
   Rust/Python code.
2. Receiver is a parameter of the enclosing function → its declared `param_type` is already
   captured (`function.rs:36`). One scope-free lookup.
3. Everything else → `Ambiguous` with the candidate list = all types defining that method
   name (computable once methods have owners). **Stop there.** No dataflow, no inference.

### 3.4 Imports resolved to definitions

**Real.** Verified strings-all-the-way-down (`struct_def.rs:119-167`, `memory.rs:471-480`).
`get_dependencies` returns e.g. `"crate::config"` and `"std::collections::HashMap"`
indistinguishably.

**Agent value: the best value-per-effort item in the entire proposal.** File-level resolved
import edges unlock a *reverse* dependency graph — "who imports X (transitively)", cycle
detection, module blast radius. Transitive reverse closure is genuinely grep-impossible, it is
file-granularity (no symbol resolution needed to be useful), and it composes with
`analyze_impact`. It is also pure derived state: computed at index build from existing
`ImportStatement`s, **zero `TreeNode` change, zero cache-format impact**.

**Right-sized: yes.** Per-language cost is wildly uneven (§4) — ship TS first (near-trivial),
Python second, Rust third; unresolvable targets are `External`/`Unresolved`, never guessed.

### 3.5 Inheritance / implements edges

**Real gap** — Rust traits aren't extracted at all; Python superclasses are captured then
dropped (`python.rs:486-491`); TS `implements`/`extends` not modeled.

**Challenge on value: this is more greppable than the proposal admits.** `grep "impl Display
for"`, `class Foo(Base)`, `implements IThing` all mostly work — the base name is textually
adjacent to the declaration. The genuine wedge is *composition*: interface-change impact =
implements-edges × call-graph, and aliased/imported base names that grep misses. Worth doing,
but it is a mid-tier priority (Phase 4), not a pillar. Prerequisite is merely storing what the
parsers already see (a `supertypes: Vec<String>` on the type signature).

### 3.6 Containment hierarchy (repo→module→file→impl→method→callsite)

**Real:** functions are flat; `Loader::load` is indexed as `load` with its impl owner
discarded after the `is_static` check (`rust.rs:358-380`); same flatness in Python
(`python.rs:150-158`) and TS (`typescript.rs:326`).

**Right-sized? No — a full tree is over-modeled.** The 80/20 is **one field**:
`owner: Option<String>` on `FunctionSignature` (+ the analyzers filling it from the enclosing
`impl_item` / `class_definition` / `class_declaration`, nodes they already visit). That gives
qualified display names (`Loader::load`), method-vs-free-function disambiguation, and the
prerequisite for §3.3's `self` resolution. A repo→module→…→callsite tree already
half-exists for browsing (`RepositoryTree`, `memory.rs:99-126`) and doesn't need unifying with
the symbol layer. This is the cheapest high-value schema change in the plan (Phase 1).

### 3.7 Reference graph beyond calls (type usage, field access, constructors, …)

**Partially worth it, mostly later.** The unglamorous observation: type usage in *signatures*
is already captured as strings — `param_type`, `return_type` (`function.rs:5-7, 37`),
`field_type` (`struct_def.rs:6`). An index over existing data yields `find_type_usages`
("where does `Config` appear in signatures/fields") for near-zero parsing cost — precision
grep can't match (no comments/strings, kind-classified). Constructors, field access, trait
bounds, macros, test linkage: each is a new extraction surface × 3 languages with modest
value over grep. Defer all of it; body-level references only if eval evidence demands.

### 3.8 Scope model (local/imported/shadowed/external)

**Skip. Explicitly.** This is the LSP rabbit hole. Shadowing and lexical scopes matter for a
rename engine, not for an agent asking repo-level structure questions. The useful subset —
"`Config` here is the one imported from `src/config.rs`" vs "ambiguous" — falls out of import
resolution (§3.4) plus the status enum. Zero additional model.

### 3.9 Under-modeled types (enum/struct conflation, missing trait/mod/alias/etc.)

**Real, and *worse* than stated** — not conflated but **absent**: no `enum_item` or
`trait_item` in `extract_structs` (`rust.rs:398-412`), so today's `search_structs` silently
returns nothing for enums and traits. TS conflates class/interface/type-alias with no kind
field (`typescript.rs:433-437`). The `is_enum: false` TODO at `memory.rs:728` documents it.

**High priority — this is a correctness hole in the shipped tool surface**, not graph
gold-plating. Fix: `kind: TypeKind {Struct, Enum, Trait, Class, AbstractClass, Interface,
TypeAlias}` on `StructSignature` (serde-defaulted for compat) + the missing tree-sitter
queries. Visibility-path, consts, macros: defer; they appear in exports already when public.

### 3.10 Index consistency / storage (arena, edge list, adjacency, persisted reachability)

**The consistency complaint is validated by a real bug** (call_graph staleness, §1) and by
`reindex_after_removal`'s O(everything) walk. Fix the bug immediately (Phase 0); the walk is
ugly but correct and not hot until watch-mode ships.

**The storage prescription is half right, half gold-plating:**
- *Right:* an explicit edge list + in-memory adjacency (outgoing/incoming) once edges carry
  resolution status — that is just the natural shape of Phase 3.
- *Over-built:* arena/`SlotMap` as a standalone migration (adopt it *inside* Phase 3, where a
  symbol table appears anyway), and **precomputed/persisted reverse-reachability**. At
  loregrep's scale (thousands of files, ~10⁴–10⁵ call edges) a BFS is sub-millisecond; the
  branch's own BFS over the whole eval fixture is unmeasurable. Persisting derived
  reachability buys nothing measurable and costs an invalidation protocol interlocked with
  incremental deltas — the classic cache-of-a-cache trap. Keep the invariant *"everything
  derived is rebuilt from `TreeNode`s"* (which persistence already enforces,
  `persistence.rs:144-151`) until a profile says otherwise.

---

## 4. Per-language resolution reality (the 80/20 table)

A "general resolver" is a big lift precisely because these columns share almost nothing:

| | TypeScript/TSX | Python | Rust |
| --- | --- | --- | --- |
| Import → file | **Trivial-to-easy.** Relative specifiers are file paths (`./x` → `x.ts/x.tsx/x/index.ts` probing). Bare specifiers → `External`. Skip `tsconfig` path aliases in v1 (→ `Unresolved`, statused). | **Easy-medium.** Dotted module path → file path under the scanned roots; handle packages (`__init__.py`) and relative imports (level counting). Site-packages → `External`. | **Medium.** Requires building the `mod` tree from `mod foo;` declarations + `mod.rs`/`foo.rs` conventions to map `crate::a::b`. `use` items are already parsed. External crates → `External`. Skip `pub use` re-export chains in v1 (statused). |
| Callee → definition | Imported-name match ≻ same-file ≻ ambiguous. Default exports and object destructuring add noise — accept `Ambiguous`. | Same, plus module-attribute calls (`mod.f()`) via import alias table. Dynamic dispatch → `Ambiguous` by design. | Free fns: same-file ≻ imported ≻ single-global ≻ `Ambiguous`. Methods: `self` via owner, param-typed receivers, else `Ambiguous` (§3.3). No trait-object/generic dispatch — ever. |
| Canonical name | file path + export name (free) | dotted module path (nearly free) | `crate::mod::Item` (falls out of the mod tree) |

Sequencing inside Phase 2/3 follows this table: TS lands first and validates the tool UX
cheaply; Rust module-tree work is the long pole and is also what the eval fixtures exercise
most, so it gets the deepest gold coverage.

---

## 5. Interaction with what already exists

- **Persistence (shipped, gzip-JSON).** `SerializedRepoMap` = header + metadata +
  `Vec<TreeNode>`; *all* indexes and the call graph are rebuilt through `add_file` on load
  (`persistence.rs:144-151`). **Keep this invariant.** Consequences: (1) the graph never
  serializes — no graph cache format, no graph migration code; (2) only phases that touch
  `TreeNode`/`FunctionSignature`/`StructSignature` (Phases 1, 4) affect the cache, and the
  existing version check (`persistence.rs:138`) already forces clean regeneration — use
  serde defaults so *reading* old caches simply never arises; (3) load cost grows by one
  resolver pass — measure it in the existing cold/warm latency evals before optimizing.
- **Incremental deltas (planned).** `IncrementalUpdateInfo` diffs by content hash
  (`persistence.rs:178-230`) but nothing applies deltas yet. Design rule: incremental unit =
  *replace the changed file's `TreeNode`, then rebuild derived state*. Rebuild-all-derived is
  correct by construction and cheap at target scale; per-file patching of adjacency is an
  optimization with its own bug class (the call_graph staleness bug is exactly what hand-patched
  derived state produces). Do rebuild-all first, patch later behind a benchmark. Ephemeral
  symbol IDs (§3.1) make this safe — nothing outside a single build ever holds an ID.
- **Eval harness.** Layer 1 gold cases are the contract for every phase (`evals/EVAL_PLAN.md`
  §1: "Layer 1 is a hard prerequisite"). Each phase below names its fixture additions. The
  `known_failures.json` ledger is the right home for *documented* ambiguity limitations
  (e.g., "trait-object dispatch returns Ambiguous by design"). Layer 2 A/B answers the only
  question that matters per new tool: do agents *adopt* it and win on tokens/turns.
- **The `graph-tools` branch evolves, is not thrown away.** Its containment-based
  `caller_function` population, BFS shape, cycle handling, tool schemas, and the
  `pipeline_a/b/c` + gold-case eval work all carry forward. Phase 0 = rebase + harden it;
  Phase 3 swaps its traversal keys from names to symbol IDs behind the same two tool names.

---

## 6. Ranked priorities (agent value ÷ effort)

| # | Change | Agent value | Effort | Notes |
| --- | --- | --- | --- | --- |
| 1 | Fix call_graph staleness; rebase & land `graph-tools` with `(file_path, name)`-keyed traversal + ambiguity flag | Very high (trust in the flagship tools) | S (days) | No schema change |
| 2 | Type kinds (enum/trait/interface/alias) + `owner` on functions | High (fixes silent empty results; qualified method names) | M (~1 wk) | `TreeNode` change → cache version bump |
| 3 | Import → file resolution + module-graph tools | High (new grep-impossible capability) | M (TS/Py) + M (Rust mod tree) | Derived only; no cache impact |
| 4 | Resolved call edges + symbol table + status on every edge | High | L (2–3 wk) | The real "CodeGraph"; arena/adjacency live here |
| 5 | Type-usage index from existing signature strings; implements/extends edges | Medium | M | Edges from data already captured |
| 6 | Receiver resolution beyond `self`/params; body-level references | Low-medium | L each | Only if evals demand |
| — | Scope model, type inference, persisted reachability, re-export chains, macro expansion | — | — | **Deliberately not built** (§8) |

The proposal's claim that stable IDs are "the single highest-leverage change" is therefore
**half-endorsed**: identity *discipline* is #1, but it arrives as a days-sized fix to the
existing branch, not as an ID-system migration; the ID system proper is deferred to #4 where
it pays for itself.

---

## 7. Phased plan

Every phase ships a user-visible tool improvement, has eval coverage, and is individually
abandonable. No phase is a rewrite; `RepoMap`'s public API (`find_functions`, `find_callers`,
…) survives every phase as a compatibility surface.

### Phase 0 — Land the graph tools honestly (S: days) — do first

*Data model:* none (no `TreeNode` change, no cache impact).
1. Rebase `graph-tools` onto main (a88d789) — verify the language-filter and directory
   double-count fixes survive; delete the branch's committed `results/*.jsonl`.
2. Fix the staleness bug: `remove_file_by_index` must remove the file's entries from
   `call_graph` (they are identifiable by `CallSite.file_path`). Add a regression test:
   add file → re-add same path → `find_callers` count unchanged.
3. Re-key `transitive_callers` BFS nodes as `(file_path, function_name)` — data already in
   `CallSite` + `caller_function`. When expanding, a name with multiple definitions
   (`function_index[name].len() > 1`) contributes candidates tagged ambiguous rather than
   silently merging.
4. Surface uncertainty in tool output: `trace_callers`/`analyze_impact` responses gain
   `"resolution": "exact" | "name_ambiguous"` per caller and an `ambiguous_names` summary.
   Tool descriptions updated to say what the numbers mean.

*Unlocks:* trustworthy `trace_callers` / `analyze_impact` — the A/B-tested wedge, minus its
known lie.
*Evals:* extend `rust-basic` with a deliberate collision (two `load` methods on different
types feeding different chains); gold asserts the ambiguity flag, not a merged count. Existing
`pipeline_a/b/c` cases carry over.
*Risk:* low. *Incremental/persistence:* none.

### Phase 1 — Complete the nodes (M: ~1 week)

*Data model:* `StructSignature.kind: TypeKind` (serde default `Struct`);
`FunctionSignature.owner: Option<String>` (serde default `None`); analyzers fill them
(Rust: `enum_item`, `trait_item` queries + enclosing `impl_item` type; Python: enclosing
class name + store the already-captured superclasses; TS: kind from the existing
`@class/@interface/@type_alias` captures + enclosing class). Store
`StructSignature.supertypes: Vec<String>` while touching the same queries (cheap now, used in
Phase 4). Cache version bumps via the existing check — old caches regenerate, no migration.

*Unlocks:* `search_structs` finds enums/traits/interfaces (fixes silent empty results);
every tool output shows `Loader::load` instead of a bare `load`; `find_callers`/graph tools
disambiguate methods by owner. Optional `kind`/`owner` filters on the search tools.
*Evals:* gold cases: enum search (`ConfigError`), trait search, method-owner attribution,
same-name-different-owner disambiguation. Python/TS fixture equivalents when those fixtures
land (EVAL_PLAN P2).
*Risk:* low; additive fields. *Persistence:* version bump only.

### Phase 2 — Module graph from resolved imports (M; TS/Py first, Rust mod-tree second)

*Data model:* none persisted. New derived structure built after load/scan:
`module_graph: file → Vec<(target: FileId | External(String) | Unresolved(String), status)>`
plus reverse adjacency. Per-language resolvers per §4.

*Unlocks (new tools):*
- `find_importers { file, transitive? }` — reverse dependency closure; grep-impossible
  transitively and through aliases.
- `get_dependency_graph { scope }` — module subgraph + cycle report.
- Upgraded `get_dependencies`: resolved file paths + status instead of raw strings
  (additive fields; keep `module_path` for compat).

*Evals:* gold per language: relative-import resolution (TS `./x` probing), Python package
imports, Rust `crate::`/`super::`; an unresolvable-alias case asserting `Unresolved` (not a
guess); a synthetic cycle. Layer 2 task: "what breaks if this module's interface changes" A/B.
*Risk:* medium — path-probing edge cases; contained per language, statused when unsure.
*Incremental/persistence:* pure derived state; rebuilt on load; nothing serialized.

### Phase 3 — Resolved call graph (L: 2–3 weeks) — the actual "CodeGraph"

*Data model (in-memory only):* symbol table built per index run — `symbols: Vec<Symbol>`
(ephemeral `SymbolId = u32`; kind, name, owner, `FileId`, span) + name→ids and
(owner, method)→id maps; `call_edges: Vec<CallEdge { caller: SymbolId, callee:
CalleeRef }>` where `CalleeRef = Resolved(SymbolId) | Ambiguous(SmallVec<SymbolId>) |
External(String) | Unresolved(String)`; outgoing/incoming adjacency. This is where the
proposal's arena/edge/adjacency design lands — as an implementation detail, not a migration.
Resolution policy: free calls — same-file def ≻ imported def (Phase 2 tables) ≻ unique global
≻ Ambiguous; method calls — `self` via `owner` ≻ param-typed receiver ≻ Ambiguous over
(any-owner, method-name). Nothing deeper.

*Unlocks:* `trace_callers`/`analyze_impact` traverse resolved edges — collisions no longer
merge; responses report counts split by confidence (`resolved: 14, via_ambiguous: 3`);
`find_definition { name, from_file }` and an honest `find_references` (call sites +
import sites) become one lookup each. Existing name-keyed tools keep working via a
name→symbols layer.
*Evals:* the Phase 0 collision fixture flips from "flagged ambiguous" to "resolved via
import"; add a param-typed receiver case and a deliberately unresolvable dynamic-dispatch
case pinned in `known_failures.json` as by-design `Ambiguous`. Layer 2: re-run the original
callers/impact A/B tasks; the metric is unchanged win at higher correctness.
*Risk:* the largest phase; mitigated because tool names/shapes don't change (output gains
fields) and the resolver is three small per-language policies, each independently testable.
*Incremental/persistence:* symbol table + edges rebuilt from `TreeNode`s on every load/change
(rebuild-all first; patch-per-file later only behind a benchmark). Nothing serialized; IDs
never escape a build.

### Phase 4 — Type edges on the cheap (M: ~1 week)

*Data model:* none new — edges derived from strings already captured: `param_type`,
`return_type`, `field_type` (type-usage), Phase 1's `supertypes` (implements/extends), plus a
Rust `impl_item` Trait-for-Type query.

*Unlocks:* `find_type_usages { type }` (signature/field-level "where is `Config` used" —
precise, kind-classified), `find_implementations { trait_or_interface }`, and
interface-change impact = implements-edges composed with Phase 3's caller traversal.
*Evals:* gold: trait with 2 impls incl. one behind an alias (the case grep misses);
type-usage set for a fixture type; negative control (type name appearing only in a comment).
*Risk:* low. Type-string normalization (`Option<Config>`, `&Config`, `Vec<Config>`) is a
contained matcher; over-matching is statused, not guessed.

### Phase 5 — Deliberately parked

Body-level reference extraction (field access, constructors-as-expressions, trait bounds at
use sites), receiver inference beyond §3.3, precomputed reachability, watch-mode delta
patching of adjacency. Each needs eval evidence of demand before a line is written.

---

## 8. What we deliberately do NOT build

Recorded so future contributors don't relitigate the compiler:

1. **No type inference / dataflow.** Receiver resolution stops at `self` + declared parameter
   types. `let x = make_thing(); x.run()` → `Ambiguous`. rust-analyzer exists.
2. **No scope/shadowing model.** Import edges + status subsume the useful subset (§3.8).
3. **No cross-dependency resolution.** `std`, crates.io, site-packages, `node_modules` are
   `External` — terminal, never traversed.
4. **No dynamic-dispatch resolution.** Trait objects, generics, Python duck typing, TS
   unions → `Ambiguous` with candidates, by design, pinned in `known_failures.json`.
5. **No persisted derived graph / reachability caches.** Everything derived rebuilds from
   `TreeNode`s (§5). Revisit only with a profile showing rebuild cost dominating.
6. **No macro/decorator expansion.** Macro-generated calls are invisible; documented as a
   known limitation in tool descriptions rather than half-solved.
7. **No unified "resolver" abstraction.** Three per-language policies behind one result enum.
8. **The stance, in one line:** *heuristic resolution with explicit uncertainty* — loregrep
   may say "I'm not sure (here are the candidates)"; it must never silently guess. The A/B
   evidence says correctness earns adoption; a wrong-but-confident edge costs more than an
   honest `Ambiguous`.

---

## 9. Open decisions (recommended defaults)

| Decision | Recommendation | Why |
| --- | --- | --- |
| Symbol ID durability | Ephemeral per-build `u32`; never serialized | Persistence rebuilds everything (`persistence.rs:144-151`); `(path,name,line)` keys churn under edits |
| Where resolution lives | New `src/storage/graph.rs` (or `src/graph/`) consuming `&[TreeNode]` post-add; analyzers stay single-file | Cross-file resolution can't live in per-file analyzers; keeps `LanguageAnalyzer` trait untouched for the language-addition path (`docs/adding-a-language.md`) |
| Ambiguity in tool JSON | Every edge-bearing response carries `resolution` + candidates capped at ~5 + total count | Bounded tokens; agents act on candidates |
| Fate of `call_graph: HashMap<String, Vec<CallSite>>` | Keep through Phase 2 (bug-fixed); from Phase 3 it's a derived compatibility view over the edge list | `find_callers` schema is in shipped gold cases |
| `StructSignature` rename to `TypeSignature` | Don't rename; add `kind` | Serde/API compat across Python bindings (`python/loregrep/`) and gold cases |
| Rust mod-tree scope in Phase 2 | `mod` declarations + `mod.rs`/`name.rs` only; `pub use` re-exports → `Unresolved` in v1 | Re-export chains are the long tail; statused honesty beats delay |
| tsconfig path aliases | v1: `Unresolved(specifier)` | Common but config-parsing heavy; the status makes the gap visible in evals, which then justify (or don't) v2 |
| Landing `graph-tools` | Rebase onto main before anything else | Branch predates a88d789; merging as-is reverts the language-filter and double-count fixes its own gold cases still mark as known failures |
| Phase 1 cache handling | Serde defaults + rely on the existing version-mismatch regeneration | No migration code for a cache that rebuilds in seconds |
