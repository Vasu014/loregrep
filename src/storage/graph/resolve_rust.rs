//! Rust import resolver (P2-4). Maps a `use` path's MODULE portion to a scanned
//! file using path + file-convention probing — `foo.rs` first, then `foo/mod.rs`.
//! No full `mod`-tree is built; resolution is heuristic with explicit uncertainty:
//!
//! - `crate::a::b` → suffix-match the scanned set for `.../a/b.rs` or `.../a/b/mod.rs`.
//! - `super::x` / `self::x` → probe relative to the importing file's directory
//!   (one directory up per `super`).
//! - a first segment that is not `crate`/`self`/`super` and matches no scanned file
//!   (`std`, `serde`, `tokio`, …) → [`ImportTarget::External`].
//! - anything not confidently mappable (re-exports, items with no file of their own,
//!   ambiguous suffix matches) → [`ImportTarget::Unresolved`] — carried, never guessed.
//!
//! The analyzer stores `module_path` as the raw text of the `use` argument, e.g.
//! `"crate::config::Config"`, `"super::x::Y"`, `"std::collections::HashMap"`,
//! `"crate::a::*"` (glob), `"crate::a::{X, Y}"` (group). The last plain segment is
//! normally the imported item and is dropped to get the module path; a `*` or a
//! `{...}` group names items, so everything before it is the module.

use super::{normalize_path, parent_dir, FileSet, ImportTarget};
use crate::types::ImportStatement;

pub fn resolve_rust_import(
    import: &ImportStatement,
    from_file: usize,
    files: &FileSet,
) -> ImportTarget {
    let raw = import.module_path.trim();
    if raw.is_empty() {
        return ImportTarget::Unresolved(import.module_path.clone());
    }

    // Drop a trailing `{...}` group (`crate::a::{X, Y}` → `crate::a`); a glob `*` is
    // filtered out below. Both name items, not modules.
    let base = match raw.find('{') {
        Some(i) => raw[..i].trim_end_matches(':').trim(),
        None => raw,
    };
    // When the import names items via a group or glob, none of the remaining
    // segments is a trailing item to drop — they are all module path.
    let no_drop = raw.contains('{') || raw.contains('*') || import.is_glob;

    let segments: Vec<&str> = base
        .split("::")
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "*")
        .collect();
    if segments.is_empty() {
        return ImportTarget::Unresolved(import.module_path.clone());
    }

    let root = segments[0];
    let after: &[&str] = &segments[1..];

    let resolved = match root {
        "crate" => resolve_in_repo(after, no_drop, files),
        "self" => resolve_relative(after, no_drop, from_file, 0, files),
        "super" => {
            // Count the leading `super` hops (`super::super::x` → 2).
            let mut hops = 1usize;
            let mut rest = after;
            while let Some((first, tail)) = rest.split_first() {
                if *first == "super" {
                    hops += 1;
                    rest = tail;
                } else {
                    break;
                }
            }
            resolve_relative(rest, no_drop, from_file, hops, files)
        }
        // Non-crate root: in-repo only if the full path (root included) suffix-matches
        // a scanned file; otherwise it's an external dependency (std / crates.io).
        _ => {
            return resolve_in_repo(&segments, no_drop, files)
                .unwrap_or_else(|| ImportTarget::External(import.module_path.clone()));
        }
    };

    resolved.unwrap_or_else(|| ImportTarget::Unresolved(import.module_path.clone()))
}

/// The module-path interpretations to try, most specific first. For a group/glob
/// import every segment is module path. Otherwise the last segment is normally the
/// imported item (so try the path without it first), but `use crate::foo;` names a
/// module directly, so fall back to including the last segment.
fn module_candidates<'a>(segs: &'a [&'a str], no_drop: bool) -> Vec<&'a [&'a str]> {
    if segs.is_empty() {
        return Vec::new();
    }
    if no_drop {
        return vec![segs];
    }
    let mut out: Vec<&[&str]> = Vec::new();
    if segs.len() >= 2 {
        out.push(&segs[..segs.len() - 1]); // drop the trailing item
    }
    out.push(segs); // …or treat the last segment as a module
    out
}

/// Resolve an absolute (crate-rooted or external-rooted) module path by suffix-
/// matching the scanned file set.
fn resolve_in_repo(module_segs: &[&str], no_drop: bool, files: &FileSet) -> Option<ImportTarget> {
    for cand in module_candidates(module_segs, no_drop) {
        if let Some(idx) = suffix_match(cand, files) {
            return Some(ImportTarget::File(idx));
        }
    }
    None
}

/// Resolve a `self`/`super` path by probing relative to the importing file's
/// directory, walking `up_hops` directories up first (one per `super`).
fn resolve_relative(
    module_segs: &[&str],
    no_drop: bool,
    from_file: usize,
    up_hops: usize,
    files: &FileSet,
) -> Option<ImportTarget> {
    let mut base = files.dir_of(from_file)?;
    for _ in 0..up_hops {
        base = parent_dir(&base);
    }
    for cand in module_candidates(module_segs, no_drop) {
        let joined = cand.join("/");
        if let Some(idx) = files.probe(&base, &format!("{joined}.rs")) {
            return Some(ImportTarget::File(idx));
        }
        if let Some(idx) = files.probe(&base, &format!("{joined}/mod.rs")) {
            return Some(ImportTarget::File(idx));
        }
    }
    None
}

/// Find the unique scanned file whose normalized path ends with `<segs>.rs` or
/// `<segs>/mod.rs`. Tries the `foo.rs` convention before `foo/mod.rs`, and refuses
/// to pick when a convention matches more than one file (never guess).
fn suffix_match(segs: &[&str], files: &FileSet) -> Option<usize> {
    if segs.is_empty() {
        return None;
    }
    let joined = segs.join("/");
    for suffix in [format!("{joined}.rs"), format!("{joined}/mod.rs")] {
        let needle = format!("/{suffix}");
        let mut hit: Option<usize> = None;
        let mut ambiguous = false;
        for i in 0..files.len() {
            if let Some(p) = files.path(i) {
                let np = normalize_path(p);
                if np == suffix || np.ends_with(&needle) {
                    if hit.is_some() {
                        ambiguous = true;
                        break;
                    }
                    hit = Some(i);
                }
            }
        }
        if !ambiguous {
            if let Some(idx) = hit {
                return Some(idx);
            }
        }
        // Ambiguous → don't guess; a later convention may still be unique.
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TreeNode;

    fn rust_files(paths: &[&str]) -> Vec<TreeNode> {
        paths
            .iter()
            .map(|p| TreeNode::new(p.to_string(), "rust".to_string()))
            .collect()
    }

    fn imp(use_path: &str, from: &str) -> ImportStatement {
        // Mirror the analyzer's is_external rule so tests exercise real inputs.
        let is_ext = !use_path.starts_with("crate::")
            && !use_path.starts_with("self::")
            && !use_path.starts_with("super::");
        ImportStatement::new(use_path.to_string(), from.to_string())
            .with_external(is_ext)
            .with_glob(use_path.contains('*'))
    }

    #[test]
    fn crate_path_resolves_to_file() {
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/config.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::config::Config", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn crate_nested_resolves_to_foo_rs() {
        // Module `a::b` as `src/a/b.rs`.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/a/b.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::a::b::Item", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn crate_nested_resolves_to_mod_rs() {
        // Same module `a::b` under the `src/a/b/mod.rs` convention.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/a/b/mod.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::a::b::Item", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn crate_bare_module_import_resolves() {
        // `use crate::config;` — the last segment IS the module, not an item.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/config.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::config", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn crate_glob_resolves_module() {
        // `use crate::a::*;` — module is `a`, `*` names items.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/a.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::a::*", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn crate_group_resolves_module() {
        // `use crate::a::{X, Y};` — module is `a`, the group names items.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/a.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::a::{X, Y}", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn super_resolves_up_one_level() {
        // `super::x::Y` from a submodule file → one directory up, then `x.rs`.
        let files = rust_files(&["/repo/src/x.rs", "/repo/src/parent/child.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("super::x::Y", "/repo/src/parent/child.rs");
        assert_eq!(resolve_rust_import(&import, 1, &fs), ImportTarget::File(0));
    }

    #[test]
    fn self_resolves_submodule_in_same_dir() {
        // `use self::child::Thing;` → a submodule file beside the importer.
        let files = rust_files(&["/repo/src/a/lib.rs", "/repo/src/a/child.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("self::child::Thing", "/repo/src/a/lib.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn std_path_is_external() {
        let files = rust_files(&["/repo/src/main.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("std::collections::HashMap", "/repo/src/main.rs");
        assert_eq!(
            resolve_rust_import(&import, 0, &fs),
            ImportTarget::External("std::collections::HashMap".to_string())
        );
    }

    #[test]
    fn third_party_crate_is_external() {
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/config.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("serde::Deserialize", "/repo/src/main.rs");
        assert_eq!(
            resolve_rust_import(&import, 0, &fs),
            ImportTarget::External("serde::Deserialize".to_string())
        );
    }

    #[test]
    fn unmappable_crate_path_is_unresolved_not_a_wrong_file() {
        // A re-export / missing module: must be Unresolved, never a spurious File.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/config.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::reexports::deep::Symbol", "/repo/src/main.rs");
        match resolve_rust_import(&import, 0, &fs) {
            ImportTarget::Unresolved(p) => assert_eq!(p, "crate::reexports::deep::Symbol"),
            other => panic!("expected Unresolved, got {other:?}"),
        }
    }

    #[test]
    fn ambiguous_suffix_match_does_not_guess() {
        // Two files both end with `/util.rs`; `crate::util::X` must not pick one.
        let files = rust_files(&["/repo/src/util.rs", "/repo/vendor/util.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::util::X", "/repo/src/util.rs");
        assert!(matches!(
            resolve_rust_import(&import, 0, &fs),
            ImportTarget::Unresolved(_)
        ));
    }
}
