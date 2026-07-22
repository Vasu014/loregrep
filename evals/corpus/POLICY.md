# Corpus golden policy — definitions

This is the decision record for `evals/corpus/scripts/scip_to_golden.py`. It describes what
that converter **actually does** today, not what a golden could ideally contain. Every
inclusion, exclusion and abstention below is implemented and covered by
`evals/corpus/scripts/test_scip_to_golden.py`.

Scope: **definitions only**, per `evals/EVAL_PLAN.md` §4b.2. Edge (call/import) goldens are
deliberately deferred to P3-7. Language implemented: **Rust** (rust-analyzer SCIP). The
converter refuses any other `--language` rather than pretending its Rust descriptor grammar
generalises.

Contract: output validates against `evals/corpus/golden.schema.json` (`schema_version: 1`).

## 1. What counts as a symbol

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

`class` / `interface` exist in the mapping table for the TypeScript/Python converters that
will follow; no Rust construct produces them.

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
`include_nested_functions` is false, as recorded in each golden's `policy` block.

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
