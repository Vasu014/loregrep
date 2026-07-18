---
name: loregrep
description: >-
  Query a codebase's structure via the loregrep CLI. loregrep parses the repo with
  tree-sitter into a queryable graph of functions, types, imports, and call
  relationships, and returns structured results (signatures, call sites,
  dependencies, line numbers). Use it for structural questions about code: where a
  symbol is defined and its signature, who calls a function, what a file imports or
  exports, a file's skeleton, or a map of the repository. Languages: Rust, Python,
  TypeScript/TSX.
---

# loregrep — a queryable code graph

`loregrep` parses a repository with tree-sitter into a structural graph — functions,
types, imports, and call relationships — and lets you query it, returning structured
JSON (names, signatures, file paths, line numbers). One tool per call, JSON on stdout:

```bash
loregrep exec-tool <TOOL> --params '<JSON>' --path <DIR>
```

- `--path` is the directory to analyze (defaults to the current directory).
- Diagnostics go to stderr; **stdout is pure JSON** — parse it directly.
- The index is cached under `<DIR>/.loregrep/`, so repeated calls are fast; editing
  a source file automatically invalidates the cache and re-scans.
- Exit code is non-zero if the tool fails (JSON `success` is `false`).

## Prerequisite

The `loregrep` binary must be on PATH. If a call fails with "command not found":

```bash
cargo install loregrep      # Rust toolchain
# or use the Python wheel:  pip install loregrep
```

## Tools

| Tool | What it returns | Required params |
|---|---|---|
| `find_callers` | The direct call sites of a function (file:line), from the call graph | `function_name` |
| `search_functions` | Function definitions matching a name/regex, with their signatures | `pattern` |
| `search_structs` | Struct/class/interface definitions matching a name/regex, with their fields | `pattern` |
| `get_dependencies` | A file's imports and exports | `file_path` |
| `analyze_file` | A file's skeleton (functions, structs, imports, exports, calls) | `file_path` |
| `get_repository_tree` | A structural map of the repository | — |

Optional params: `limit` (search/callers), `language` (`rust`/`python`/`typescript`),
`include_content` (analyze_file), `include_file_details` + `max_depth` (repository tree).

## Examples

Find the call sites of a function:

```bash
loregrep exec-tool find_callers --params '{"function_name":"parse_config"}' --path .
```

Find a function's definition and signature:

```bash
loregrep exec-tool search_functions --params '{"pattern":"auth","limit":20}' --path .
```

A file's structural skeleton:

```bash
loregrep exec-tool analyze_file --params '{"file_path":"src/main.rs"}' --path .
```

A map of an unfamiliar repository:

```bash
loregrep exec-tool get_repository_tree --params '{"include_file_details":false,"max_depth":1}' --path .
```

## Result shape

Each call returns `{"success": bool, "data": {...}, "error": null|string}`.
`find_callers` → `data.callers` (`file_path`, `line_number`, `caller_function`).
Search tools → `data.results` with `name`, `file_path`, `start_line`, and (for
functions) `parameters`, `return_type`, `is_async`, `is_public`. Read those fields
directly rather than re-parsing source.
