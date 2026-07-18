---
name: loregrep
description: >-
  Query a codebase's structure as a graph, not as text. loregrep parses the repo
  with tree-sitter into a queryable graph of functions, types, imports, and call
  relationships, and answers structural questions with exact, structured results
  (signatures, call sites, dependencies, line numbers). Reach for it — instead of
  grep or reading files — whenever the question is about code STRUCTURE: "where is
  X defined", "who calls X", "what does this file import/export", "what's the shape
  of this function", or "give me a map of the repo". grep returns every textual
  mention (comments, string literals, the definition itself, unrelated matches);
  loregrep returns the real structural answer, precise and cheaper on tokens.
  Languages: Rust, Python, TypeScript/TSX.
---

# loregrep — a queryable code graph

`loregrep` parses a repository with tree-sitter into a **structural graph** —
every function, type, import, and call edge — and lets you query it. You get
exact, structured answers, not text matches. One tool per call, JSON on stdout:

```bash
loregrep exec-tool <TOOL> --params '<JSON>' --path <DIR>
```

- `--path` is the directory to analyze (defaults to the current directory).
- Diagnostics go to stderr; **stdout is pure JSON** — parse it directly.
- The index is cached under `<DIR>/.loregrep/`, so repeated calls are fast; editing
  a source file automatically invalidates the cache and re-scans.
- Exit code is non-zero if the tool fails (JSON `success` is `false`).

## When to use loregrep instead of grep/read

Default to loregrep for anything **structural** — it answers precisely what grep
answers noisily:

| The question | Use | Why not grep |
|---|---|---|
| Who calls function `X`? | `find_callers` | grep also returns the definition, comments, strings, imports |
| Where is `X` defined / what's its signature? | `search_functions` / `search_structs` | grep can't give you params/return type/visibility, and hits every mention |
| What does this file import/export? | `get_dependencies` | grep can't distinguish imports from usage |
| What's in this file (skeleton)? | `analyze_file` | avoids reading the whole file into context |
| Orient me in this repo | `get_repository_tree` | a structural map, not a file dump |

Use plain `grep` only for free-text / non-code search (log strings, config values,
prose). For "who calls this", "where's this defined", "what depends on what" —
loregrep is exact where grep is a guess.

## Prerequisite

The `loregrep` binary must be on PATH. If a call fails with "command not found":

```bash
cargo install loregrep      # Rust toolchain
# or use the Python wheel:  pip install loregrep
```

## Tools

| Tool | Answers | Required params |
|---|---|---|
| `find_callers` | Exact call sites of a function (file:line), no false positives | `function_name` |
| `search_functions` | Function definitions + signatures by name/pattern | `pattern` |
| `search_structs` | Struct/class/interface definitions + fields by name/pattern | `pattern` |
| `get_dependencies` | A file's import/export edges | `file_path` |
| `analyze_file` | A file's skeleton (functions/structs/imports/calls) | `file_path` |
| `get_repository_tree` | Structural map of the repo | — |

Optional params: `limit` (search/callers), `include_content` (analyze_file),
`include_file_details` + `max_depth` (repository tree).

## Examples

Who calls a function (the thing grep gets wrong):

```bash
loregrep exec-tool find_callers --params '{"function_name":"parse_config"}' --path .
```

Find a function's definition and signature:

```bash
loregrep exec-tool search_functions --params '{"pattern":"auth","limit":20}' --path .
```

A file's structural skeleton, without reading it:

```bash
loregrep exec-tool analyze_file --params '{"file_path":"src/main.rs"}' --path .
```

Orient in an unfamiliar repo:

```bash
loregrep exec-tool get_repository_tree --params '{"include_file_details":false,"max_depth":1}' --path .
```

## Result shape

Each call returns `{"success": bool, "data": {...}, "error": null|string}`.
`find_callers` → `data.callers` (`file_path`, `line_number`, `caller_function`).
Search tools → `data.results` with `name`, `file_path`, `start_line`, and (for
functions) `parameters`, `return_type`, `is_async`, `is_public`. Read those fields
directly rather than re-parsing source.
