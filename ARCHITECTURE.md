# Architecture

Loregrep turns a repository into structured, queryable code intelligence for AI agents. It parses
source with Tree-sitter, builds a fast in-memory index, and exposes a small set of **tools** (JSON
in, JSON out) that an agent calls.

## Data flow

```
files ──▶ scanner ──▶ LanguageAnalyzer ──▶ RepoMap (in-memory index) ──▶ tools ──▶ JSON
        (discovery)   (Tree-sitter, per      (inverted indexes:          (agent-facing)
                       language)               functions/structs/
                                               imports/calls)
```

1. **Scan** (`src/scanner/`, `src/loregrep.rs`) — gitignore-aware file discovery.
2. **Analyze** (`src/analyzers/`) — each file is parsed by the analyzer registered for its language.
3. **Index** (`src/storage/`) — `RepoMap` stores symbols in inverted `HashMap` indexes for fast lookup.
4. **Query** (`src/internal/ai_tools.rs`) — the tools read the index and return structured results.

## Module map

| Path | Responsibility |
|------|----------------|
| `src/analyzers/` | Language analyzers. `traits.rs` defines `LanguageAnalyzer`; `rust.rs`, `python.rs`, `typescript.rs` implement it; `registry.rs` maps language/extension → analyzer. **This is the main extension point.** |
| `src/storage/` | `memory.rs` = `RepoMap` (the in-memory index). `persistence.rs` = on-disk cache (gzip JSON). |
| `src/scanner/` | File discovery (`ignore`/gitignore-aware, parallel). |
| `src/loregrep.rs` | Public API + builder (`LoreGrep`, `LoreGrepBuilder`): `scan()`, `execute_tool()`, `get_tool_definitions()`. |
| `src/internal/ai_tools.rs` | The tool implementations + their JSON schemas. |
| `src/internal/cli.rs`, `src/main.rs` | The `loregrep` CLI (agent-facing; machine-first output). |
| `src/core/`, `src/types/` | Shared data types and error enums (`Result<T, ...>`). |
| `src/lib.rs` | Library entry + Python (PyO3) bindings. |

## The tools

Six structured tools an agent calls (schemas in `ai_tools.rs::get_tool_schemas`):
`search_functions`, `search_structs`, `analyze_file`, `get_dependencies`, `find_callers`,
`get_repository_tree`. Each returns a `ToolResult { success, data, error }`.

## The extension seam: `LanguageAnalyzer`

Adding a language means implementing one trait in one new file and registering it — no core changes.
The trait (`src/analyzers/traits.rs`):

```rust
#[async_trait]
pub trait LanguageAnalyzer: Send + Sync {
    fn language(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];
    fn supports_async(&self) -> bool;
    async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis>;
    fn extract_functions(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<FunctionSignature>>;
    fn extract_structs(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<StructSignature>>;
    fn extract_imports(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<ImportStatement>>;
    fn extract_exports(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<ExportStatement>>;
    fn extract_function_calls(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<FunctionCall>>;
    fn extract_with_fallback(&self, content: &str, file_path: &str) -> PartialAnalysis;
}
```

See [docs/adding-a-language.md](docs/adding-a-language.md); `src/analyzers/typescript.rs` is the
reference implementation to copy.

## Design principles

- **Agent-facing:** the interface is structured tools + a machine-first CLI (JSON→stdout, logs→stderr).
  No interactive TUI; loregrep does not call an LLM itself (the agent is the LLM).
- **Result-based errors:** operations return `Result<T, _>`; avoid `unwrap()` on non-test paths.
- **Drop-in languages:** the registry dispatches by language, so analyzers are additive.
