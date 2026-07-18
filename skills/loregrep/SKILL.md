---
name: loregrep
description: >-
  Fast structural code search for a repository via the `loregrep` CLI. Use this
  when you need to find functions or structs/classes by name or pattern, list a
  function's callers, get a file's imports/exports, get a structured skeleton of
  a file, or get a repository overview — across Rust, Python, and TypeScript/TSX.
  Prefer this over grep/manual file reading for "where is X defined", "what calls
  X", "what does this file export", and codebase-orientation questions: it returns
  precise, structured JSON (names, signatures, line numbers, file paths) instead
  of raw text matches, and it's cheaper on tokens.
---

# loregrep — structural code search

`loregrep` parses a repository with tree-sitter and answers structural queries.
You invoke a single tool per call and get JSON on stdout:

```bash
loregrep exec-tool <TOOL> --params '<JSON>' --path <DIR>
```

- `--path` is the directory to analyze (defaults to the current directory).
- Diagnostics go to stderr; **stdout is pure JSON** — parse it directly.
- The index is cached under `<DIR>/.loregrep/`, so repeated calls are fast; an
  edit to a source file automatically invalidates the cache and re-scans.
- Exit code is non-zero if the tool fails (the JSON `success` field is `false`).

## Prerequisite

The `loregrep` binary must be on PATH. If a call fails with "command not found":

```bash
cargo install loregrep      # Rust toolchain
# or use the Python wheel:  pip install loregrep
```

## When to use which tool

| You want to… | Tool | Required params |
|---|---|---|
| Find functions by name/pattern | `search_functions` | `pattern` |
| Find structs/classes by name/pattern | `search_structs` | `pattern` |
| Get a file's functions/structs/imports (skeleton) | `analyze_file` | `file_path` |
| Get a file's imports/exports (dependencies) | `get_dependencies` | `file_path` |
| Find everywhere a function is called | `find_callers` | `function_name` |
| Get a repository overview / tree | `get_repository_tree` | — |

Optional params: `limit` (search/callers), `language` (filter: `rust`/`python`/`typescript`),
`include_content` (analyze_file), `include_file_details` + `max_depth` (repository tree).

## Examples

Find functions matching a pattern (regex-capable), limited to 20:

```bash
loregrep exec-tool search_functions --params '{"pattern":"auth","limit":20}' --path .
```

Find who calls a function:

```bash
loregrep exec-tool find_callers --params '{"function_name":"parse_config"}' --path .
```

Get a file's structured skeleton (functions, structs, imports — no full source):

```bash
loregrep exec-tool analyze_file --params '{"file_path":"src/main.rs"}' --path .
```

Orient in an unfamiliar repo (shallow overview):

```bash
loregrep exec-tool get_repository_tree --params '{"include_file_details":false,"max_depth":1}' --path .
```

Find structs/classes, Python only:

```bash
loregrep exec-tool search_structs --params '{"pattern":"Config","language":"python"}' --path .
```

## Result shape

Each call returns `{"success": bool, "data": {...}, "error": null|string}`. For the
search tools, `data.results` is an array of items carrying `name`, `file_path`,
`start_line`/`end_line`, and (for functions) `parameters`, `return_type`,
`is_async`, `is_public`. Read those fields directly rather than re-parsing source.

## Tips

- Reach for `search_functions`/`find_callers` instead of `grep` when you care about
  *definitions* and *call sites* rather than every textual mention — fewer false
  positives, precise line numbers, far fewer tokens.
- Use `get_repository_tree` first when dropped into an unfamiliar codebase.
- Supported languages today: **Rust, Python, TypeScript/TSX**. Other languages are
  skipped (the tool tells you if no analyzer matched a file).
