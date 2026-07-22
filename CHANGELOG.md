# Changelog

Notable changes to loregrep. This file starts at 0.6.0; for earlier releases see
the [GitHub releases](https://github.com/Vasu014/loregrep/releases) and git history.

## 0.6.0 — unreleased

The theme is trustworthy paths and an index cache that stays out of your way. A
definition-parity eval corpus (real repositories indexed by `rust-analyzer scip`
and `scip-python`) was added first, and then found the bugs below.

### Breaking

- **The index cache no longer lives inside the repository it indexes.** It moved
  to your user cache directory (`~/Library/Caches/…` on macOS, `$XDG_CACHE_HOME`
  on Linux), keyed by a hash of the repository's canonical path. `exec-tool` used
  to write `<repo>/.loregrep/index.cache` into whatever tree it was pointed at,
  gated on nothing — a read-only query modified your working tree.

  **If you have automation that deletes `<repo>/.loregrep` to force a cold
  scan, it is now a silent no-op.** Force a cold scan by pointing
  `LOREGREP_CACHE_PATH` at a fresh directory, or set
  `LOREGREP_CACHE_ENABLED=false` to disable caching entirely. Old `.loregrep/`
  directories are left alone; they are inert and safe to delete.

- **Cache format v3.** Every previously written cache is rejected on load, by
  design and with no migration — the older formats could not prove which root
  they described. The first run after upgrading rescans.

- `LoreGrep::default_cache_path(root)` is replaced by
  `LoreGrep::cache_path_for(cache_root, repo_root)`, which rejects a relative
  cache root.

- `LoreGrepBuilder::cache_ttl` is removed. It set a field nothing ever read: a
  caller's value never reached `RepoMap`, so its hardcoded 300s applied and your
  setting was discarded silently. Index freshness is decided by content hashes.

- `LoreGrepBuilder::with_go_analyzer` is removed. It was a no-op that returned
  `self`, so calling it announced Go support and then skipped every Go file in
  silence. It returns when an analyzer honours it.

- The `scan` subcommand's `--include`, `--exclude` and `--follow-symlinks` flags
  are removed. They were parsed and discarded — nothing ever read them. Use the
  config file's `file_scanning.*` keys.

- The `cache.ttl_hours` config key is removed. Leaving it in an existing config
  file is harmless (unknown keys are ignored), and it never had an effect.

### Fixed

- `-d`/`--directory` applied only when the path argument was the default, so
  `loregrep -d /repo scan src` scanned nothing and cheerfully reported
  `Files scanned: 0`. All four subcommands now resolve paths by one rule:
  absolute arguments verbatim, relative arguments joined onto `-d`.
- `loregrep search` never loaded the persisted index. Since every process starts
  empty, it always answered "Repository not scanned. Run 'scan' first" — telling
  you to run the scan you had just run. It now loads-or-scans like `exec-tool`.
- Emitted paths varied with how `--path` was spelled, symlinked roots aliased to
  separate entries, and the index and file-set used two disagreeing key spaces.
  Paths are canonicalized once at the scanner boundary and emitted root-relative,
  with the canonical root reported as `analysis_root` on every tool response.
- `analyze_file` could read files outside the analysis root.
- A truncated index (hitting `max_files`) was cached and later reloaded as if it
  were a complete view of the repository. It is now refused for caching, refused
  on load, and reported in the response — absence of a symbol is not evidence
  that the repository lacks it.
- Cache freshness compared only the newest mtime, so a file added with a
  preserved older timestamp (`cp -p`, `rsync -a`, `tar -x`) stayed invisible.
- `scan()` was additive and never reset, so one instance scanning several roots
  accumulated symbols across repositories and answered queries from the wrong one.
- Several Rust and TypeScript parsing bugs found by the SCIP parity corpus,
  including TypeScript declarations being dropped after an unparseable construct.

### Added

- SCIP definition-parity eval corpus over ripgrep, flask and hono, scored
  against compiler-grade indexers.
- Python bindings for `get_stats`, `index_coverage`, `is_scanned`,
  `set_scan_root`, `first_missing_indexed_path` and `clear_index`; builder
  bindings for direct construction, `with_typescript_analyzer`, `max_files`,
  `include_patterns`, `follow_symlinks`, `unlimited_depth` and
  `configure_patterns_for_languages`. New `IndexCoverage` type so Python callers
  can detect a truncated index, and `ScanResult.languages` is now populated
  rather than silently dropped.
- `loregrep.__version__` is derived from the compiled extension instead of being
  hardcoded, so it can no longer report a version no code in the process
  implements.
