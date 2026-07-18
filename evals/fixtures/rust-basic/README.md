# rust-basic fixture

A tiny, hand-written Rust repository used as a deterministic Layer 1 retrieval
fixture for the loregrep eval harness. Files are intentionally small so line
numbers can be reviewed by hand.

It embeds "grep traps" that distinguish structural search from text search:

- `parse_config` appears as a real function, in a comment, in a string literal,
  in a doc example, and as a commented-out call site.
- `parse` vs `parse_config` exercise the exact-match-first branch.
- `handle_get` / `handle_post` / `handle_delete` exercise the `^handle_.*`
  regex-pattern branch (while `dispatch` must not match).
- `parse_config` has real call sites in `main` and in `Loader::load` (a method),
  plus a commented-out call in `cache.rs` that must not be reported.
- `describe` exists in two files (`loader.rs`, `cache.rs`).
- `Cache<T>` / `Wrapper<T>` are generic structs; `Span` is a tuple struct.

Do not auto-format these files; gold line numbers depend on their exact layout.
