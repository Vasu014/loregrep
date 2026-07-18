//! TypeScript/TSX import resolver (P2-2).
//!
//! Decides relative-vs-bare from the specifier string itself (`./`, `../`) rather
//! than trusting the analyzer's `is_external` flag, though the two agree.
//!
//! - **Relative** (`./x`, `../a/b`): resolved as a file path, probed against the
//!   scanned set from the importing file's directory. Probe order:
//!   `<spec>.ts`, `<spec>.tsx`, `<spec>/index.ts`, `<spec>/index.tsx`, then
//!   `<spec>` verbatim (covers specifiers that already carry an extension). First
//!   hit → `File`; none → `Unresolved` (carried, never guessed).
//! - **`@`-prefixed** (`@app/thing`, `@/utils`): potential tsconfig path alias →
//!   `Unresolved`. v1 deliberately does not distinguish real scoped npm packages
//!   (also `@`-prefixed); the Unresolved status makes the gap visible in evals.
//! - **Any other bare specifier** (`react`, `lodash`, `node:fs`): `External`
//!   (terminal, never traversed).

use super::{FileSet, ImportTarget};
use crate::types::ImportStatement;

pub fn resolve_ts_import(
    import: &ImportStatement,
    from_file: usize,
    files: &FileSet,
) -> ImportTarget {
    let spec = &import.module_path;

    // Relative specifier: resolve as a file path against the importer's directory.
    if spec.starts_with("./") || spec.starts_with("../") {
        let dir = files.dir_of(from_file).unwrap_or_default();
        // `join_normalized` (used inside `probe`) collapses a leading `./`, so we
        // can pass these candidates verbatim.
        let candidates = [
            format!("{spec}.ts"),
            format!("{spec}.tsx"),
            format!("{spec}/index.ts"),
            format!("{spec}/index.tsx"),
            spec.clone(),
        ];
        for cand in &candidates {
            if let Some(idx) = files.probe(&dir, cand) {
                return ImportTarget::File(idx);
            }
        }
        return ImportTarget::Unresolved(spec.clone());
    }

    // `@`-prefixed bare specifier: potential tsconfig path alias, parked.
    if spec.starts_with('@') {
        return ImportTarget::Unresolved(spec.clone());
    }

    // Any other bare specifier: an external dependency.
    ImportTarget::External(spec.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TreeNode;

    /// Build a `FileSet` over hand-made TS files (paths only; contents irrelevant
    /// to resolution).
    fn ts_file(path: &str) -> TreeNode {
        TreeNode::new(path.to_string(), "typescript".to_string())
    }

    /// Construct an import statement as the analyzer would: `is_external` true for
    /// bare specifiers, false for relative ones.
    fn import(spec: &str, from_path: &str, is_bare: bool) -> ImportStatement {
        ImportStatement::new(spec.to_string(), from_path.to_string()).with_external(is_bare)
    }

    /// Resolve `spec` imported from the file at index `from` within `files`.
    fn resolve(files: &[TreeNode], from: usize, spec: &str, is_bare: bool) -> ImportTarget {
        let fs = FileSet::new(files);
        let from_path = files[from].file_path.clone();
        resolve_ts_import(&import(spec, &from_path, is_bare), from, &fs)
    }

    #[test]
    fn relative_resolves_to_sibling_ts() {
        // `./x` from src/a.ts -> src/x.ts
        let files = vec![ts_file("src/a.ts"), ts_file("src/x.ts")];
        assert_eq!(resolve(&files, 0, "./x", false), ImportTarget::File(1));
    }

    #[test]
    fn relative_resolves_via_index_ts() {
        // `./x` from src/a.ts -> src/x/index.ts (directory index probing)
        let files = vec![ts_file("src/a.ts"), ts_file("src/x/index.ts")];
        assert_eq!(resolve(&files, 0, "./x", false), ImportTarget::File(1));
    }

    #[test]
    fn relative_resolves_via_tsx() {
        let files = vec![ts_file("src/a.ts"), ts_file("src/x.tsx")];
        assert_eq!(resolve(&files, 0, "./x", false), ImportTarget::File(1));
    }

    #[test]
    fn relative_resolves_via_index_tsx() {
        let files = vec![ts_file("src/a.ts"), ts_file("src/x/index.tsx")];
        assert_eq!(resolve(&files, 0, "./x", false), ImportTarget::File(1));
    }

    #[test]
    fn parent_relative_resolves_across_dirs() {
        // `../a/b` from src/pkg/c.ts -> src/a/b.ts
        let files = vec![ts_file("src/pkg/c.ts"), ts_file("src/a/b.ts")];
        assert_eq!(resolve(&files, 0, "../a/b", false), ImportTarget::File(1));
    }

    #[test]
    fn specifier_with_explicit_extension_matches_verbatim() {
        let files = vec![ts_file("src/a.ts"), ts_file("src/x.ts")];
        assert_eq!(resolve(&files, 0, "./x.ts", false), ImportTarget::File(1));
    }

    #[test]
    fn bare_specifier_is_external() {
        let files = vec![ts_file("src/a.ts")];
        assert_eq!(
            resolve(&files, 0, "react", true),
            ImportTarget::External("react".to_string())
        );
    }

    #[test]
    fn node_builtin_is_external() {
        let files = vec![ts_file("src/a.ts")];
        assert_eq!(
            resolve(&files, 0, "node:fs", true),
            ImportTarget::External("node:fs".to_string())
        );
    }

    #[test]
    fn at_prefixed_specifier_is_unresolved_alias() {
        let files = vec![ts_file("src/a.ts")];
        assert_eq!(
            resolve(&files, 0, "@app/thing", true),
            ImportTarget::Unresolved("@app/thing".to_string())
        );
    }

    #[test]
    fn at_slash_specifier_is_unresolved_alias() {
        let files = vec![ts_file("src/a.ts")];
        assert_eq!(
            resolve(&files, 0, "@/utils", true),
            ImportTarget::Unresolved("@/utils".to_string())
        );
    }

    #[test]
    fn missing_relative_is_unresolved_never_file() {
        // `./missing` with no matching file must never guess a File.
        let files = vec![ts_file("src/a.ts"), ts_file("src/x.ts")];
        assert_eq!(
            resolve(&files, 0, "./missing", false),
            ImportTarget::Unresolved("./missing".to_string())
        );
    }

    #[test]
    fn ts_preferred_over_tsx_in_probe_order() {
        // Both src/x.ts and src/x.tsx exist; `.ts` is probed first.
        let files = vec![
            ts_file("src/a.ts"),
            ts_file("src/x.ts"),
            ts_file("src/x.tsx"),
        ];
        assert_eq!(resolve(&files, 0, "./x", false), ImportTarget::File(1));
    }
}
