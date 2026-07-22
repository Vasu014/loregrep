//! Resolution of agent-supplied paths against the analysis root.
//!
//! Every tool parameter that names a file is untrusted input: it arrives from a
//! model, which may in turn be relaying text from a README, an issue comment or
//! a dependency's source. Two properties must therefore hold for every such
//! parameter, and they must hold in ONE place rather than per handler:
//!
//! 1. **Resolve against the analysis root, never the process working
//!    directory.** Tools compose — the output of one is fed to another — so a
//!    path that works in one tool must work in its siblings regardless of where
//!    the caller's shell happens to stand.
//! 2. **Refuse anything outside the root.** A code-analysis tool that will read
//!    an arbitrary absolute path, and hand back its contents, is a file-read
//!    primitive with an analysis-shaped alibi.
//!
//! Both were learned the hard way: `analyze_file` resolved its `file_path`
//! against the process cwd and read it with no containment check, so
//! `{"file_path": "/etc/passwd", "include_content": true}` returned the file.
//! Every other tool was safe purely because it happened to route through the
//! index. Safety that depends on each handler remembering to use the right
//! helper is a convention; this module exists to make it an invariant.

use std::path::{Component, Path, PathBuf};

/// Why a supplied path could not be used. Each variant carries enough context
/// for the message to name the path AS RESOLVED — an error that says only
/// "not found" teaches an agent nothing and costs a retry it cannot aim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The path resolved outside the analysis root.
    OutsideRoot { resolved: String, root: String },
    /// The path resolved inside the root but nothing is there.
    NotFound { resolved: String, root: String },
    /// No analysis root is known, so containment cannot be enforced.
    NoRoot { input: String },
}

impl PathError {
    /// A message that names what was tried, so the next attempt can be aimed.
    pub fn message(&self) -> String {
        match self {
            PathError::OutsideRoot { resolved, root } => format!(
                "path is outside the analysis root: {resolved} (root: {root}). \
                 loregrep only reads files inside the directory it was pointed at"
            ),
            PathError::NotFound { resolved, root } => format!(
                "file not found: {resolved} (resolved relative to the analysis root {root}). \
                 hint: pass a path as returned by get_repository_tree or search_functions"
            ),
            PathError::NoRoot { input } => format!(
                "cannot resolve {input:?}: no analysis root is known for this index, \
                 and the path is not in it. Re-scan the repository, or pass a path \
                 exactly as returned by another tool"
            ),
        }
    }
}

/// Collapse `.` and `..` lexically, without touching the filesystem.
///
/// Deliberately NOT `fs::canonicalize` for the traversal step: canonicalize
/// fails on paths that do not exist, and we need to reject `../../../etc/passwd`
/// with a clear message whether or not it happens to exist.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    // A `..` that escapes the start is preserved, so containment
                    // below sees it and refuses rather than silently clamping.
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve `input` against `root` and require the result to stay inside it.
///
/// Relative inputs are joined onto the root (NOT the process cwd). Absolute
/// inputs are accepted only if they already live under the root. Symlinks are
/// resolved where the path exists, so a link inside the root pointing outside it
/// is refused rather than followed.
pub fn resolve_within_root(root: &Path, input: &str) -> Result<PathBuf, PathError> {
    let root_real = std::fs::canonicalize(root).unwrap_or_else(|_| lexically_normalize(root));
    let candidate = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        root_real.join(input)
    };
    let normalized = lexically_normalize(&candidate);

    // Resolve symlinks when we can; fall back to the lexical form so a missing
    // file still produces a NotFound naming the path we actually tried.
    let resolved = std::fs::canonicalize(&normalized).unwrap_or(normalized);

    if !resolved.starts_with(&root_real) {
        return Err(PathError::OutsideRoot {
            resolved: resolved.display().to_string(),
            root: root_real.display().to_string(),
        });
    }
    if !resolved.exists() {
        return Err(PathError::NotFound {
            resolved: resolved.display().to_string(),
            root: root_real.display().to_string(),
        });
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        let root = dir.path().to_path_buf();
        (dir, root)
    }

    #[test]
    fn relative_input_resolves_against_the_root_not_the_cwd() {
        let (_d, root) = fixture();
        let resolved = resolve_within_root(&root, "src/main.rs").unwrap();
        assert!(resolved.ends_with("src/main.rs"));
        // The cwd during tests is the crate root, which also has src/main.rs —
        // so a cwd-resolving implementation would pass this by accident. Assert
        // the resolved path is under the fixture root, which it never would be.
        assert!(resolved.starts_with(std::fs::canonicalize(&root).unwrap()));
    }

    #[test]
    fn leading_dot_slash_is_accepted() {
        let (_d, root) = fixture();
        assert!(resolve_within_root(&root, "./src/main.rs").is_ok());
    }

    #[test]
    fn absolute_path_inside_the_root_is_accepted() {
        let (_d, root) = fixture();
        let abs = root.join("src/main.rs");
        assert!(resolve_within_root(&root, abs.to_str().unwrap()).is_ok());
    }

    #[test]
    fn absolute_path_outside_the_root_is_refused() {
        let (_d, root) = fixture();
        let err = resolve_within_root(&root, "/etc/hosts").unwrap_err();
        assert!(matches!(err, PathError::OutsideRoot { .. }), "{err:?}");
        assert!(err.message().contains("outside the analysis root"));
        assert!(err.message().contains("/etc/hosts"));
    }

    #[test]
    fn parent_traversal_out_of_the_root_is_refused() {
        let (_d, root) = fixture();
        let err = resolve_within_root(&root, "../../../../etc/hosts").unwrap_err();
        assert!(matches!(err, PathError::OutsideRoot { .. }), "{err:?}");
    }

    #[test]
    fn traversal_that_returns_inside_the_root_is_allowed() {
        let (_d, root) = fixture();
        assert!(resolve_within_root(&root, "src/../src/main.rs").is_ok());
    }

    #[test]
    fn missing_file_inside_the_root_names_the_resolved_path() {
        let (_d, root) = fixture();
        let err = resolve_within_root(&root, "src/nope.rs").unwrap_err();
        match &err {
            PathError::NotFound { resolved, .. } => assert!(resolved.ends_with("src/nope.rs")),
            other => panic!("expected NotFound, got {other:?}"),
        }
        // INV-4: the message must carry the path as resolved, not a bare
        // "not found" the agent cannot act on.
        assert!(err.message().contains("src/nope.rs"));
        assert!(err.message().contains("get_repository_tree"));
    }

    #[test]
    fn a_symlink_pointing_out_of_the_root_is_refused() {
        let (_d, root) = fixture();
        let link = root.join("escape.rs");
        if std::os::unix::fs::symlink("/etc/hosts", &link).is_ok() {
            let err = resolve_within_root(&root, "escape.rs").unwrap_err();
            assert!(matches!(err, PathError::OutsideRoot { .. }), "{err:?}");
        }
    }
}
