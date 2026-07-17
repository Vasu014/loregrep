---
name: New language analyzer
about: Add support for parsing a new language
title: "[lang] Add <language> analyzer"
labels: new-language, good-first-issue
---

## Language

<!-- e.g. Go, Java, Ruby, C++ -->

## Tree-sitter grammar

<!-- Link to the tree-sitter-<lang> crate and the version compatible with our core. -->

## Scope

- File extensions:
- Constructs to extract: functions, types/structs/classes, imports, exports, calls

## Guide

Follow **[docs/adding-a-language.md](../../docs/adding-a-language.md)**. Use
`src/analyzers/typescript.rs` as the reference implementation. This is self-contained — you should
not need to modify core code beyond registering the analyzer.

## Checklist

- [ ] `src/analyzers/<lang>.rs` implements `LanguageAnalyzer`
- [ ] `with_<lang>_analyzer()` builder method
- [ ] ~20 unit tests
- [ ] Extraction fixtures under `evals/fixtures/<lang>/`
- [ ] CI green
