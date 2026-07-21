//! Derived module graph: cross-file import relationships resolved from the
//! `ImportStatement`s already parsed per file.
//!
//! This is pure derived state — computed from `&[TreeNode]` after a scan or a
//! cache load, never serialized (persistence rebuilds it), and rebuilt wholesale
//! rather than patched per file (the same discipline that keeps the name-keyed
//! call graph honest: hand-patched derived state is what leaked in P0-1).
//!
//! Resolution is heuristic with **explicit uncertainty**: every import becomes
//! exactly one [`ImportTarget`] — a resolved in-repo `File`, an `External`
//! dependency (std / crates.io / node_modules / site-packages — terminal, never
//! traversed), or `Unresolved` (couldn't map it; carried, never dropped, never
//! guessed). Per-language resolvers live in the sibling modules and are filled in
//! by P2-2/P2-3/P2-4; until then they classify by the analyzer's `is_external`
//! flag so the graph is honest but coarse.

use crate::types::{ImportStatement, TreeNode};
use std::collections::{HashMap, HashSet};

mod resolve_py;
mod resolve_rust;
mod resolve_ts;

pub use resolve_py::resolve_py_import;
pub use resolve_rust::resolve_rust_import;
pub use resolve_ts::resolve_ts_import;

/// Where a single import points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportTarget {
    /// A file in the scanned repo, by its index into the file set.
    File(usize),
    /// A dependency outside the repo (std, a crate, a package). Terminal.
    External(String),
    /// Could not be mapped to a file — carried with its raw specifier so the gap
    /// is visible (in evals, in tool output), never a silent drop or guess.
    Unresolved(String),
}

/// One import edge with its resolution and enough raw context to render output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    /// The raw specifier as parsed (`./x`, `crate::a::b`, `react`).
    pub module_path: String,
    pub target: ImportTarget,
    pub line_number: u32,
}

/// Forward and reverse import adjacency over the scanned file set. Indices match
/// positions in the `&[TreeNode]` the graph was built from; they are ephemeral to
/// one build and must never be persisted or held across a rebuild.
#[derive(Debug, Clone, Default)]
pub struct ModuleGraph {
    /// `forward[i]` = the resolved imports declared by file `i`.
    pub forward: Vec<Vec<ResolvedImport>>,
    /// `reverse[i]` = indices of files that import file `i` (sorted, deduped).
    pub reverse: Vec<Vec<usize>>,
}

impl ModuleGraph {
    /// The resolved imports declared by file `idx`.
    pub fn imports(&self, idx: usize) -> &[ResolvedImport] {
        self.forward.get(idx).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The direct importers of file `idx`.
    pub fn importers(&self, idx: usize) -> &[usize] {
        self.reverse.get(idx).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Every file that transitively imports `idx` (reverse reachability), excluding
    /// `idx` itself. Cycle-safe.
    pub fn transitive_importers(&self, idx: usize) -> Vec<usize> {
        let mut seen: HashSet<usize> = HashSet::new();
        let mut frontier = vec![idx];
        while let Some(node) = frontier.pop() {
            for &importer in self.importers(node) {
                if seen.insert(importer) {
                    frontier.push(importer);
                }
            }
        }
        // A cycle through `idx` walks back to it; the contract excludes the file
        // itself, so a file is never reported as its own importer.
        seen.remove(&idx);
        let mut out: Vec<usize> = seen.into_iter().collect();
        out.sort_unstable();
        out
    }

    /// The resolved in-repo edges as `(from_file, to_file)` pairs (imports whose
    /// target is another scanned file; `External`/`Unresolved` are skipped).
    pub fn file_edges(&self) -> Vec<(usize, usize)> {
        let mut edges = Vec::new();
        for (from, imports) in self.forward.iter().enumerate() {
            for imp in imports {
                if let ImportTarget::File(to) = imp.target {
                    edges.push((from, to));
                }
            }
        }
        edges
    }

    /// Import cycles among the scanned files: each returned group is a set of files
    /// that mutually (transitively) import each other — a strongly-connected
    /// component of size ≥ 2, plus any file that imports itself. Node indices in
    /// each group are sorted; groups are sorted by their smallest member. Computed
    /// with Tarjan's SCC algorithm over the resolved file edges.
    pub fn cycles(&self) -> Vec<Vec<usize>> {
        let n = self.forward.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut self_loops: HashSet<usize> = HashSet::new();
        for (from, to) in self.file_edges() {
            if from == to {
                self_loops.insert(from);
            } else if to < n {
                adj[from].push(to);
            }
        }

        let mut sccs = tarjan_scc(&adj);
        // Keep only genuine cycles: multi-node SCCs, or single nodes with a self-loop.
        sccs.retain(|c| c.len() > 1 || (c.len() == 1 && self_loops.contains(&c[0])));
        for c in &mut sccs {
            c.sort_unstable();
        }
        sccs.sort_by_key(|c| c.first().copied().unwrap_or(0));
        sccs
    }
}

/// Tarjan's strongly-connected-components over an adjacency list, iterative to
/// avoid stack overflow on deep graphs. Returns every SCC (including singletons).
fn tarjan_scc(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0usize;
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    // Explicit DFS stack of (node, next-neighbor-cursor).
    for start in 0..n {
        if index[start] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some((v, ci)) = work.pop() {
            if ci == 0 {
                index[v] = next_index;
                low[v] = next_index;
                next_index += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if ci < adj[v].len() {
                // Re-push v to resume after handling the child.
                work.push((v, ci + 1));
                let w = adj[v][ci];
                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                // Done with v's neighbors; propagate low to parent (top of work).
                if let Some(&(parent, _)) = work.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(comp);
                }
            }
        }
    }
    sccs
}

/// Read-only view over the scanned files that resolvers query: path→index lookup,
/// per-file language and directory. Built once per graph build.
pub struct FileSet<'a> {
    files: &'a [TreeNode],
    by_path: HashMap<String, usize>,
}

impl<'a> FileSet<'a> {
    pub fn new(files: &'a [TreeNode]) -> Self {
        let by_path = files
            .iter()
            .enumerate()
            .map(|(i, f)| (normalize_path(&f.file_path), i))
            .collect();
        Self { files, by_path }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn language(&self, idx: usize) -> Option<&str> {
        self.files.get(idx).map(|f| f.language.as_str())
    }

    pub fn path(&self, idx: usize) -> Option<&str> {
        self.files.get(idx).map(|f| f.file_path.as_str())
    }

    /// True if file `idx` declares `mod <name>;` — the Rust resolver's evidence
    /// that a bare `use <name>::…` is in-repo rather than an external crate.
    pub fn declares_module(&self, idx: usize, name: &str) -> bool {
        self.files
            .get(idx)
            .is_some_and(|f| f.declared_modules.iter().any(|m| m == name))
    }

    /// The directory portion of file `idx`'s path (normalized, no trailing slash).
    pub fn dir_of(&self, idx: usize) -> Option<String> {
        self.path(idx).map(|p| parent_dir(&normalize_path(p)))
    }

    /// Look up a file by an exact (normalized) path.
    pub fn index_of(&self, path: &str) -> Option<usize> {
        self.by_path.get(&normalize_path(path)).copied()
    }

    /// Resolve `rel` (e.g. `./x`, `../a/b`) against `from_dir` and return the file
    /// index if that normalized path is in the set. The shared probing primitive
    /// for the relative-path resolvers.
    pub fn probe(&self, from_dir: &str, rel: &str) -> Option<usize> {
        self.index_of(&join_normalized(from_dir, rel))
    }
}

/// Build the module graph from the file set, dispatching each file's imports to
/// the resolver for its language. Unknown languages → every import `Unresolved`.
pub fn build_module_graph(files: &[TreeNode]) -> ModuleGraph {
    build_with(files, dispatch_resolve)
}

/// Dispatch one import to the per-language resolver. Kept separate from
/// [`build_with`] so tests can inject a stub resolver.
fn dispatch_resolve(import: &ImportStatement, from_file: usize, files: &FileSet) -> ImportTarget {
    match files.language(from_file) {
        Some("rust") => resolve_rust_import(import, from_file, files),
        Some("python") => resolve_py_import(import, from_file, files),
        Some("typescript") | Some("tsx") => resolve_ts_import(import, from_file, files),
        _ => ImportTarget::Unresolved(import.module_path.clone()),
    }
}

/// Split a Rust brace group into one import per member, so a statement naming
/// several modules (`use crate::{config, storage};`) yields an edge for each.
/// Everything else — other languages, nested groups, no group — passes through
/// unchanged, and the caller collapses expansions that resolve to the same target.
fn expand_import(language: Option<&str>, import: &ImportStatement) -> Vec<ImportStatement> {
    let single = || vec![import.clone()];
    if language != Some("rust") {
        return single();
    }
    let raw = import.module_path.trim();
    let (open, close) = match (raw.find('{'), raw.rfind('}')) {
        (Some(o), Some(c)) if c > o => (o, c),
        _ => return single(),
    };
    let inner = &raw[open + 1..close];
    // Nested groups (`use a::{b::{c, d}, e};`) are rare; don't half-parse them.
    if inner.contains('{') {
        return single();
    }
    let base = raw[..open].trim_end_matches(':').trim();
    if base.is_empty() {
        return single();
    }

    let members: Vec<ImportStatement> = inner
        .split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty() && *m != "*")
        .map(|m| {
            let mut expanded = import.clone();
            // `use a::{self, X};` — `self` names the base module itself.
            expanded.module_path = if m == "self" {
                base.to_string()
            } else {
                format!("{base}::{m}")
            };
            expanded
        })
        .collect();

    if members.is_empty() {
        single()
    } else {
        members
    }
}

/// The injectable core: resolve every file's imports via `resolve`, then derive
/// reverse adjacency from the forward edges. Reverse is rebuilt from scratch, so a
/// replaced file leaves no stale reverse edges (rebuild-all by construction).
pub fn build_with<F>(files: &[TreeNode], resolve: F) -> ModuleGraph
where
    F: Fn(&ImportStatement, usize, &FileSet) -> ImportTarget,
{
    let fs = FileSet::new(files);
    let mut forward: Vec<Vec<ResolvedImport>> = Vec::with_capacity(files.len());

    for (i, file) in files.iter().enumerate() {
        let mut edges: Vec<ResolvedImport> = Vec::with_capacity(file.imports.len());
        for import in &file.imports {
            // A Rust brace group can name several distinct modules
            // (`use crate::{config, storage};`), so one statement can carry more
            // than one edge. Each expansion keeps the ORIGINAL specifier as its
            // `module_path`, and expansions that land on the same target collapse
            // below — so the common `use crate::a::{X, Y};` stays a single entry.
            for member in expand_import(fs.language(i), import) {
                let edge = ResolvedImport {
                    module_path: import.module_path.clone(),
                    target: resolve(&member, i, &fs),
                    line_number: import.line_number,
                };
                if !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
        forward.push(edges);
    }

    let mut reverse: Vec<HashSet<usize>> = vec![HashSet::new(); files.len()];
    for (importer, edges) in forward.iter().enumerate() {
        for edge in edges {
            if let ImportTarget::File(target) = edge.target {
                if target < files.len() && target != importer {
                    reverse[target].insert(importer);
                }
            }
        }
    }
    let reverse = reverse
        .into_iter()
        .map(|s| {
            let mut v: Vec<usize> = s.into_iter().collect();
            v.sort_unstable();
            v
        })
        .collect();

    ModuleGraph { forward, reverse }
}

/// Normalize a path for lookup: forward slashes, collapse `.`/`..`, strip a leading
/// `./`. Deliberately string-based (not `std::path`) so joined probe candidates and
/// stored file paths compare identically regardless of host separators.
pub fn normalize_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let leading_slash = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(out.last(), Some(&s) if s != "..") {
                    out.pop();
                } else if !leading_slash {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if leading_slash {
        format!("/{joined}")
    } else {
        joined
    }
}

/// The parent directory of a normalized path (empty string for a top-level file).
pub fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// Join a relative specifier onto a base directory and normalize the result.
pub fn join_normalized(base_dir: &str, rel: &str) -> String {
    if base_dir.is_empty() {
        normalize_path(rel)
    } else {
        normalize_path(&format!("{base_dir}/{rel}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, lang: &str, imports: &[&str]) -> TreeNode {
        let mut node = TreeNode::new(path.to_string(), lang.to_string());
        for (n, spec) in imports.iter().enumerate() {
            let mut imp = ImportStatement::new(spec.to_string(), path.to_string());
            imp.line_number = n as u32 + 1;
            node.imports.push(imp);
        }
        node
    }

    // Stub resolver: `./<name>` resolves to a same-directory file `<name>.x`,
    // anything else is Unresolved. Enough to exercise adjacency + carry.
    fn stub(import: &ImportStatement, from: usize, fs: &FileSet) -> ImportTarget {
        let spec = &import.module_path;
        if let Some(rel) = spec.strip_prefix("./") {
            let dir = fs.dir_of(from).unwrap_or_default();
            if let Some(idx) = fs.probe(&dir, &format!("{rel}.x")) {
                return ImportTarget::File(idx);
            }
        }
        ImportTarget::Unresolved(spec.clone())
    }

    #[test]
    fn forward_and_reverse_are_consistent() {
        // a imports ./b and an external; b imports nothing.
        let files = vec![
            file("src/a.x", "stub", &["./b", "vendor"]),
            file("src/b.x", "stub", &[]),
        ];
        let g = build_with(&files, stub);

        // forward: a has two edges (one File(1), one Unresolved); b has none.
        assert_eq!(g.imports(0).len(), 2);
        assert_eq!(g.imports(0)[0].target, ImportTarget::File(1));
        assert_eq!(
            g.imports(0)[1].target,
            ImportTarget::Unresolved("vendor".to_string())
        );
        assert!(g.imports(1).is_empty());

        // reverse: b is imported by a; a by no one.
        assert_eq!(g.importers(1), &[0]);
        assert!(g.importers(0).is_empty());
    }

    #[test]
    fn unresolved_imports_are_carried_never_dropped() {
        let files = vec![file("x.x", "stub", &["./missing", "alsoexternal"])];
        let g = build_with(&files, stub);
        assert_eq!(g.imports(0).len(), 2, "both imports retained");
        assert!(
            g.imports(0)
                .iter()
                .all(|e| matches!(e.target, ImportTarget::Unresolved(_)))
        );
    }

    #[test]
    fn rebuild_after_file_replace_leaves_no_stale_reverse_edges() {
        // Build 1: a -> b.
        let g1 = build_with(
            &[
                file("src/a.x", "stub", &["./b"]),
                file("src/b.x", "stub", &[]),
            ],
            stub,
        );
        assert_eq!(g1.importers(1), &[0]);

        // Build 2: a no longer imports b. A full rebuild must drop the edge — no
        // stale reverse entry survives (the P0-1 lesson, by construction).
        let g2 = build_with(
            &[file("src/a.x", "stub", &[]), file("src/b.x", "stub", &[])],
            stub,
        );
        assert!(
            g2.importers(1).is_empty(),
            "stale reverse edge survived rebuild"
        );
    }

    #[test]
    fn cycles_reports_sccs_and_self_loops_only() {
        // a -> b -> c -> a  (3-cycle);  d -> e (no cycle);  f -> f (self loop).
        let files = vec![
            file("a.x", "stub", &["./b"]),
            file("b.x", "stub", &["./c"]),
            file("c.x", "stub", &["./a"]),
            file("d.x", "stub", &["./e"]),
            file("e.x", "stub", &[]),
            file("f.x", "stub", &["./f"]),
        ];
        let g = build_with(&files, stub);
        let cycles = g.cycles();
        // The 3-cycle {0,1,2} and the self-loop {5}; d/e are acyclic and absent.
        assert_eq!(cycles.len(), 2);
        assert_eq!(cycles[0], vec![0, 1, 2]);
        assert_eq!(cycles[1], vec![5]);
    }

    #[test]
    fn transitive_importers_is_cycle_safe() {
        // a -> b -> c, plus a cycle c -> a, to prove termination.
        let files = vec![
            file("a.x", "stub", &["./b"]),
            file("b.x", "stub", &["./c"]),
            file("c.x", "stub", &["./a"]),
        ];
        let g = build_with(&files, stub);
        // a (via b) and b (directly) import c. The cycle walks back to c itself,
        // which must NOT be reported as its own importer.
        let mut ti = g.transitive_importers(2);
        ti.sort_unstable();
        assert_eq!(ti, vec![0, 1]);
    }

    #[test]
    fn rust_brace_group_under_the_root_yields_one_edge_per_module() {
        // `use crate::{config, storage};` names two modules — both edges must land.
        let files = vec![
            file("/repo/src/main.rs", "rust", &["crate::{config, storage}"]),
            file("/repo/src/config.rs", "rust", &[]),
            file("/repo/src/storage.rs", "rust", &[]),
        ];
        let g = build_module_graph(&files);
        assert_eq!(g.imports(0).len(), 2);
        assert_eq!(g.importers(1), &[0]);
        assert_eq!(g.importers(2), &[0]);
        // The raw specifier is preserved on every expansion.
        assert!(
            g.imports(0)
                .iter()
                .all(|i| i.module_path == "crate::{config, storage}")
        );
    }

    #[test]
    fn rust_item_group_stays_a_single_edge() {
        // `use crate::a::{X, Y};` names ITEMS of one module — the expansions
        // resolve to the same file and collapse back to one entry.
        let files = vec![
            file("/repo/src/main.rs", "rust", &["crate::a::{X, Y}"]),
            file("/repo/src/a.rs", "rust", &[]),
        ];
        let g = build_module_graph(&files);
        assert_eq!(g.imports(0).len(), 1);
        assert_eq!(g.imports(0)[0].target, ImportTarget::File(1));
    }

    #[test]
    fn rust_group_with_self_member_resolves_the_base_module() {
        let files = vec![
            file("/repo/src/main.rs", "rust", &["crate::a::{self, X}"]),
            file("/repo/src/a.rs", "rust", &[]),
        ];
        let g = build_module_graph(&files);
        assert_eq!(g.imports(0)[0].target, ImportTarget::File(1));
    }

    #[test]
    fn unknown_language_yields_all_unresolved() {
        let files = vec![file("f.weird", "cobol", &["SOMELIB"])];
        let g = build_module_graph(&files);
        assert_eq!(
            g.imports(0)[0].target,
            ImportTarget::Unresolved("SOMELIB".to_string())
        );
    }

    #[test]
    fn normalize_and_join_collapse_segments() {
        assert_eq!(normalize_path("./src/./a.rs"), "src/a.rs");
        assert_eq!(normalize_path("src/x/../a.rs"), "src/a.rs");
        assert_eq!(join_normalized("src/pkg", "../a"), "src/a");
        assert_eq!(join_normalized("src", "./b"), "src/b");
        assert_eq!(parent_dir("src/a/b.rs"), "src/a");
        assert_eq!(parent_dir("top.rs"), "");
    }
}
