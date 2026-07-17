# Contributing to Loregrep

Thanks for your interest in contributing! Loregrep is an **agent-facing** code-analysis tool:
Tree-sitter parsing → fast in-memory index → structured tools (JSON in, JSON out) that AI coding
agents call. It is a CLI and a library (Rust crate + Python wheel), **not** an interactive human app.

## Development setup

```bash
# Toolchain: latest stable Rust (see rust-toolchain.toml)
rustup update stable

# Build
cargo build

# Run the full test suite
cargo test --all-features

# Format + lint (must pass in CI)
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

Python bindings:

```bash
pip install maturin
maturin develop --features python
pytest python/tests
```

## Before you open a PR

CI runs on every PR and must be green:

- `cargo fmt --all --check` — run `cargo fmt --all` to fix.
- `cargo clippy --all-targets --all-features -- -D warnings` — no warnings.
- `cargo test --all-features` — all tests pass.

Keep changes focused. New behavior needs tests in the same file under `#[cfg(test)]`.

## Good first issues

The most valuable, well-scoped contribution is **adding a new language analyzer**. Each language is
a self-contained file (`src/analyzers/<lang>.rs`) implementing the `LanguageAnalyzer` trait — you
should not need to touch core code. See **[docs/adding-a-language.md](docs/adding-a-language.md)**,
using `src/analyzers/typescript.rs` as the reference implementation.

Look for issues labeled [`good-first-issue`](../../labels/good-first-issue) and
[`new-language`](../../labels/new-language).

## Architecture

See **[ARCHITECTURE.md](ARCHITECTURE.md)** for the module map and data flow.

## Commit / PR conventions

- Small, reviewable PRs. One logical change per PR.
- Describe *what* and *why* in the PR body (the template prompts you).
- Reference the issue you're closing.
