# loregrep-pi

A [pi](https://pi.dev) coding-agent extension that gives the agent **structural code
search** over a repository via the [`loregrep`](https://github.com/Vasu014/loregrep)
CLI — precise, JSON, token-cheap answers instead of grep/manual reading.

## Install

```bash
pi install npm:loregrep-pi
```

The extension shells out to the `loregrep` binary, which must be on PATH:

```bash
cargo install loregrep      # Rust toolchain
# or:  pip install loregrep  # Python wheel
```

## Tools it adds

| Tool | Purpose |
|---|---|
| `search_functions` | Find functions by name/regex (name, signature, file, line) |
| `search_structs` | Find structs/classes/interfaces by name/regex |
| `find_callers` | All call sites of a function |
| `get_dependencies` | A file's imports/exports |
| `analyze_file` | A file's skeleton (functions/structs/imports/calls) |
| `get_repository_tree` | Repository overview / directory tree |

Every tool accepts an optional `path` (directory to analyze, default `.`). Search
tools accept `limit` and `language` (`rust`/`python`/`typescript`). Supported
languages today: **Rust, Python, TypeScript/TSX**.

loregrep caches its index under `<path>/.loregrep/`, so repeated tool calls in a
session are fast; a source edit automatically invalidates the cache.

## Status

The tool-registration logic is complete and mirrors the verified `loregrep exec-tool`
CLI. The pi SDK surface (`ExtensionAPI`, `pi.exec` result shape, the `typebox` import)
is written to pi's documented extension API but **has not yet been run against a live
pi runtime** — expect to pin `@earendil-works/pi-coding-agent` / `typebox` to the
versions your pi install provides and adjust the `pi.exec` result fields if they
differ. See [`index.ts`](./index.ts).
