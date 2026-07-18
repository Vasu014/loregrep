# Loregrep

[![Crates.io](https://img.shields.io/crates/v/loregrep.svg)](https://crates.io/crates/loregrep)
[![PyPI](https://img.shields.io/pypi/v/loregrep.svg)](https://pypi.org/project/loregrep/)
[![CI](https://github.com/Vasu014/loregrep/actions/workflows/rust-ci.yml/badge.svg)](https://github.com/Vasu014/loregrep/actions/workflows/rust-ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**Structural code intelligence for AI coding agents.**

Loregrep parses a repository with [tree-sitter](https://tree-sitter.github.io/) into a fast
in-memory index and exposes it as a small set of tools an agent can call — returning precise,
structured JSON (names, signatures, callers, imports, line numbers) instead of raw text matches.
It's the *context engine* an agent calls; it is **not** an AI itself.

> Ask "where is `parse_config` defined", "what calls it", "what does this file export", or "give me
> a map of this repo" — and get exact answers, cheaper on tokens than grepping and reading files.

**Languages:** Rust · Python · TypeScript/TSX &nbsp;(Go, JavaScript planned)

---

## Why

A coding agent that greps and reads whole files burns tokens on noise and still misses cross-file
structure. Loregrep gives it **structured, symbol-level access**:

- **Precise, not textual** — definitions and call sites, with signatures and line numbers, not every string match.
- **Token-cheap** — compact JSON results instead of file dumps.
- **Fast & cached** — parse once; a persistent index makes repeated queries instant, and edits auto-invalidate it.
- **Agent-native** — one command per tool, JSON on stdout; ships as a Claude Code skill and a pi extension.
- **Embeddable** — a Rust crate *and* a Python wheel with the same tool API.

## Install

```bash
cargo install loregrep     # CLI + Rust library
pip install loregrep       # Python bindings
```

## Use it three ways

### 1. As an agent tool (CLI)

`exec-tool` runs one analysis tool and prints JSON to stdout (diagnostics go to stderr):

```bash
loregrep exec-tool search_functions --params '{"pattern":"auth","limit":20}' --path .
```

```json
{
  "success": true,
  "data": {
    "count": 1,
    "results": [
      { "name": "authenticate", "file_path": "src/auth.rs",
        "start_line": 42, "is_async": true, "is_public": true,
        "return_type": "Result<Session>", "parameters": [ ... ] }
    ]
  }
}
```

The index is cached under `.loregrep/`, so repeated calls are instant; editing a source file
invalidates the cache automatically.

### 2. Inside your coding agent (skill / extension)

- **Claude Code** — the [`loregrep` skill](skills/loregrep/SKILL.md) teaches the agent when and how
  to call these tools. Point Claude Code at `skills/loregrep/`.
- **pi** — install the [`loregrep-pi`](integrations/pi/) extension: `pi install npm:loregrep-pi`.

Both wrap the same six tools, so your agent reaches for structural search instead of grep.

### 3. As a library

**Rust:**

```rust
use loregrep::LoreGrep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lg = LoreGrep::builder()
        .with_all_analyzers()   // Rust, Python, TypeScript/TSX
        .build()?;

    lg.scan(".").await?;

    let result = lg.execute_tool(
        "search_functions",
        serde_json::json!({ "pattern": "auth", "limit": 20 }),
    ).await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
```

**Python:**

```python
import asyncio, loregrep

async def main():
    lg = loregrep.LoreGrep.polyglot_project(".")   # Rust + Python + TypeScript
    await lg.scan(".")
    result = await lg.execute_tool("search_functions", {"pattern": "auth", "limit": 20})
    print(result)

asyncio.run(main())
```

## Tools

| Tool | Purpose | Required params |
|---|---|---|
| `search_functions` | Find functions by name/regex | `pattern` |
| `search_structs` | Find structs/classes/interfaces by name/regex | `pattern` |
| `find_callers` | All call sites of a function | `function_name` |
| `get_dependencies` | A file's imports/exports | `file_path` |
| `analyze_file` | A file's skeleton (functions/structs/imports/calls) | `file_path` |
| `get_repository_tree` | Repository overview / tree | — |

Optional: `limit`, `language` (`rust`/`python`/`typescript`), `include_content`,
`include_file_details`, `max_depth`. Get machine-readable schemas with
`LoreGrep::get_tool_definitions()`.

## Language support

| Language | Functions | Structs/Classes | Imports/Exports | Calls |
|---|:-:|:-:|:-:|:-:|
| Rust | ✅ | ✅ | ✅ | ✅ |
| Python | ✅ | ✅ | ✅ | ✅ |
| TypeScript / TSX | ✅ | ✅ | ✅ | ✅ |
| JavaScript, Go | _planned_ | | | |

Adding a language is a **drop-in** contribution: implement one trait in one file — see
[docs/adding-a-language.md](docs/adding-a-language.md).

## How it works

```
files ──▶ scanner ──▶ tree-sitter analyzer ──▶ in-memory index ──▶ tools ──▶ JSON
        (gitignore-aware)   (per language)        (RepoMap)         (agent-facing)
```

The registry dispatches by language, so analyzers are additive; the index persists to disk and
re-scans only when files change. See [ARCHITECTURE.md](ARCHITECTURE.md).

## Contributing

Contributions — especially new language analyzers — are welcome. See
[CONTRIBUTING.md](CONTRIBUTING.md) and [docs/adding-a-language.md](docs/adding-a-language.md).
CI runs `cargo fmt`, `cargo test`, and the Python binding tests on every PR.

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
