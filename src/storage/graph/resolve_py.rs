//! Python import resolver (P2-3).
//!
//! Maps a parsed [`ImportStatement`] to exactly one [`ImportTarget`], heuristically
//! and honestly — a resolved in-repo `File`, a terminal `External` (site-packages /
//! stdlib), or `Unresolved` (carried, never guessed).
//!
//! Two shapes are handled, distinguished by a leading dot on `module_path`:
//!
//! 1. **Relative** (`.x`, `..pkg.y`): the leading dots are the level. One dot is the
//!    importing file's own package (its directory); each extra dot climbs one more
//!    directory. The remainder splits on `.` into path segments, resolved against the
//!    level-adjusted directory, probing `<path>.py` then `<path>/__init__.py`.
//!
//! 2. **Absolute dotted** (`a.b.c`): the package-root prefix inside the repo is
//!    unknown, so we suffix-match the scanned file set for `a/b/c.py` or
//!    `a/b/c/__init__.py`. Exactly one match → `File`; none → `External` (it is a
//!    terminal dependency); more than one → `Unresolved` (ambiguous, never guessed).
//!
//! NOTE on the analyzer (verified empirically, see the `probe` test): the Python
//! analyzer does NOT preserve the leading-dot level for relative imports — for
//! `from . import x` / `from ..pkg import y` its `module_path` is just the imported
//! name (`x` / `y`), dots stripped, `is_external=false`. So in practice relative
//! imports arrive here as bare names and fall through to the absolute suffix-match
//! path (rule 2). This resolver nonetheless implements the full dotted-relative logic
//! (rule 1) for callers/tests that do carry the dots; we do not extend the analyzer.

use super::{FileSet, ImportTarget, normalize_path};
use crate::types::ImportStatement;

pub fn resolve_py_import(
    import: &ImportStatement,
    from_file: usize,
    files: &FileSet,
) -> ImportTarget {
    let module_path = import.module_path.trim();
    if module_path.is_empty() {
        return ImportTarget::Unresolved(import.module_path.clone());
    }

    if module_path.starts_with('.') {
        resolve_relative(module_path, from_file, files)
            .map(ImportTarget::File)
            .unwrap_or_else(|| ImportTarget::Unresolved(import.module_path.clone()))
    } else if !module_path.contains('.') && !import.is_external {
        // A single-segment in-repo specifier is (per the NOTE above) most often a
        // relative import the analyzer stripped the dots from — the segment is the
        // imported NAME, not a module path. Matching that name against the whole
        // repo would happily pick some unrelated `<name>.py`, so probe only the
        // importer's own package and stay honest otherwise.
        resolve_relative(&format!(".{module_path}"), from_file, files)
            .map(ImportTarget::File)
            .unwrap_or_else(|| ImportTarget::Unresolved(import.module_path.clone()))
    } else {
        resolve_absolute(module_path, files)
    }
}

/// Rule 1: relative import. Returns the resolved file index, or `None`.
fn resolve_relative(module_path: &str, from_file: usize, files: &FileSet) -> Option<usize> {
    let level = module_path.chars().take_while(|&c| c == '.').count();
    let remainder = &module_path[level..];

    let dir = files.dir_of(from_file)?;
    // One dot = the importing file's own directory; each extra dot climbs one more.
    let ups = "../".repeat(level - 1);

    let seg_path: String = remainder
        .split('.')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/");

    // Candidate relative specifiers, first hit wins.
    let candidates: Vec<String> = if seg_path.is_empty() {
        // `from . import x` with the name folded away → the package's own __init__.
        vec![format!("{ups}__init__.py")]
    } else {
        vec![
            format!("{ups}{seg_path}.py"),
            format!("{ups}{seg_path}/__init__.py"),
        ]
    };

    candidates.iter().find_map(|rel| files.probe(&dir, rel))
}

/// Rule 2: absolute dotted import. Suffix-match the scanned set; classify External
/// when nothing matches (terminal dependency) and Unresolved when ambiguous.
fn resolve_absolute(module_path: &str, files: &FileSet) -> ImportTarget {
    let segs: Vec<&str> = module_path.split('.').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return ImportTarget::External(module_path.to_string());
    }

    // `from pkg.mod import Symbol` reaches us as `pkg.mod.Symbol` (the analyzer
    // folds the imported name into the path), so the full path matches no file.
    // Try it first, then drop trailing segments — the first prefix that matches a
    // scanned file is the module. Stop before the single-segment case: a lone name
    // is too weak a needle to match repo-wide (that is the ambiguity the caller
    // routes away from).
    let mut n = segs.len();
    while n >= 2 {
        match match_dotted(&segs[..n], module_path, files) {
            ImportTarget::External(_) => n -= 1,
            hit => return hit,
        }
    }
    match_dotted(&segs[..1], module_path, files)
}

/// Suffix-match one dotted-path interpretation against the scanned set.
fn match_dotted(segs: &[&str], module_path: &str, files: &FileSet) -> ImportTarget {
    let seg_path = segs.join("/");
    let module_suffix = format!("{seg_path}.py");
    let package_suffix = format!("{seg_path}/__init__.py");

    let mut matches: Vec<usize> = Vec::new();
    for idx in 0..files.len() {
        let path = match files.path(idx) {
            Some(p) => normalize_path(p),
            None => continue,
        };
        if path_ends_with_segment(&path, &module_suffix)
            || path_ends_with_segment(&path, &package_suffix)
        {
            matches.push(idx);
        }
    }

    match matches.as_slice() {
        [] => ImportTarget::External(module_path.to_string()),
        [only] => ImportTarget::File(*only),
        // More than one candidate — don't guess between them.
        _ => ImportTarget::Unresolved(module_path.to_string()),
    }
}

/// True if `path` equals `suffix` or ends with `/<suffix>` — a whole-segment suffix
/// match (so `pkg/mod.py` matches `.../pkg/mod.py` but not `.../mypkg/mod.py`).
fn path_ends_with_segment(path: &str, suffix: &str) -> bool {
    path == suffix || path.ends_with(&format!("/{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::graph::FileSet;
    use crate::types::{ImportStatement, TreeNode};

    fn py(path: &str) -> TreeNode {
        TreeNode::new(path.to_string(), "python".to_string())
    }

    fn imp(module_path: &str, from_path: &str, is_external: bool) -> ImportStatement {
        ImportStatement::new(module_path.to_string(), from_path.to_string())
            .with_external(is_external)
    }

    // Absolute dotted `pkg.mod` → the scanned `.../pkg/mod.py`.
    #[test]
    fn absolute_dotted_resolves_to_module_file() {
        let files = vec![py("/repo/main.py"), py("/repo/pkg/mod.py")];
        let fs = FileSet::new(&files);
        let statement = imp("pkg.mod", "/repo/main.py", true);
        assert_eq!(resolve_py_import(&statement, 0, &fs), ImportTarget::File(1));
    }

    // Package import `pkg` → resolved via `.../pkg/__init__.py`.
    #[test]
    fn package_import_resolves_via_init() {
        let files = vec![py("/repo/app.py"), py("/repo/pkg/__init__.py")];
        let fs = FileSet::new(&files);
        let statement = imp("pkg", "/repo/app.py", true);
        assert_eq!(resolve_py_import(&statement, 0, &fs), ImportTarget::File(1));
    }

    // `from . import x` (level 1) → the sibling `x.py`.
    #[test]
    fn relative_level_one_resolves_sibling() {
        let files = vec![py("/repo/pkg/sub/mod.py"), py("/repo/pkg/sub/x.py")];
        let fs = FileSet::new(&files);
        let statement = imp(".x", "/repo/pkg/sub/mod.py", false);
        assert_eq!(resolve_py_import(&statement, 0, &fs), ImportTarget::File(1));
    }

    // `from ..pkg import y` (level 2) → resolved up one package into `pkg/y.py`.
    #[test]
    fn relative_level_two_resolves_up() {
        let files = vec![py("/repo/proj/sub/mod.py"), py("/repo/proj/pkg/y.py")];
        let fs = FileSet::new(&files);
        let statement = imp("..pkg.y", "/repo/proj/sub/mod.py", false);
        assert_eq!(resolve_py_import(&statement, 0, &fs), ImportTarget::File(1));
    }

    // `numpy` — no scanned match → terminal External.
    #[test]
    fn third_party_with_no_match_is_external() {
        let files = vec![py("/repo/app.py")];
        let fs = FileSet::new(&files);
        let statement = imp("numpy", "/repo/app.py", true);
        assert_eq!(
            resolve_py_import(&statement, 0, &fs),
            ImportTarget::External("numpy".to_string())
        );
    }

    // A relative import with no matching file → Unresolved, never a wrong File.
    #[test]
    fn unresolvable_relative_is_unresolved_never_wrong_file() {
        let files = vec![py("/repo/proj/sub/mod.py"), py("/repo/proj/pkg/y.py")];
        let fs = FileSet::new(&files);
        let statement = imp("..missing.thing", "/repo/proj/sub/mod.py", false);
        assert_eq!(
            resolve_py_import(&statement, 0, &fs),
            ImportTarget::Unresolved("..missing.thing".to_string())
        );
    }

    // Ambiguous absolute suffix-match (two candidates) → Unresolved, never a guess.
    #[test]
    fn ambiguous_absolute_match_is_unresolved() {
        let files = vec![
            py("/repo/a/pkg/mod.py"),
            py("/repo/b/pkg/mod.py"),
            py("/repo/main.py"),
        ];
        let fs = FileSet::new(&files);
        let statement = imp("pkg.mod", "/repo/main.py", true);
        assert_eq!(
            resolve_py_import(&statement, 2, &fs),
            ImportTarget::Unresolved("pkg.mod".to_string())
        );
    }

    // `from pkg.mod import Symbol` reaches the resolver as `pkg.mod.Symbol` (the
    // analyzer folds the name in); the trailing symbol must be trimmed off.
    #[test]
    fn trailing_symbol_segment_is_trimmed_to_the_module() {
        let files = vec![py("/repo/pkg/mod_a.py"), py("/repo/pkg/mod_b.py")];
        let fs = FileSet::new(&files);
        let statement = imp("pkg.mod_b.helper", "/repo/pkg/mod_a.py", false);
        assert_eq!(resolve_py_import(&statement, 0, &fs), ImportTarget::File(1));
    }

    // …but trimming must not turn a genuine stdlib import into a repo file.
    #[test]
    fn trimming_does_not_invent_a_file_for_stdlib() {
        let files = vec![py("/repo/app.py")];
        let fs = FileSet::new(&files);
        let statement = imp("typing.List", "/repo/app.py", true);
        assert_eq!(
            resolve_py_import(&statement, 0, &fs),
            ImportTarget::External("typing.List".to_string())
        );
    }

    // A dot-stripped relative import (`from .mod_b import helper` → `helper`) must
    // never be matched repo-wide: that picks an unrelated same-named file.
    #[test]
    fn dot_stripped_relative_never_matches_an_unrelated_file() {
        let files = vec![
            py("/repo/pkg/mod_a.py"),
            py("/repo/pkg/mod_b.py"),
            py("/repo/other/helper.py"),
        ];
        let fs = FileSet::new(&files);
        let statement = imp("helper", "/repo/pkg/mod_a.py", false);
        assert_eq!(
            resolve_py_import(&statement, 0, &fs),
            ImportTarget::Unresolved("helper".to_string())
        );
    }

    // The same shape DOES resolve when the name really is a sibling module.
    #[test]
    fn dot_stripped_relative_resolves_against_own_package() {
        let files = vec![py("/repo/pkg/mod_a.py"), py("/repo/pkg/sibling.py")];
        let fs = FileSet::new(&files);
        let statement = imp("sibling", "/repo/pkg/mod_a.py", false);
        assert_eq!(resolve_py_import(&statement, 0, &fs), ImportTarget::File(1));
    }

    // Suffix matching is whole-segment: `pkg/mod.py` must not match `mypkg/mod.py`.
    #[test]
    fn suffix_match_is_whole_segment() {
        let files = vec![py("/repo/mypkg/mod.py"), py("/repo/main.py")];
        let fs = FileSet::new(&files);
        let statement = imp("pkg.mod", "/repo/main.py", true);
        // No genuine `pkg/mod.py` → terminal External, not a false File.
        assert_eq!(
            resolve_py_import(&statement, 1, &fs),
            ImportTarget::External("pkg.mod".to_string())
        );
    }
}
