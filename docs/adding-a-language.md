# Adding a language analyzer

Adding support for a new language is the most valuable contribution to loregrep, and it's designed
to be self-contained: **one new file, no core changes.**

Reference implementation: `src/analyzers/typescript.rs` (or `rust.rs` / `python.rs`).

## The 5 steps

### 1. Add the Tree-sitter grammar
In `Cargo.toml`, add the grammar crate (matching the workspace's `tree-sitter` core version), e.g.:

```toml
tree-sitter-go = "0.25"   # use the version compatible with our tree-sitter core
```

### 2. Create `src/analyzers/<lang>.rs`
Implement the `LanguageAnalyzer` trait (see `src/analyzers/traits.rs`). Copy `typescript.rs` as a
skeleton and adapt the Tree-sitter queries to your language's grammar node names:

- `language()` → `"go"`
- `file_extensions()` → `["go"]`
- `extract_functions` / `extract_structs` / `extract_imports` / `extract_exports` /
  `extract_function_calls` — write Tree-sitter queries against the grammar. Prefer AST nodes over
  string matching.
- `extract_with_fallback` — a regex-based best-effort recovery when parsing fails.

Register the module in `src/analyzers/mod.rs`.

### 3. Wire the builder method
In `src/loregrep.rs`, add `with_<language>_analyzer()` following the `with_rust_analyzer()` pattern, and add
your language to auto-discovery if applicable. Because dispatch goes through the registry, this is
the only touch outside your file.

### 4. Add tests
In your `<lang>.rs`, add a `#[cfg(test)]` module mirroring the coverage in `typescript.rs`:
functions (incl. async/visibility variants), types/structs/classes, imports, exports, calls, and the
fallback path. Aim for ~20 focused tests.

### 5. Add extraction fixtures
Add a small sample repo under `evals/fixtures/<lang>/` with a gold symbol set, so the benchmark's
retrieval suite gates your analyzer objectively. (See existing fixtures.)

## Checklist

- [ ] Grammar crate added at a compatible version
- [ ] `src/analyzers/<lang>.rs` implements `LanguageAnalyzer`
- [ ] Module registered in `mod.rs`; `with_<lang>_analyzer()` added
- [ ] ~20 unit tests, all passing
- [ ] Extraction fixtures added
- [ ] `cargo fmt --all` + `cargo clippy -- -D warnings` + `cargo test` green
