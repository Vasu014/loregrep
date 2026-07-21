//! Rust import resolver (P2-4). Maps a `use` path's MODULE portion to a scanned
//! file using path + file-convention probing — `foo.rs` first, then `foo/mod.rs`.
//! No full `mod`-tree is built; resolution is heuristic with explicit uncertainty:
//!
//! - `crate::a::b` → probe the importing file's own crate root for `a/b.rs` then
//!   `a/b/mod.rs`; failing that, suffix-match the scanned set ANCHORED to that same
//!   crate root, so neither a workspace sibling's nor a deeper unrelated
//!   `b.rs` can be picked.
//! - `super::x` / `self::x` → probe relative to the importing file's MODULE
//!   directory (for `foo.rs` that is `<dir>/foo/`, not `<dir>/`), walking one
//!   directory up per `super`.
//! - a first segment that is not `crate`/`self`/`super` → an external dependency
//!   (`std`, `serde`, `tokio`, …), unless it names a sibling workspace crate
//!   (`<...>/<seg>/src/…`), which is the only in-repo reading we accept.
//! - anything not confidently mappable (re-exports, items with no file of their own,
//!   ambiguous suffix matches) → [`ImportTarget::Unresolved`] — carried, never guessed.
//!
//! The analyzer stores `module_path` as the raw text of the `use` argument, e.g.
//! `"crate::config::Config"`, `"super::x::Y"`, `"std::collections::HashMap"`,
//! `"crate::a::*"` (glob), `"crate::a::{X, Y}"` (group). The last plain segment is
//! normally the imported item and is dropped to get the module path; a `*` or a
//! `{...}` group names items, so everything before it is the module.

use super::{FileSet, ImportTarget, normalize_path, parent_dir};
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
    // Drop an `as` alias (`crate::config as cfg` → `crate::config`); it renames the
    // binding, it is not part of the module path.
    let base = base.split(" as ").next().unwrap_or(base).trim();
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
        // `crate::a::b` is a path FROM the crate root, so probe the root directly
        // and only then fall back to a suffix match anchored to that same root —
        // in a workspace `crate::util` can never mean a sibling crate's `util`.
        "crate" => {
            let root_dir = crate_root(from_file, files);
            root_dir
                .as_deref()
                .and_then(|dir| probe_from(dir, after, no_drop, files))
                .or_else(|| resolve_in_repo(after, no_drop, root_dir.as_deref(), files))
        }
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
        // Non-crate root. In edition 2018+ an in-repo module is reached via `crate::`,
        // so a bare root is an external dependency — EXCEPT a sibling workspace crate,
        // where the root segment names the crate directory (`crates/<root>/src/…`).
        // A bare suffix match is not enough on its own: `use log::info;` must stay
        // External even when the repo happens to contain a `src/log.rs`.
        _ => {
            // `use config::parse_config;` beside `mod config;` is this crate's
            // module (edition-2018 uniform paths / edition 2015). Without that
            // declaration the root names a dependency — `use log::info;` stays
            // External even in a repo that happens to contain a `src/log.rs`.
            let in_repo = if files.declares_module(from_file, root) {
                resolve_in_repo(
                    &segments,
                    no_drop,
                    crate_root(from_file, files).as_deref(),
                    files,
                )
            } else {
                // …or a sibling crate in the same workspace (`b::util::X`).
                sibling_crate_root(root, files)
                    .and_then(|anchor| resolve_in_repo(after, no_drop, Some(&anchor), files))
            };
            return in_repo.unwrap_or_else(|| ImportTarget::External(import.module_path.clone()));
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
/// matching the scanned file set, optionally restricted to one crate root.
fn resolve_in_repo(
    module_segs: &[&str],
    no_drop: bool,
    anchor: Option<&str>,
    files: &FileSet,
) -> Option<ImportTarget> {
    for cand in module_candidates(module_segs, no_drop) {
        if let Some(idx) = suffix_match(cand, anchor, files) {
            return Some(ImportTarget::File(idx));
        }
    }
    None
}

/// Probe a module path directly against a known base directory, `foo.rs` before
/// `foo/mod.rs`. Exact, so it beats any same-named file elsewhere in the repo.
fn probe_from(
    base: &str,
    module_segs: &[&str],
    no_drop: bool,
    files: &FileSet,
) -> Option<ImportTarget> {
    for cand in module_candidates(module_segs, no_drop) {
        let joined = cand.join("/");
        for rel in [format!("{joined}.rs"), format!("{joined}/mod.rs")] {
            if let Some(idx) = files.probe(base, &rel) {
                return Some(ImportTarget::File(idx));
            }
        }
    }
    None
}

/// The crate root the file belongs to: the path up to and including its nearest
/// `src` ancestor (`/repo/crates/a/src/x/y.rs` → `/repo/crates/a/src`). `None` when
/// the file is not under a `src` directory — then `crate::` matches are unanchored.
fn crate_root(from_file: usize, files: &FileSet) -> Option<String> {
    let path = normalize_path(files.path(from_file)?);
    if let Some(i) = path.rfind("/src/") {
        return Some(path[..i + 4].to_string());
    }
    if path.starts_with("src/") {
        return Some("src".to_string());
    }
    None
}

/// The crate root of a scanned sibling crate literally named `name`
/// (`.../crates/b/src` for `b`). `None` when no scanned file lives under such a
/// directory, or when several distinct roots claim the name — the signal that the
/// root segment is an outside dependency (`log`, `serde`) rather than a workspace
/// member, so the caller can classify it External without guessing.
fn sibling_crate_root(name: &str, files: &FileSet) -> Option<String> {
    let marker = format!("/{name}/src/");
    let mut root: Option<String> = None;
    for i in 0..files.len() {
        let Some(raw) = files.path(i) else { continue };
        let path = normalize_path(raw);
        let Some(at) = path.find(&marker) else {
            continue;
        };
        let candidate = path[..at + marker.len() - 1].to_string();
        match &root {
            Some(existing) if *existing != candidate => return None,
            Some(_) => {}
            None => root = Some(candidate),
        }
    }
    root
}

/// The directory that a file's own module owns. For a module-root file
/// (`mod.rs`/`lib.rs`/`main.rs`) that is its containing directory; for a
/// `foo.rs`-style module (Rust 2018) it is `<dir>/foo`. `self::x` lives in this
/// directory and each `super` climbs one level above it.
fn module_dir(from_file: usize, files: &FileSet) -> Option<String> {
    let path = normalize_path(files.path(from_file)?);
    let dir = parent_dir(&path);
    let stem = path.rsplit('/').next().unwrap_or_default();
    let stem = stem.strip_suffix(".rs").unwrap_or(stem);
    if matches!(stem, "mod" | "lib" | "main") {
        return Some(dir);
    }
    Some(match dir.as_str() {
        "" => stem.to_string(),
        "/" => format!("/{stem}"),
        d => format!("{d}/{stem}"),
    })
}

/// Resolve a `self`/`super` path by probing relative to the importing file's
/// module directory, walking `up_hops` directories up first (one per `super`).
fn resolve_relative(
    module_segs: &[&str],
    no_drop: bool,
    from_file: usize,
    up_hops: usize,
    files: &FileSet,
) -> Option<ImportTarget> {
    let mut base = module_dir(from_file, files)?;
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
/// to pick when a convention matches more than one file (never guess). When
/// `anchor` is set, only files under that crate root are eligible.
fn suffix_match(segs: &[&str], anchor: Option<&str>, files: &FileSet) -> Option<usize> {
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
                if anchor.is_some_and(|root| !np.starts_with(&format!("{root}/"))) {
                    continue;
                }
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
    fn super_resolves_to_the_parent_module_not_the_parent_directory() {
        // `/repo/src/parent/child.rs` is module `crate::parent::child`, so
        // `super::x` is `crate::parent::x` — the SIBLING file, not `/repo/src/x.rs`.
        let files = rust_files(&[
            "/repo/src/x.rs",
            "/repo/src/parent/x.rs",
            "/repo/src/parent/child.rs",
        ]);
        let fs = FileSet::new(&files);
        let import = imp("super::x::Y", "/repo/src/parent/child.rs");
        assert_eq!(resolve_rust_import(&import, 2, &fs), ImportTarget::File(1));
    }

    #[test]
    fn super_from_mod_rs_climbs_a_real_directory() {
        // For a module-root file the module directory IS the containing directory.
        let files = rust_files(&["/repo/src/x.rs", "/repo/src/parent/mod.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("super::x::Y", "/repo/src/parent/mod.rs");
        assert_eq!(resolve_rust_import(&import, 1, &fs), ImportTarget::File(0));
    }

    #[test]
    fn super_super_climbs_two_module_levels() {
        let files = rust_files(&["/repo/src/x.rs", "/repo/src/a/b/c.rs"]);
        let fs = FileSet::new(&files);
        // c.rs is `crate::a::b::c`; `super::super::x` is `crate::a::x`… which does
        // not exist here, while `/repo/src/x.rs` is three levels up — Unresolved.
        assert!(matches!(
            resolve_rust_import(&imp("super::super::x::Y", "/repo/src/a/b/c.rs"), 1, &fs),
            ImportTarget::Unresolved(_)
        ));
    }

    #[test]
    fn self_resolves_submodule_of_a_module_root_file() {
        // `use self::child::Thing;` from lib.rs → a submodule file beside it.
        let files = rust_files(&["/repo/src/a/lib.rs", "/repo/src/a/child.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("self::child::Thing", "/repo/src/a/lib.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn self_resolves_into_the_module_directory_not_the_sibling() {
        // In `/repo/src/a/foo.rs` (module `a::foo`), `self::child` is
        // `a::foo::child` = `/repo/src/a/foo/child.rs`, not `/repo/src/a/child.rs`.
        let files = rust_files(&[
            "/repo/src/a/foo.rs",
            "/repo/src/a/child.rs",
            "/repo/src/a/foo/child.rs",
        ]);
        let fs = FileSet::new(&files);
        let import = imp("self::child::Thing", "/repo/src/a/foo.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(2));
    }

    #[test]
    fn aliased_module_import_resolves() {
        // `use crate::config as cfg;` — the alias renames the binding, not the path.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/config.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::config as cfg", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn local_module_never_shadows_an_external_crate() {
        // `use log::info;` is the `log` CRATE even though `src/log.rs` exists —
        // an in-repo module would have to be reached as `crate::log`.
        let files = rust_files(&["/repo/src/main.rs", "/repo/src/log.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("log::info", "/repo/src/main.rs");
        assert_eq!(
            resolve_rust_import(&import, 0, &fs),
            ImportTarget::External("log::info".to_string())
        );
    }

    #[test]
    fn crate_path_probes_the_crate_root_before_matching_by_suffix() {
        // `crate::types` is `src/types/mod.rs`; an unrelated deeper `types.rs` must
        // not win just because the `foo.rs` convention is tried first.
        let files = rust_files(&[
            "/repo/src/main.rs",
            "/repo/src/core/types.rs",
            "/repo/src/types/mod.rs",
        ]);
        let fs = FileSet::new(&files);
        let import = imp("crate::types::Thing", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(2));
    }

    #[test]
    fn declared_module_makes_a_bare_root_in_repo() {
        // `mod config;` + `use config::parse_config;` at a crate root — the same
        // shape as `log::info`, told apart by the declaration.
        let mut files = rust_files(&["/repo/src/main.rs", "/repo/src/config.rs"]);
        files[0].declared_modules = vec!["config".to_string()];
        let fs = FileSet::new(&files);
        let import = imp("config::parse_config", "/repo/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn sibling_workspace_crate_resolves_by_crate_directory() {
        // `use b::util::X;` from crate `a` → crate `b`'s file, because the root
        // segment names the crate directory.
        let files = rust_files(&["/repo/crates/a/src/main.rs", "/repo/crates/b/src/util.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("b::util::X", "/repo/crates/a/src/main.rs");
        assert_eq!(resolve_rust_import(&import, 0, &fs), ImportTarget::File(1));
    }

    #[test]
    fn crate_path_does_not_cross_into_another_workspace_crate() {
        // `crate::util` in crate `a` means `a`'s own `util`; `b`'s must not match.
        let files = rust_files(&["/repo/crates/a/src/main.rs", "/repo/crates/b/src/util.rs"]);
        let fs = FileSet::new(&files);
        let import = imp("crate::util::X", "/repo/crates/a/src/main.rs");
        assert!(matches!(
            resolve_rust_import(&import, 0, &fs),
            ImportTarget::Unresolved(_)
        ));
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
        // `crate::deep::util` is not at the crate root, and two files inside the
        // crate end with `/deep/util.rs` — the fallback must not pick one.
        let files = rust_files(&[
            "/repo/src/main.rs",
            "/repo/src/a/deep/util.rs",
            "/repo/src/b/deep/util.rs",
        ]);
        let fs = FileSet::new(&files);
        let import = imp("crate::deep::util::X", "/repo/src/main.rs");
        assert!(matches!(
            resolve_rust_import(&import, 0, &fs),
            ImportTarget::Unresolved(_)
        ));
    }
}
