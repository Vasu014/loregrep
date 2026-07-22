# Corpus golden policy — definitions

This is the decision record for `evals/corpus/scripts/scip_to_golden.py`. It describes what
that converter **actually does** today, not what a golden could ideally contain. Every
inclusion, exclusion and abstention below is implemented and covered by
`evals/corpus/scripts/test_scip_to_golden.py`.

Scope: **definitions only**, per `evals/EVAL_PLAN.md` §4b.2. Edge (call/import) goldens are
deliberately deferred to P3-7. Languages implemented: **Rust** (rust-analyzer SCIP, §§1–9),
**Python** (scip-python, §10) and **TypeScript** (scip-typescript, §11). Each has its own
converter function; `convert()` only dispatches, so a TypeScript change cannot regress Rust.
Any other `--language` is refused rather than pretending one descriptor grammar generalises
to another.

Contract: output validates against `evals/corpus/golden.schema.json` (`schema_version: 1`).

Sections 1–9 describe the **Rust** path. Section 10 describes **Python** and section 11
**TypeScript**; each states, point by point, where it agrees with Rust and where it cannot.

## 1. What counts as a symbol (Rust)

A SCIP occurrence enters the golden iff **all** of the following hold:

1. Its `symbol_roles` has bit 1 (`Definition`) set. Reference occurrences are dropped
   silently — they are not this golden's subject.
2. Its symbol is **not** `local N`. Rust locals in this index are only variables,
   parameters, `self` receivers and type parameters; none has a neutral kind. This also
   means **nested items are not currently emitted** (`include_nested_functions: false`) —
   rust-analyzer gives a function defined inside a function body a `local` symbol, so an
   inner `fn` is indistinguishable from a `let` at this layer.
3. Its package is an **in-repo** package (see §5).
4. It is not the per-crate-target `crate/` pseudo-symbol (see §6).
5. It is not macro-expansion-generated (see §6).
6. Its kind maps to a neutral kind in §2.

## 2. Kind mapping

The neutral vocabulary is fixed by the schema:
`function, method, class, struct, enum, interface, trait, type_alias, namespace`.

Kind is decided from the **rendered signature keyword** first
(`SymbolInformation.signature_documentation.text`, visibility and `async`/`unsafe`/`const
fn`/`extern "C"`/`default` modifiers stripped), because that text is a direct rendering of
the source item and is stable across SCIP proto revisions. The numeric
`SymbolInformation.kind` is a **secondary** signal: its wire values are assigned out of
alphabetical order in `scip.proto` and entries have been added/renamed between releases, so
a hard-coded table can silently drift after an indexer bump. When both signals are present
and disagree, the signature wins and the disagreement is counted in `--stats` (over the
loregrep sample: **zero disagreements**). If neither is available, a `().` descriptor suffix
yields `function`/`method` and a `/` suffix yields `namespace`; a bare `#` (type) descriptor
with no information at all is **dropped**, because struct-vs-enum-vs-trait cannot be
recovered from the descriptor and guessing is worse than a miss.

| Rust source form | SCIP kind (name / observed number) | Descriptor suffix | Neutral kind |
| --- | --- | --- | --- |
| free `fn` | `Function` (17) | `foo().` | `function` |
| `fn` in an inherent `impl` | `Method` (26) | `impl#[T]foo().` | `method`, `owner = T` |
| `fn` in a trait `impl` | `Method` (26) | `impl#[T][Tr]foo().` | `method`, `owner = T` |
| associated `fn` (no `self`) | `StaticMethod` (80) | `impl#[T]foo().` | `method`, `owner = T` |
| `fn` declared in a `trait` | `TraitMethod` (70) | `Tr#foo().` | `method`, `owner = Tr` |
| `struct` / `union` | `Struct` (49) | `T#` | `struct` |
| `enum` | `Enum` (11) | `E#` | `enum` |
| `trait` | `Trait` (53) | `Tr#` | `trait` |
| `type X = ...` | `Type` (55) | `X#` | `type_alias` |
| `mod` (inline or file) | `Module` (29) | `m/` | `namespace` |
| struct/enum field | `Field` (15) | `T#f.` | *dropped* |
| enum variant | `EnumMember` (12) | `E#V#` | *dropped* |
| `const` / `static` | `Constant` (8) | `C.` | *dropped* |
| local var / param / `self` / type param | 61 / 37 / 44 / 58 | `local N` | *dropped* |
| `impl` block itself | `Type` (55), no `display_name` | `impl#[T][Tr]` | *dropped* (not an identifier) |
| `macro_rules!` | `Macro` | `m:` | *dropped* (no neutral kind) |

`class` / `interface` exist in the mapping table for the non-Rust converters; no Rust
construct produces them. Python produces `class` (§10.2); TypeScript produces `class`,
`interface`, `type_alias` and `enum`, and is the only language that uses all four (§11.2).

Fields, variants, constants and macros are dropped because the schema's `kind` enum has no
member that fits them. That is a **schema** decision, not a claim that they are
uninteresting; if the scorer ever wants them, the schema changes first and this file with it.

## 3. Names

`name` is the bare identifier as written at the definition site — never the mangled SCIP
symbol string. It is taken from `SymbolInformation.display_name` when present (it always is,
in the loregrep sample, except for the pseudo-symbols of §6), and otherwise from the trailing
descriptor component of the symbol string.

`owner` is the enclosing **type**, and is set only for methods:

- `mod/path/Type#member().` → `Type` (a member declared inside a `trait`).
- `mod/path/impl#[Type]member().` → `Type`.
- `mod/path/impl#[Type][Trait]member().` → `Type`, i.e. the **implementing type**, not the
  trait. A trait impl's members belong to the type in the sense a reader means by "owner".
- A method whose descriptor chain yields no type is reclassified as `function` with
  `owner: null`, so `kind == "method"` always implies a non-null owner.

Generic and lifetime-carrying impl targets are backtick-escaped by rust-analyzer
(`` impl#[`FileSet<'a>`] ``); the escaping and the type arguments are stripped, so the owner
is `FileSet` — the identifier a user would search for.

## 4. Spans, visibility, flags

**Spans.** `start_line` / `end_line` are 1-indexed and inclusive. They come from the
occurrence's `enclosing_range` (the whole definition *including its body*), falling back to
`range` (the identifier alone) when absent. Both SCIP encodings are handled:
`[line, startCol, endCol]` and `[startLine, startCol, endLine, endCol]`. SCIP ranges are
half-open, so an end column of 0 on a later line means the span stops at the *start* of that
line and the last covered line is `endLine - 1`; that is corrected (a whole 543-line file
module is `1..543`, not `1..544`).

Consequence worth knowing before triaging a span diff: rust-analyzer's `enclosing_range`
starts at the item's **leading attributes and doc comments**, not at its signature line.
`TypeKind` in `src/types/struct_def.rs` is `3..21` — line 3 is the first `///` line, line 9 is
`pub enum TypeKind`. A `#[test] fn` starts at the `#[test]` line. This is intentional (it is
the "definition with body" span the schema asks for) and any scorer comparing spans must be
tolerant at the top edge or normalise both sides the same way.

**Visibility.** Emitted as `public` / `private` / `unknown`, derived only from the visibility
modifier rust-analyzer renders into the signature. The converter **abstains** (`unknown`)
rather than guessing whenever:

- there is no signature text at all;
- the modifier is restricted — `pub(crate)`, `pub(super)`, `pub(in path)`. The schema has no
  vocabulary for "public within the crate", and collapsing it to either `public` or `private`
  would be an invention;
- the symbol is a member of a `trait` declaration or of a trait `impl`. Rust forbids a
  visibility modifier there; the effective visibility is the trait's, which SCIP does not
  carry at the member.

`private` is therefore asserted only where a definition site could have written `pub` and did
not. Over the loregrep sample: 303 public, 527 private, 81 unknown.

**Flags.** Under-populated on purpose; a wrong flag is worse than a missing one.

- `async` — set when the rendered signature is an `async fn` (after stripping visibility and
  `const`/`unsafe`). 20 in the sample.
- `test` — set when the file is under `tests/`, when the symbol's module path contains a
  segment named `tests`/`test`, or when the symbol lies inside a module span that the
  converter identified as a test module. With `--repo-root`, a module is also recognised as
  a test module when its `enclosing_range` opens with `#[cfg(test)]`, which catches
  differently-named modules such as `mod p1_1_tests`. Without `--repo-root` only the naming
  convention applies, and such modules are silently under-flagged. `benches/` and `examples/`
  are **not** flagged test — they are separate build targets, not tests.
- `generator`, `generated`, `nested`, `ambient`, `abstract` — never emitted. None is
  reliably derivable from a Rust SCIP index today.

## 5. Which packages are "in this repo"

A SCIP symbol carries `<scheme> <manager> <package> <version> <descriptors>`. Only
definitions from in-repo packages may enter the golden; a stray `... cargo std <url> ...`
definition must never inflate the inventory.

In-repo packages are those owning at least one **module or crate-root definition** inside the
indexed documents. A crate that lives in this checkout defines its own modules; a dependency
never does — the only way a foreign package acquires a definition in one of these documents
is macro expansion, which produces types and members, not modules. This admits every member
of a workspace without a hard-coded list. Packages named `std`/`core`/`alloc`/`proc_macro`/
`test`, and any whose version field is a URL, are excluded up front. `--package` overrides
the inference explicitly. In the loregrep sample the inference keeps exactly one package
(`loregrep`) and drops nothing.

## 6. Deliberate exclusions

| Category | Rule | Sample count |
| --- | --- | --- |
| references | `symbol_roles` bit 1 clear | 28601 occurrences |
| locals (vars, params, `self`, type params, and hence nested items) | symbol starts `local ` | 4145 |
| fields | kind `Field` — no neutral kind | 335 |
| enum variants | kind `EnumMember` — no neutral kind | 37 |
| crate roots | `<pkg> crate/`: one per crate target (lib, each bin, bench, example, integration test), no `display_name`, span = the whole root file. Not an identifier anyone wrote. | 11 |
| constants | kind `Constant` — no neutral kind | 2 |
| macro-generated definitions | two or more *distinct* symbols sharing one identical definition range in a document. rust-analyzer attributes expansion output to the macro call site, so a collision is the only in-band signal available. | 0 |
| external packages | §5 | 0 |
| `impl` blocks | `Type`-kinded symbol with no `display_name` and an `impl` descriptor; contributes no identifier | no definition occurrences emitted by rust-analyzer |

`include_test_symbols` is **true**: test symbols are kept and flagged, so a consumer can
filter without the golden having to be regenerated. `include_generated` is false and
`include_nested_functions` is false for Rust, as recorded in each golden's `policy` block.
(`include_nested_functions` is **true** for Python — see §10.6 for why that is a statement
about the two oracles rather than about the two languages.)

## 7. Determinism

Symbols are sorted by `(file, start_line, name)` with `(kind, owner, end_line)` as
tie-breakers, exact duplicates are collapsed, and the JSON is pretty-printed with 2-space
indent so a regeneration diff is reviewable line by line. `generated_at` is the only
non-deterministic field; set `SOURCE_DATE_EPOCH` to pin it.

## 8. Independence

Per `evals/EVAL_PLAN.md` §4b.5, nothing in this directory may use loregrep to produce or
audit a golden. This converter reads the SCIP index and (optionally, for `#[cfg(test)]`
detection only) the raw source files. It has no dependency on loregrep's parsers, index or
CLI, and no pip dependencies at all.

## 9. Regenerating

```sh
rust-analyzer scip . --output /tmp/repo.scip           # ~9s on loregrep
python3 evals/corpus/scripts/scip_to_golden.py \
    --scip /tmp/repo.scip \
    --repo <corpus-id> --commit <40-hex> --language rust \
    --repo-root . \
    --out evals/corpus/<corpus-id>/golden-symbols.json --stats
python3 evals/corpus/scripts/test_scip_to_golden.py
```

An indexer or converter version bump is a fixture bump: regenerate, diff, re-triage.

---

# 10. Python (scip-python)

Oracle: **scip-python 0.6.6** (pinned in `evals/corpus/corpus.lock`), driven by
`evals/corpus/regen_python.sh`. Reference corpus: **flask 3.1.3** at
`22d924701a6ae2e4cd01e9a15bbaf3946094af65`.

## 10.1 What the oracle actually gives us

scip-python's output shape differs from rust-analyzer's in three ways that drive every
decision below. All three were measured on the flask index, not assumed:

1. **There is no `kind` field on `SymbolInformation`.** A symbol entry carries only
   `{symbol, documentation, relationships}` — 3396 with documentation, 1747 with nothing but
   the symbol string, 184 with relationships. There is also no `display_name`.
2. **`documentation[0]` is a rendered fenced block**, e.g.
   ```` ```python\n@staticmethod\nasync def fetch() -> str:\n``` ````, or, for a module, the
   unfenced string `(module) flask.app`.
3. **`enclosing_range` opens on the first decorator line**, not on the `def`/`class` line.
   613 of flask's definitions are decorated, so this is not a corner case.

**Kind therefore comes from the symbol string's trailing descriptor** — the structural,
machine-generated part of the SCIP grammar — and the rendered documentation is a *secondary,
corroborating* signal. This is the mirror image of the Rust path, where the rendered
signature is primary and a numeric, drift-prone `kind` enum is secondary. The principle is
the same in both: **the signal that cannot silently drift wins**, and disagreements are
counted in `--stats` rather than resolved in silence. Over flask: **zero disagreements**
(1481 kept symbols).

## 10.2 Kind mapping

| Python source form | Descriptor suffix | Rendered `documentation[0]` | Neutral kind |
| --- | --- | --- | --- |
| module-level `def` | `foo().` | ` ```python def foo(` | `function` |
| `def` in a `class` body | `Cls#foo().` | ` ```python def foo(` | `method`, `owner = Cls` |
| `async def` | `foo().` / `Cls#foo().` | ` ```python async def foo(` | as above, `+ async` flag |
| `def` inside a `def` | `outer().inner().` | ` ```python def inner(` | `function`, `owner = null`, `+ nested` |
| `class` (any base, incl. `Enum`, `Protocol`, `TypedDict`) | `Cls#` | ` ```python class Cls(Base):` | `class` |
| `class` inside a `def` | `outer().Cls#` | ` ```python class Cls:` | `class`, `+ nested` |
| the module itself | `` `flask.app`/__init__: `` | `(module) flask.app` | `namespace` → **excluded**, §10.4 |
| module-level binding, `TypeVar`, type alias, `X: T = ...` | `X.` | `(variable) …` / `(type alias) …` / `undefined = t.TypeVar(…)` | *dropped* |
| class attribute / annotated slot | `Cls#attr.` | `(variable) attr: T` | *dropped* |
| parameter | `foo().(name)` | the parameter's docstring line, or absent | *dropped* |
| comprehension var, local, import alias | `local N` | — | *dropped* |

`:` is SCIP's **meta** descriptor; `parse_descriptors` labels it `"macro"` because Rust's
`macro_rules!` uses the same suffix character. Python emits it for exactly one thing — the
module — so the label is unambiguous on this path.

Every `class` statement maps to `class`. `enum` / `interface` / `struct` / `trait` /
`type_alias` are **never emitted for Python**: `class Colour(Enum)` and `class P(Protocol)`
are class statements, and loregrep's Python analyzer likewise assigns `TypeKind::Class` to
every `class_definition`. Inventing an `enum` kind from a base-class name would be a guess on
the oracle side that the parser could never match. Python's `X = t.Union[...]` type aliases
are bindings (`X.`), not declarations, and are dropped with every other binding — the
schema's `type_alias` kind is reserved for languages with a declaration form.

## 10.3 Names, owners, nesting

`name` is the trailing descriptor's identifier. scip-python has **no `display_name`**, so
unlike the Rust path there is no preferred alternative source; the descriptor is all there is.
The one special case is the module pseudo-symbol, whose trailing descriptor is the literal
`__init__`: its name is the last segment of the dotted module path (`flask.app` → `app`).

`owner` is set **only for callables** and only from the **immediately enclosing** descriptor:

- `Cls#foo().` → `Cls`.
- `outer().Inner#run().` → `Inner` (the nested class, not the function).
- `Cls#go().poll().` → **`null`**. A `def` nested inside a method is a closure bound to the
  function's local scope; it is not reachable as `Cls.poll`, and claiming it as a method
  would invent an attribute that does not exist. (The Rust path walks further out because
  Rust has no such nesting to confuse it.)
- A nested *class* gets `owner: null`, exactly as a top-level class does — the schema defines
  `owner` as "enclosing type for a method".

**`nested` flag.** Set when any *callable* descriptor encloses the definition. 608 of flask's
1481 symbols carry it. It is **under-populated in one known way**: scip-python attributes a
`def` nested inside a *method* to the enclosing class (`Blueprint#decorator().`, not
`Blueprint#route().decorator().`), so 44 flask definitions are nested in the source but carry
`owner: <class>` and no `nested` flag. That is the oracle's model, and loregrep's Python
analyzer independently reaches the same answer (it walks to the nearest enclosing
`class_definition`), so the two agree; the flag is documented as descriptor-derived rather
than source-derived, and no attempt is made to "correct" it by reading the source.

## 10.4 Modules are excluded, exactly as Rust `mod` is

Every indexed file yields one `<module.path>/__init__:` pseudo-symbol with `range [0,0,0]` and
no `enclosing_range`, i.e. a 1..1 span at the top of the file. It maps to `namespace` and is
then removed by `policy.excluded_kinds: ["namespace"]`, for the same reason as Rust `mod`:
**loregrep's Python analyzer emits nothing module-shaped.** `PythonAnalyzer::extract_structs`
queries `class_definition` only, `extract_functions` queries `function_definition` only, and
there is no Python equivalent of `TreeNode.declared_modules`. A module is therefore outside
the definition-parity contract rather than a recall miss. 65 dropped in flask.

Unlike the Rust path, nothing depends on module symbols before they are dropped: Python's
`test` flag is path-derived (§10.5), not propagated down from a `#[cfg(test)] mod` span, so
the exclusion happens inline rather than in a second pass.

## 10.5 Spans, visibility, flags

**Spans — identical convention to Rust (§4), and load-bearing here.** `start_line` is the
**declaration** line, taken from the symbol's own name occurrence; `end_line` comes from
`enclosing_range`. Both 1-indexed inclusive. scip-python's `enclosing_range` opens on the
first **decorator**, so using its start would report `@app.route(...)` as the start of a view
function — a line no syntax-level parser calls part of the declaration, and one that loregrep
(whose span is the tree-sitter `function_definition` node, excluding the `decorated_definition`
wrapper) will never produce. Pinned by
`test_decorated_declaration_line_is_the_def_not_the_decorator`. Against flask this convention
produces **span_exact_rate 1.00 over 1481 matched symbols**.

`range` is always 3 ints here (an identifier is always on one line); `enclosing_range` is
always 4. The half-open end-column correction of §4 applies unchanged.

**Visibility** is the PEP 8 **naming convention**, not a language construct — Python has no
visibility modifier for the oracle to report, so unlike Rust there is nothing to abstain
*from*:

- leading `_` or `__` → `private`;
- dunder (`__init__`, `__call__`) → `public`; these are protocol members, not private ones;
- otherwise → `public`.

Over flask: 1438 public, 43 private, 0 unknown. This is recorded as a convention on purpose:
the scorer does **not** score visibility, so this field is documentation for a human triager,
and it happens to match loregrep's own rule (`is_public = !name.starts_with('_')`).

**Flags.**

- `async` — the rendered declaration (after skipping decorator lines) begins `async def`. 22
  in flask.
- `nested` — §10.3. 608 in flask.
- `test` — path-derived: any `tests`/`test` directory component, or a basename matching
  `test_*.py`, `*_test.py`, `tests.py`, `conftest.py`. 1083 in flask.
  `include_test_symbols` stays **true**, as for Rust: test symbols are kept and flagged so a
  consumer can filter without regenerating.
- **`generated` — never emitted, and there is nothing to detect.** The Rust path infers
  macro-generated definitions from two distinct symbols colliding on one range, because
  rust-analyzer expands macros and attributes the results to the call site. **Python has no
  macro expansion**: every definition scip-python reports corresponds to `def`/`class` text a
  syntax-level parser can see. Inventing a mechanism here would be inventing a defect class.
  `include_generated` is recorded as `false` in the policy block for schema uniformity; it
  excludes nothing because nothing is flagged.
- `generator`, `ambient`, `abstract` — never emitted, as on the Rust path.

## 10.6 Nested functions are IN for Python

`policy.include_nested_functions` is **`true`** for Python and `false` for Rust. The languages
did not change; **the oracles differ**. rust-analyzer gives an inner `fn` a `local` symbol, so
a Rust golden structurally *cannot* contain one. scip-python gives an inner `def` a fully
qualified symbol (`stream_with_context().generator().`), so a Python golden contains 464 of
them and it would be a lie to declare otherwise.

The declaration is also what the scorer needs. `corpus_score.py` consults
`include_nested_functions` in exactly one place: when it is `false`, `nested_symbol_ids()`
derives nesting from emitted spans and moves loregrep's inner functions into an `excluded`
bucket. Declaring `false` while the golden still *contained* 464 nested definitions would
strip loregrep's matches out of the precision denominator while the recall denominator kept
demanding them — manufacturing false negatives out of a policy field. `true` is both the
honest description and the one that scores correctly.

## 10.7 Which packages are "in this repo"

A scip-python symbol is `scip-python python <PackageName> <Version> <descriptors>`. First-party
packages are those owning at least one **module** definition (`<module.path>/__init__:`) in the
indexed documents: a module pseudo-symbol is *defined* only in the file that is that module, so
owning one is exactly "this package's source is in this checkout". `python-stdlib` is excluded
up front regardless, because an index built with typeshed sources on the search path would
otherwise clear that bar. In the flask index no external package owns a definition of any kind
— `Werkzeug`, `click` and `python-stdlib` appear only as reference occurrences. `--package`
overrides the inference explicitly.

**Package-name casing is not stable** in scip-python output: flask's own definitions are
stamped `flask 3.1.3`, but ~108 occurrences say `Flask 3.1.3`. First-party membership is
therefore decided case-insensitively. Over flask the inference keeps exactly one package
(`flask`, 1481 symbols) and drops nothing that has a definition.

## 10.8 Path exclusions for flask

`policy.excluded_paths: ["docs", "examples"]` (206 definitions in 18 documents):

- `docs/conf.py` — Sphinx build configuration. Not library code and not a test.
- `examples/` — three self-contained tutorial applications (`celery`, `javascript`,
  `tutorial`), each with its own `pyproject.toml`. They are separate distributions vendored
  into the repo for documentation; scip-python only stamps them with the `flask` package
  because it indexed the whole worktree from one root.

`tests/` is **kept** and flagged, matching the Rust path. It is ~700 real test functions and
class-based views, and it is the part of flask that exercises nested classes hardest.

The scorer applies `excluded_paths` **symmetrically**, so loregrep's 66 emitted symbols under
those trees are removed from the precision denominator and counted in their own bucket rather
than charged as false positives.

## 10.9 Known oracle artifacts (read this before triaging a Python false positive)

**scip-python emits one definition occurrence per unique symbol string.** When one scope
contains several definitions of the *same name*, they collapse into a single symbol and only
the **first** definition occurrence survives. In flask this affects, exhaustively:

| Shape | Example | Golden has | Source has |
| --- | --- | --- | --- |
| `@t.overload` chains | `locate_app` `src/flask/cli.py:230,236,241` | 1 | 3 |
| `@property` + `@x.setter` | `max_content_length` `src/flask/wrappers.py:60,89` | 1 | 2 |
| same name redefined in `if`/`else` | `view` `src/flask/views.py:106,115` | 1 | 2 |
| same name redefined in a straight line | `class Module` `tests/test_cli.py` (12 occurrences) | 1 | 12 |
| a `def` shadowing a same-named local variable | `app` `src/flask/cli.py:963` (assignment), `:971` (`def`) | 0 | 1 |

**pyright's reachability analysis prunes `if not t.TYPE_CHECKING:` blocks.** `__getattr__` at
`src/flask/__init__.py:47` lives in such a block and has no symbol in the index at all, so
`src/flask/__init__.py` contributes **zero** golden symbols.

Both are properties of the oracle, not defects in a parser under test. A syntax-level
extractor is *right* to report all of these, so they surface as false positives with no
corresponding golden entry. They are deliberately **not** compensated for in the converter:
fabricating golden entries the oracle never emitted would make the golden unverifiable
against its own index.

## 10.10 Reference numbers (flask 3.1.3)

1481 symbols — 922 `function`, 418 `method`, 141 `class` — across 49 files.
Dropped: 1807 parameters, 972 `local`, 310 bindings (variables / attributes / TypeVars /
type aliases), 206 definitions under excluded paths, 65 modules. Zero kind-signal
disagreements, zero unparseable symbols, zero external-package definitions.

## 10.11 Regenerating

```sh
bash evals/corpus/regen_python.sh                      # venv + pip install -e + scip-python, ~10s
python3 evals/corpus/scripts/scip_to_golden.py \
    --scip evals/corpus/flask/index.scip \
    --repo flask --commit 22d924701a6ae2e4cd01e9a15bbaf3946094af65 \
    --language python --exclude docs --exclude examples \
    --repo-root evals/corpus/flask/src \
    --out evals/corpus/flask/golden-symbols.json --stats
python3 evals/corpus/scripts/test_scip_to_golden.py
```

`--repo-root` is accepted but unused on the Python path (it exists for Rust's `#[cfg(test)]`
detection); the Python converter reads no source files at all. An indexer or converter version
bump is a fixture bump: regenerate, diff, re-triage.

---

# 11. TypeScript (scip-typescript)

Oracle: **scip-typescript 0.4.0** (pinned in `evals/corpus/corpus.lock`), driven by
`evals/corpus/regen_typescript.sh`. Reference corpus: **hono v4.12.31** at
`cadff88bba34153646c9b35f24d7cc0cb61be913` — 512 indexed documents (326 distinct paths; the
root `tsconfig.json` is a project-references file and several files belong to more than one
project), of which 23 are `.tsx`, 34446 definition occurrences, ~8s to index.

## 11.1 What the oracle actually gives us

Measured on the hono index, not assumed:

1. **No `kind` and no `display_name` on `SymbolInformation`** — as with scip-python, an entry
   carries only `{symbol, documentation, relationships}` (34446 with documentation, 166 with
   relationships).
2. **`documentation[0]` is a fenced ```` ```ts ```` block** rendering the declaration as
   TypeScript's own quick-info would: `type InfoStatusCode`, `interface Router`,
   `class HTTPException`, `enum AlgorithmTypes`, `function parseFormData<T extends BodyData>(…)`,
   `(method) getResponse(): Response`, `constructor(…): HTTPException`, `get url: string`,
   `module "http-status.ts"`, `var splitPath: (path: string) => string[]`,
   `(property) res: Response | undefined`, `(enum member) HS256 = HS256`.
3. **`enclosing_range` starts at the declaration, not at the JSDoc** (unlike Python's
   decorator problem), but it is **absent** for accessors (0 of 28), for `namespace`
   declarations (0 of 22) and for 58 signature-only class/interface members.
4. **One module pseudo-symbol per FILE**, spelled exactly like a real `namespace X {}` symbol
   apart from its trailing name (§11.4).
5. **Everything inside a function body is `local N`** — 22463 of the 34446 definition
   occurrences. There are no `f().g().` chains for real declarations; the only non-local
   symbols with a callable in the middle are 44 parameter/type-literal descriptors.

## 11.2 Kind signals: the rendered head is PRIMARY, the descriptor is secondary

Rust makes the rendered signature keyword primary (§2); Python makes the descriptor primary
(§10.1). TypeScript follows **Rust**, and the reason is forced rather than stylistic: a `#`
(type) descriptor covers `class`, `interface`, `type X = …` **and** `enum` alike, and
TypeScript is the one corpus language whose neutral kinds must tell those four apart — it is
where the mapping earns its keep, and loregrep emits all four
(`TypeKind::{Class,AbstractClass,Interface,TypeAlias,Enum}` in `src/analyzers/typescript.rs`).
The descriptor simply cannot express the distinction, so it cannot be primary.

The descriptor is still load-bearing as the **secondary** signal. It supplies the coarse
class the rendered head must land in —

| Descriptor suffix | Coarse class | Rendered heads that belong to it |
| --- | --- | --- |
| `X#` | type-ish | `type` / `interface` / `class` / `enum` |
| `f().` | callable | `function` / `(method)` / `constructor` / `get`/`set` |
| `x.` | binding | `var x: T` / `(property)` / `(enum member)` |
| `N/` | namespace | `module "f.ts"` / `N: any` |
| `t0:` (meta), `(p)`, `[T]` | *none* | dropped outright |

— and it decides function-vs-method, via the owner chain, where the head is silent. **When a
type-ish descriptor's head says nothing at all the symbol is DROPPED, not guessed**, exactly
as Rust drops an uninformative `#` descriptor (§2). Over hono this happens 0 times.

Disagreements are counted in `--stats`, never silently resolved. Over hono:

- descriptor-class vs head-class: **0** disagreements across all 11983 first-party
  definition occurrences.
- The one place the two available signals genuinely conflict is module-level `var` bindings,
  because **TypeScript renders every module-level binding as `var name: <type>`** — the head
  is kind-silent by construction. A binding is a *definition* only when its initializer is a
  function-like expression, and that is exactly when scip-typescript attaches an
  `enclosing_range`. So the structural signal decides and the rendered *type* corroborates.
  107 of 1273 bindings disagree, and the structural signal is right in **both** directions:

  | Disagreement | Count | Reality |
  | --- | --- | --- |
  | has `enclosing_range`, type is not a function type | 95 | arrow annotated with a named alias — `const notFoundHandler: NotFoundHandler = (c) => {…}` (`src/hono-base.ts:31`). A real function; loregrep emits it. |
  | no `enclosing_range`, type IS a function type | 12 | re-export binding — `export const verify = Jwt.verify` (`src/middleware/jwt/jwt.ts:176`), `export const decodeURIComponent_ = decodeURIComponent` (`src/utils/url.ts:319`). Not a definition; loregrep does not emit it. |

## 11.3 Kind mapping

| TypeScript source form | Descriptor suffix | Rendered `documentation[0]` | Neutral kind |
| --- | --- | --- | --- |
| `type X = …` | `X#` | `type X` | `type_alias` (385) |
| `interface X {}` | `X#` | `interface X` | `interface` (196) |
| `class X {}` / `abstract class X {}` | `X#` | `class X` | `class` (52) |
| `enum X {}` / `const enum X {}` | `X#` | `enum X` | `enum` (3) |
| `function f() {}` (incl. `async`, generators) | `f().` | `function f(…)` | `function` |
| `const f = (…) => {}` at module level | `f.` + `enclosing_range` | `var f: (…) => T` | `function` |
| class / interface member | `X#m().` | `(method) m(…)` | `method`, `owner = X` |
| `constructor(…)` | `` X#`<constructor>`(). `` | `constructor(…): X` | `method` `constructor`, `owner = X` |
| `get p()` / `set p(v)` | `` X#`<get>p`(). `` | `get p: T` | `method` `p`, `owner = X` |
| `namespace X {}` / `declare namespace X {}` | `X/` | `X: any` | `namespace` (4) |
| the file itself | `` <dir>/`<f.ts>`/ `` | `module "f.ts"` | *dropped* — §11.4 |
| `declare module '…' {}` | `` <dir>/`<f.ts>`/`'../..'`/ `` | `'../..': typeof import(…)` | *dropped* — §11.4 |
| module-level `const`/`let`/`var` | `x.`, no `enclosing_range` | `var x: T` | *dropped* |
| class field, interface/type-literal property | `X#p.`, `X#typeLiteral0:p.` | `(property) p: T` | *dropped* |
| enum member | `E#V.` | `(enum member) V = V` | *dropped* |
| parameter, type parameter | `f().(p)`, `X#[T]` | `(parameter) …` | *dropped* |
| object / type-literal scope | `name0:`, `X#typeLiteral0:` | `(property) …` | *dropped* (SCIP meta descriptor) |
| anything inside a function body | `local N` | — | *dropped* |

`struct` and `trait` are never emitted for TypeScript: the language has no such declaration
form, and loregrep's TypeScript analyzer never produces them either. An `abstract class` maps
to `class`, matching the scorer's `TYPE_KIND_MAP` (`abstract_class` → `class`); the oracle
renders it as a plain `class X` and cannot be asked for more.

## 11.4 Modules: TypeScript is the exception to `excluded_kinds: ["namespace"]`

Rust and Python both set `policy.excluded_kinds: ["namespace"]` because loregrep models a
module as a property of a file, never as a searchable symbol. **TypeScript does not**, and
`policy.excluded_kinds` is `[]` here. `TypeScript::extract_structs` queries
`(internal_module name: (identifier))` and `(module name: (identifier))` and assigns
`TypeKind::Namespace`, so `namespace X {}` / `module X {}` *is* part of the parity contract.

That makes it essential to separate two things scip-typescript spells almost identically:

- **The per-file module pseudo-symbol** — `` src/utils/`http-status.ts`/ ``, range `[0,0,0]`,
  head `module "http-status.ts"`. There is one per document and it is **not a declaration
  anyone wrote**. It is dropped **structurally**, by the rule *the trailing namespace
  descriptor equals the document's basename* — never by kind, because dropping it by kind
  would take the real namespaces with it. **510 dropped** in hono.
- **A real `namespace` block** — `` src/jsx/`base.ts`/JSX/ ``, head `JSX: any`. **4 kept**:
  `JSX` (`src/jsx/base.ts:36`, `src/jsx/intrinsic-elements.ts:14`), `Deno`
  (`src/adapter/deno/deno.d.ts:1`), `global` (`runtime-tests/fastly/index.test.ts:7`).

Getting this backwards costs either ~510 fabricated false negatives (excluding nothing) or
4 hidden real ones (excluding the kind). Pinned by
`test_file_module_is_dropped_but_a_real_namespace_is_kept`.

A third shape shares the `N/` suffix and is dropped separately: **`declare module '../..' {}`**
(15 occurrences, 6 distinct). Its trailing descriptor is a *module specifier string*
(`` `'../..'` ``), not an identifier; tree-sitter parses it as a `module` node whose name is a
`string`, so loregrep's `(identifier)`-constrained query structurally cannot match it and
neither should the golden. The rule is "the trailing namespace descriptor must be a JS
identifier". Declarations *inside* such a block are kept normally (its descriptor is just
another namespace in the path) — e.g. `ContextRenderer` at
`runtime-tests/bun/index.test.tsx:17`.

## 11.5 Names, owners, spans, visibility, flags

**Names.** scip-typescript has no `display_name`, so the name is the trailing descriptor,
with one normalisation: `<constructor>` → `constructor`, `<get>url` / `<set>url` → `url`
(69 constructors, 26 getters, 2 setters in hono). Those angle-bracketed spellings are SCIP's
role markers, not identifiers anyone writes or searches for, and loregrep names the same
nodes `constructor` / `url`. Verified mechanically: **all 1343 emitted names occur verbatim
on their own `start_line` in the source** (`constructor` excepted, where the source token is
the same word).

**Owner** is the nearest enclosing `type` descriptor, set only for callables. Unlike Python,
interfaces own their members exactly as classes do (`Router#add().` → `owner = Router`);
`method` therefore always implies a non-null owner, and 198 of the 1343 symbols carry one.

**Spans** follow §4/§10.5 unchanged: `start_line` is the DECLARATION line from the symbol's
own name occurrence, `end_line` from `enclosing_range`, 1-indexed inclusive, with the
half-open end-column correction. This matches loregrep exactly for arrow-bound functions,
whose span is the tree-sitter `variable_declarator` — `export const splitPath = (path: string)`
at `src/utils/url.ts:8` is `8..14` on both sides. Against hono the convention yields
**span_exact_rate 0.97 over 1181 matched symbols**; every miss is one of the two artifacts in
§11.9.

**Visibility is always `unknown`.** This is an abstention, not an oversight. scip-typescript
renders neither `export` nor `private`/`protected`: over hono's 34446 definition occurrences,
zero rendered heads contain `private`, `protected`, `public`, `abstract` or `async`. Rust's
rule applies (§4) — the converter never asserts a visibility the oracle did not carry. The
scorer does not score visibility, so this costs nothing measurable; it is recorded here so a
triager does not read `unknown` as a bug.

**Flags.**

- `test` — path-derived: a `test`/`tests`/`__tests__`/`runtime-tests`/`spec`/`specs`
  directory component, or a basename matching `*.{test,spec}.{ts,tsx,js,jsx}`. 79 in hono.
  `include_test_symbols` stays **true**.
- `ambient` — the file is a `.d.ts`. 9 in hono (all of `src/adapter/deno/deno.d.ts`). Purely
  path-derived and therefore certain.
- `async` — **never emitted, and not derivable.** TypeScript's quick-info renders
  `async function f()` as `function f(…): Promise<T>`; a `Promise` return type is not proof of
  an `async` keyword (hand-written `Promise`-returning functions are everywhere in hono). The
  Rust and Python paths could read `async` off the rendered declaration; this one cannot, so
  it abstains rather than guessing.
- `generated` — **never emitted, and there is nothing to detect.** The Rust path infers
  macro-expansion artifacts from two distinct symbols colliding on one range (§6). TypeScript
  has no macro expansion and no analogous defect class: every definition scip-typescript
  reports corresponds to source text a syntax-level parser can see. Inventing a mechanism
  here would be inventing a defect class. `include_generated` is recorded `false` for schema
  uniformity and excludes nothing on the golden side.
- `nested` — never emitted; §11.6 explains why there is nothing to flag.
- `generator`, `abstract` — never emitted, as on the other paths.

## 11.6 Nested functions are OUT for TypeScript

`policy.include_nested_functions` is **`false`**, matching Rust and *not* Python. Again the
oracles differ, not the languages: scip-typescript gives every definition inside a function
body a `local N` symbol (22463 of hono's 34446 definition occurrences), so a TypeScript golden
**structurally cannot contain a nested function** — declaring `true` would be a lie about the
contents.

`false` is also what the scorer needs. loregrep's TypeScript analyzer surfaces arrow functions
and function expressions only when bound at MODULE TOP LEVEL (`is_top_level_arrow_decl`), but
its `(function_declaration …)` query has no such restriction, so it *does* emit `function`
declarations nested inside other bodies. With `include_nested_functions: false` the scorer's
`nested_symbol_ids()` moves those into a counted `excluded` bucket (13 over hono) instead of
charging them as false positives — which is right, because the golden never demanded them.
No false negatives are manufactured either way, because the golden contains none.

## 11.7 Which packages are "in this repo"

A scip-typescript symbol is `scip-typescript npm <package> <version> <descriptors>`. An npm
dependency is an ordinary symbol under a different package name, so first-party membership is
decided the same way as elsewhere: **a package is first-party iff it owns at least one
per-file module pseudo-symbol** among the indexed documents. Over hono the inference keeps
exactly one package (`hono`, 2607 pre-dedup definitions) and drops nothing — no node_modules
document is indexed at all, because the root `tsconfig.json` names only first-party projects.
`--package` overrides the inference. Unlike Python, package-name casing is stable here, so
matching is case-sensitive.

## 11.8 Path exclusions for hono

`policy.excluded_paths`:
`["benchmarks", "build", "perf-measures", "runtime-tests/deno", "runtime-tests/deno-jsx", "vitest.config.ts"]`
— 2 indexed documents / 16 definition occurrences on the golden side, 33 loregrep symbols on
the emitted side.

The list is derived from a measured fact, not taste: hono has 356 `.ts`/`.tsx` files outside
`node_modules` and the oracle indexed 326 of them. The 31 it did not are **exactly** the ones
above (30 files, plus `src/middleware/jwk/keys.test.json` on the other side of the ledger),
because the root `tsconfig.json` is a project-references file and these trees are not among
its references. Excluding them keeps the two sides symmetric instead of charging loregrep for
files the oracle was never asked to see.

- `benchmarks/` (18 files) — self-contained benchmark apps with their own `package.json`,
  the direct analogue of flask's `examples/` (§10.8).
- `build/` (5 files) — the library's own build/validation tooling, not library code. Named
  explicitly rather than relying on the scorer's `looks_like_generated` heuristic, which would
  bucket it as "generated" purely because the directory is called `build`.
- `perf-measures/` (4 files, 2 of them indexed) — a performance-measurement harness. The whole
  tree is excluded rather than the two unindexed sub-paths, because "this is not library code"
  is a cleaner rule than an enumerated file list; the cost is 16 definitions.
- `runtime-tests/deno`, `runtime-tests/deno-jsx` (5 files) — Deno-toolchain tests. Every other
  `runtime-tests/*` directory has a `tsconfig.json` referenced from the root; these do not,
  because Deno does not use one.
- `vitest.config.ts` — the root vitest config, in no project.

`docs/` needs no exclusion: it contains only Markdown. **`src/` is kept in full, including its
132 `*.test.ts` / `*.test.tsx` files**, whose symbols are flagged `test` and kept — the same
decision as flask's `tests/` (§10.8). `runtime-tests/{bun,fastly,lambda,lambda-edge,node,workerd}`
are kept for the same reason.

**`.tsx` is in scope.** 23 indexed documents, 10 of which contribute 16 golden symbols. The
scorer's `LANGUAGE_EXTENSIONS` maps `typescript` to `(".ts", ".tsx")` and `language_of_path`
therefore classifies `.tsx` as `typescript`, so the golden's `language: "typescript"` and the
scorer's extension-based scoping agree; no change to `corpus_score.py` is needed.

## 11.9 Known oracle artifacts (read this before triaging a TypeScript span or FP)

1. **Accessors carry no `enclosing_range`** — 0 of 28. The golden's span for a `get`/`set`
   collapses to the declaration line (`src/context.ts:366 req` is `366..366`, source
   `366..369`). Every accessor is a guaranteed span mismatch against any parser that reports a
   body. 20 of hono's 30 span mismatches are this.
2. **`namespace` declarations carry no `enclosing_range`** — 0 of 22. Same collapse:
   `Deno` is `1..1` (source `1..79`), `JSX` in `src/jsx/intrinsic-elements.ts` is `14..14`
   (source `14..922`). All 3 matched namespaces mismatch on span, so the `namespace` row's
   `span_exact_rate` is `0.00` **by construction**.
3. **58 signature-only class/interface members carry no `enclosing_range`** either, so a
   multi-line member signature collapses to its first line.
4. **Declarations inside a `describe()` / `it()` callback are `local`** and therefore absent
   from the golden, while a syntax-level parser reports them. This is the mirror of flask's
   §10.9 and produces false positives with no golden counterpart — e.g. `Repeat`,
   `ListOfTenThings` (`src/jsx/index.test.tsx:432,440`), `JSXRendererEnv`
   (`src/middleware/jsx-renderer/index.test.tsx:358`). The scorer's `nested` bucket cannot
   absorb them because the enclosing callback is anonymous and so is never itself emitted.
5. **`declare global {}`** (`runtime-tests/fastly/index.test.ts:7`) is emitted by the oracle as
   a namespace named `global`, but tree-sitter-typescript parses `declare global` with `global`
   as an anonymous token rather than a `module`/`internal_module` name, so loregrep
   structurally cannot produce it. 1 guaranteed false negative, kept in the golden rather than
   special-cased: the declaration is real, and hiding it would hide the gap.
6. **A document may be indexed more than once** (512 documents, 326 distinct paths) because
   several files belong to several tsconfig projects. Identical duplicates collapse in the
   dedup pass — 1264 of 2607 pre-dedup entries. Genuinely distinct entries at distinct lines
   survive, which is correct: they are overload signatures and merged declarations
   (`param` at `src/request.ts:94,97,100,101,102`).

These are properties of the oracle, not defects in the parser under test, and are deliberately
**not** compensated for in the converter.

## 11.10 Reference numbers (hono v4.12.31)

**1343 symbols across 193 files** — 505 `function`, 385 `type_alias`, 198 `method`,
196 `interface`, 52 `class`, 4 `namespace`, 3 `enum`. 16 of them in 10 `.tsx` files.
Flags: 79 `test`, 9 `ambient`. Visibility: 1343 `unknown` (§11.5).

Dropped, by category:

| Category | Count |
| --- | --- |
| `local N` (everything inside a function body) | 22447 |
| SCIP meta descriptors (`name0:`, `X#typeLiteral0:`) | 4558 |
| member bindings (class fields, interface/type-literal properties, enum members) | 2570 |
| exact duplicates (a file indexed under several tsconfig projects) | 1264 |
| parameters | 670 |
| type parameters | 607 |
| per-file module pseudo-symbols | 510 |
| module-level non-function bindings (`const N = 1`, re-export aliases) | 430 |
| definitions under `excluded_paths` (2 documents) | 16 |
| `declare module '…' {}` (quoted module specifiers) | 15 |
| unclassifiable type-ish descriptors, unparseable symbols, external packages, missing ranges | 0 each |

Kind-signal disagreements: **0** descriptor-vs-head, **107** binding-structure-vs-rendered-type
(all explained in §11.2).

## 11.11 Regenerating

```sh
bash evals/corpus/regen_typescript.sh                  # fetch + npm + scip-typescript, ~8s to index
python3 evals/corpus/scripts/scip_to_golden.py \
    --scip evals/corpus/hono/index.scip \
    --repo hono --commit cadff88bba34153646c9b35f24d7cc0cb61be913 \
    --language typescript \
    --exclude benchmarks --exclude build --exclude perf-measures \
    --exclude runtime-tests/deno --exclude runtime-tests/deno-jsx \
    --exclude vitest.config.ts \
    --repo-root evals/corpus/hono/src \
    --out evals/corpus/hono/golden-symbols.json --stats
python3 evals/corpus/scripts/test_scip_to_golden.py
```

`--repo-root` is accepted but unused on the TypeScript path (it exists for Rust's
`#[cfg(test)]` detection); the TypeScript converter reads no source files at all. An indexer
or converter version bump is a fixture bump: regenerate, diff, re-triage.

**Measurement hygiene, discovered while scoring hono:** `loregrep.toml` in the repository root
is picked up by the CLI's config discovery (`CliConfig::default_config_paths`), and its
`exclude_patterns` REPLACE the built-in defaults. A developer copy of that file therefore
silently changes what `corpus_score.py` sees. Score a corpus from a directory without one, or
record the effective config alongside the scorecard.
