// Placeholder RepoMap - will be enhanced in Phase 2: Task 2.1
use crate::storage::graph::{ModuleGraph, build_module_graph};
use crate::types::{
    AnalysisError, ExportStatement, FunctionSignature, ImportStatement, StructSignature, TreeNode,
    TypeKind,
};
use anyhow::Context;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::SystemTime;

// Create our own Result type alias for this module
type Result<T> = std::result::Result<T, AnalysisError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallSite {
    pub file_path: String,
    pub line_number: u32,
    pub column: u32,
    pub function_name: String,
    pub caller_function: Option<String>,
}

/// A function that transitively calls a target function, discovered by walking
/// UP the call graph. `depth` is the number of hops from the target (1 = direct
/// caller). `ambiguous` is true when this caller was reached by expanding a name
/// that has more than one definition in the repo — the call graph is name-keyed,
/// so which of those definitions the caller actually invokes cannot be
/// determined here; the caller is a *candidate*, not a confirmed link.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitiveCaller {
    pub function_name: String,
    pub file_path: String,
    pub depth: usize,
    #[serde(default)]
    pub ambiguous: bool,
}

#[derive(Debug, Clone)]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub total_matches: usize,
    pub query_duration_ms: u64,
}

impl<T> QueryResult<T> {
    pub fn new(items: Vec<T>, total_matches: usize, query_duration_ms: u64) -> Self {
        Self {
            items,
            total_matches,
            query_duration_ms,
        }
    }
}

/// Compact representation of a file's key elements for repository overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSkeleton {
    pub path: String,
    pub language: String,
    pub size_bytes: u64,
    pub line_count: u32,
    pub functions: Vec<FunctionSummary>,
    pub structs: Vec<StructSummary>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub is_public: bool,
    pub is_test: bool,
    pub last_modified: Option<SystemTime>,
}

/// Compact function signature for repository overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSummary {
    pub name: String,
    pub is_public: bool,
    pub is_async: bool,
    pub parameter_count: usize,
    pub return_type: Option<String>,
    pub line_number: u32,
}

/// Compact struct definition for repository overview  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructSummary {
    pub name: String,
    pub is_public: bool,
    pub field_count: usize,
    pub is_enum: bool,
    pub line_number: u32,
}

/// Directory node in the repository tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryNode {
    pub name: String,
    pub path: String,
    pub children: Vec<RepositoryTreeNode>,
    pub file_count: usize,
    pub total_lines: u32,
    pub languages: HashSet<String>,
}

/// File node in the repository tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub skeleton: FileSkeleton,
}

/// Repository tree node (either directory or file)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RepositoryTreeNode {
    Directory(DirectoryNode),
    File(FileNode),
}

/// Complete repository structure and overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryTree {
    pub root: DirectoryNode,
    pub summary: RepositorySummary,
    pub generated_at: SystemTime,
}

/// High-level repository statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySummary {
    pub total_files: usize,
    pub total_directories: usize,
    pub total_lines: u32,
    pub total_functions: usize,
    pub total_structs: usize,
    pub languages: HashMap<String, usize>, // language -> file count
    pub largest_files: Vec<(String, u64)>, // (path, size_bytes)
    pub function_distribution: HashMap<String, usize>, // file -> function count
    pub dependency_graph: HashMap<String, Vec<String>>, // file -> imported files
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMapMetadata {
    pub total_files: usize,
    pub total_functions: usize,
    pub total_structs: usize,
    pub total_imports: usize,
    pub total_exports: usize,
    pub languages: HashSet<String>,
    pub last_updated: SystemTime,
    pub memory_usage_bytes: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl Default for RepoMapMetadata {
    fn default() -> Self {
        Self {
            total_files: 0,
            total_functions: 0,
            total_structs: 0,
            total_imports: 0,
            total_exports: 0,
            languages: HashSet::new(),
            last_updated: SystemTime::now(),
            memory_usage_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
        }
    }
}

/// Enhanced RepoMap with fast lookups and comprehensive indexing
#[derive(Debug)]
pub struct RepoMap {
    // Core data
    files: Vec<TreeNode>,

    // Repository tree structure (uses RwLock for interior mutability)
    repository_tree: RwLock<Option<RepositoryTree>>,

    // Derived module graph (resolved cross-file imports). Rebuilt lazily from the
    // files after any change — never patched per file (see storage::graph).
    module_graph: RwLock<Option<ModuleGraph>>,

    // Fast indexes
    file_index: HashMap<String, usize>, // file_path -> index
    function_index: HashMap<String, Vec<usize>>, // function_name -> file indices
    struct_index: HashMap<String, Vec<usize>>, // struct_name -> file indices
    import_index: HashMap<String, Vec<usize>>, // import_path -> file indices
    export_index: HashMap<String, Vec<usize>>, // export_name -> file indices
    language_index: HashMap<String, Vec<usize>>, // language -> file indices

    // Call graph
    call_graph: HashMap<String, Vec<CallSite>>, // function_name -> call sites

    // Metadata
    metadata: RepoMapMetadata,

    // Memory management
    max_files: Option<usize>,

    // Query caching
    query_cache: HashMap<String, (Vec<usize>, SystemTime)>, // query -> (results, timestamp)
    cache_ttl_seconds: u64,
}

impl Clone for RepoMap {
    fn clone(&self) -> Self {
        // Clone the repository tree by reading it
        let repository_tree_clone = self.repository_tree.read().unwrap().clone();

        Self {
            files: self.files.clone(),
            repository_tree: RwLock::new(repository_tree_clone),
            // Derived; let the clone rebuild it on demand rather than deep-copying.
            module_graph: RwLock::new(None),
            file_index: self.file_index.clone(),
            function_index: self.function_index.clone(),
            struct_index: self.struct_index.clone(),
            import_index: self.import_index.clone(),
            export_index: self.export_index.clone(),
            language_index: self.language_index.clone(),
            call_graph: self.call_graph.clone(),
            metadata: self.metadata.clone(),
            max_files: self.max_files,
            query_cache: self.query_cache.clone(),
            cache_ttl_seconds: self.cache_ttl_seconds,
        }
    }
}

impl Default for RepoMap {
    fn default() -> Self {
        Self::new()
    }
}

impl RepoMap {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            repository_tree: RwLock::new(None),
            module_graph: RwLock::new(None),
            file_index: HashMap::new(),
            function_index: HashMap::new(),
            struct_index: HashMap::new(),
            import_index: HashMap::new(),
            export_index: HashMap::new(),
            language_index: HashMap::new(),
            call_graph: HashMap::new(),
            metadata: RepoMapMetadata::default(),
            max_files: None,
            query_cache: HashMap::new(),
            cache_ttl_seconds: 300, // 5 minutes
        }
    }

    pub fn with_max_files(mut self, max_files: usize) -> Self {
        self.max_files = Some(max_files);
        self
    }

    pub fn with_cache_ttl(mut self, ttl_seconds: u64) -> Self {
        self.cache_ttl_seconds = ttl_seconds;
        self
    }

    /// Add or update a file in the repository map
    pub fn add_file(&mut self, tree_node: TreeNode) -> Result<()> {
        // Check memory limits
        if let Some(max) = self.max_files {
            if self.files.len() >= max && !self.file_index.contains_key(&tree_node.file_path) {
                return Err(AnalysisError::Other(format!(
                    "Maximum file limit ({}) reached",
                    max
                )));
            }
        }

        let file_path = tree_node.file_path.clone();

        // Remove existing file if present
        if let Some(&existing_index) = self.file_index.get(&file_path) {
            self.remove_file_by_index(existing_index);
        }

        // Add new file
        let new_index = self.files.len();
        self.files.push(tree_node.clone());

        // Update indexes
        self.update_indexes_for_file(new_index, &tree_node)?;

        // Update metadata
        self.update_metadata();

        // Clear cache as data has changed
        self.query_cache.clear();

        // Invalidate repository tree - will be rebuilt when next accessed
        self.repository_tree.write().unwrap().take();
        // The module graph is derived from the files too; invalidate it in lockstep.
        self.module_graph.write().unwrap().take();

        Ok(())
    }

    /// Remove a file from the repository map
    pub fn remove_file(&mut self, file_path: &str) -> Result<bool> {
        if let Some(&index) = self.file_index.get(file_path) {
            self.remove_file_by_index(index);
            self.update_metadata();
            self.query_cache.clear();

            // Invalidate repository tree - will be rebuilt when next accessed
            self.repository_tree.write().unwrap().take();
            // The module graph is derived from the files too; invalidate it in lockstep.
            self.module_graph.write().unwrap().take();

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get a file by path
    pub fn get_file(&self, file_path: &str) -> Option<&TreeNode> {
        self.file_index
            .get(file_path)
            .and_then(|&index| self.files.get(index))
    }

    /// Get all files
    pub fn get_all_files(&self) -> &[TreeNode] {
        &self.files
    }

    /// Ensure the derived module graph is built (rebuild-all from the current
    /// files). Cheap no-op when already fresh; invalidated wholesale on any file
    /// change, so this is the single rebuild point — no per-file patching.
    pub fn build_module_graph_if_needed(&self) {
        if self.module_graph.read().unwrap().is_some() {
            return;
        }
        let graph = build_module_graph(&self.files);
        *self.module_graph.write().unwrap() = Some(graph);
    }

    /// A clone of the current module graph, building it first if stale. Cloned
    /// (not borrowed) because it lives behind interior-mutability; callers that
    /// need many queries should clone once and reuse.
    pub fn module_graph(&self) -> ModuleGraph {
        self.build_module_graph_if_needed();
        self.module_graph
            .read()
            .unwrap()
            .clone()
            .unwrap_or_default()
    }

    /// Map a file path to its index in the current file set (module-graph indices
    /// are positions in `get_all_files`).
    pub fn file_index_of(&self, file_path: &str) -> Option<usize> {
        self.file_index.get(file_path).copied()
    }

    /// Get files by language
    pub fn get_files_by_language(&self, language: &str) -> Vec<&TreeNode> {
        self.language_index
            .get(language)
            .map(|indices| indices.iter().filter_map(|&i| self.files.get(i)).collect())
            .unwrap_or_default()
    }

    /// Find functions by pattern (supports regex and fuzzy matching) - Original method
    pub fn find_functions(&self, pattern: &str) -> QueryResult<&FunctionSignature> {
        let start_time = std::time::Instant::now();

        // Check cache first
        let cache_key = format!("func:{}", pattern);
        if let Some((cached_indices, timestamp)) = self.query_cache.get(&cache_key) {
            if timestamp.elapsed().unwrap_or_default().as_secs() < self.cache_ttl_seconds {
                let functions: Vec<&FunctionSignature> = cached_indices
                    .iter()
                    .filter_map(|&file_idx| self.files.get(file_idx))
                    .flat_map(|file| &file.functions)
                    .filter(|func| self.matches_pattern(&func.name, pattern))
                    .collect();

                let len = functions.len();
                return QueryResult::new(functions, len, start_time.elapsed().as_millis() as u64);
            }
        }

        let mut results = Vec::new();

        // Try exact match first. function_index[pattern] lists a file index once
        // per definition of that name, so a file defining the name more than once
        // (e.g. a trait method signature plus its impl) appears multiple times.
        // Visit each file once — the inner loop already collects every matching
        // function in it — otherwise results are duplicated.
        if let Some(file_indices) = self.function_index.get(pattern) {
            let mut seen_files = HashSet::new();
            for &file_idx in file_indices {
                if !seen_files.insert(file_idx) {
                    continue;
                }
                if let Some(file) = self.files.get(file_idx) {
                    for func in &file.functions {
                        if func.name == pattern {
                            results.push(func);
                        }
                    }
                }
            }
        }

        // If no exact matches, try pattern matching
        if results.is_empty() {
            for file in &self.files {
                for func in &file.functions {
                    if self.matches_pattern(&func.name, pattern) {
                        results.push(func);
                    }
                }
            }
        }

        let duration = start_time.elapsed().as_millis() as u64;
        let len = results.len();
        QueryResult::new(results, len, duration)
    }

    /// Find functions with limit and fuzzy matching support - CLI-compatible method
    pub fn find_functions_with_options(
        &self,
        pattern: &str,
        limit: usize,
        fuzzy: bool,
    ) -> Vec<&FunctionSignature> {
        if fuzzy {
            let fuzzy_results = self.fuzzy_search(pattern, Some(limit));
            let mut function_results = Vec::new();

            for file in &self.files {
                for func in &file.functions {
                    for (fuzzy_match, _score) in &fuzzy_results {
                        if fuzzy_match.contains(&func.name) {
                            function_results.push(func);
                            if function_results.len() >= limit {
                                return function_results;
                            }
                        }
                    }
                }
            }

            function_results
        } else {
            let query_result = self.find_functions(pattern);
            query_result.items.into_iter().take(limit).collect()
        }
    }

    /// Find structs by pattern
    pub fn find_structs(&self, pattern: &str) -> QueryResult<&StructSignature> {
        let start_time = std::time::Instant::now();
        let mut results = Vec::new();

        // Try exact match first. Dedup file indices for the same reason as
        // find_functions: struct_index lists a file once per definition of the
        // name, so visiting each file once avoids duplicated results.
        if let Some(file_indices) = self.struct_index.get(pattern) {
            let mut seen_files = HashSet::new();
            for &file_idx in file_indices {
                if !seen_files.insert(file_idx) {
                    continue;
                }
                if let Some(file) = self.files.get(file_idx) {
                    for struct_def in &file.structs {
                        if struct_def.name == pattern {
                            results.push(struct_def);
                        }
                    }
                }
            }
        }

        // If no exact matches, try pattern matching
        if results.is_empty() {
            for file in &self.files {
                for struct_def in &file.structs {
                    if self.matches_pattern(&struct_def.name, pattern) {
                        results.push(struct_def);
                    }
                }
            }
        }

        let duration = start_time.elapsed().as_millis() as u64;
        let len = results.len();
        //println!("find_structs: {:?}", results);
        QueryResult::new(results, len, duration)
    }

    /// Find structs with limit and fuzzy matching support - CLI-compatible method
    pub fn find_structs_with_options(
        &self,
        pattern: &str,
        limit: usize,
        fuzzy: bool,
    ) -> Vec<&StructSignature> {
        if fuzzy {
            let fuzzy_results = self.fuzzy_search(pattern, Some(limit));
            let mut struct_results = Vec::new();

            for file in &self.files {
                for struct_def in &file.structs {
                    for (fuzzy_match, _score) in &fuzzy_results {
                        if fuzzy_match.contains(&struct_def.name) {
                            struct_results.push(struct_def);
                            if struct_results.len() >= limit {
                                return struct_results;
                            }
                        }
                    }
                }
            }

            struct_results
        } else {
            let query_result = self.find_structs(pattern);
            query_result.items.into_iter().take(limit).collect()
        }
    }

    /// Get file dependencies based on imports
    pub fn get_file_dependencies(&self, file_path: &str) -> Vec<String> {
        if let Some(file) = self.get_file(file_path) {
            file.imports
                .iter()
                .map(|import| import.module_path.clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Find all callers of a specific function
    pub fn find_function_callers(&self, function_name: &str) -> Vec<CallSite> {
        self.call_graph
            .get(function_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Return the distinct files that define `function_name`. More than one entry
    /// means the name is defined in more than one file — a cross-file collision:
    /// callers attributed to it through the name-keyed call graph are candidates,
    /// not a confirmed single target. Deduplicated by file, so a file that defines
    /// the name twice (e.g. a trait method signature plus its impl) counts once
    /// and is not mistaken for a collision. Used to explain ambiguity in output.
    pub fn function_definition_files(&self, function_name: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        self.function_index
            .get(function_name)
            .into_iter()
            .flatten()
            .filter(|&&idx| seen.insert(idx))
            .filter_map(|&idx| self.files.get(idx).map(|f| f.file_path.clone()))
            .collect()
    }

    /// Number of distinct files defining `function_name`. `> 1` is the name-collision
    /// signal for ambiguity flagging: it counts each file once, so a trait signature
    /// plus its impl in the same file is not a false collision (the flaw the bare
    /// `function_index[name].len()` had).
    pub fn name_definition_file_count(&self, function_name: &str) -> usize {
        let mut seen = HashSet::new();
        self.function_index
            .get(function_name)
            .into_iter()
            .flatten()
            .filter(|&&idx| seen.insert(idx))
            .count()
    }

    /// The owning type of the function named `name` defined in `file_path`, if any
    /// (Rust `impl` type, Python/TS class). Used to render qualified caller names
    /// like `Loader::load`. If a file has more than one function of that name with
    /// *different* owners (e.g. `Foo::build` and `Bar::build`), the name is
    /// ambiguous within the file and we return `None` rather than guessing the
    /// wrong owner — an honest bare name beats a confidently wrong `Foo::build`.
    pub fn function_owner(&self, file_path: &str, name: &str) -> Option<String> {
        let file = self.get_file(file_path)?;
        let mut matches = file.functions.iter().filter(|f| f.name == name);
        let first = matches.next()?;
        if matches.any(|f| f.owner != first.owner) {
            return None;
        }
        first.owner.clone()
    }

    /// Render a function's display name as `Owner::name` when it has an owning
    /// type, else the bare name.
    pub fn qualified_function_name(&self, file_path: &str, name: &str) -> String {
        match self.function_owner(file_path, name) {
            Some(owner) => format!("{owner}::{name}"),
            None => name.to_string(),
        }
    }

    /// Walk UP the call graph to find every function that TRANSITIVELY calls
    /// `function_name`.
    ///
    /// BFS starting from the target: level-1 callers are the distinct enclosing
    /// functions (`caller_function`) taken from `find_function_callers`; for each
    /// of those we recurse to find ITS callers, and so on. `depth` records the
    /// level at which each caller was first reached (1 = direct caller). Visited
    /// functions are tracked so cycles terminate. `max_depth == 0` means
    /// unlimited depth.
    ///
    /// Caller identity is `(file_path, function_name)`: two functions that share a
    /// name in different files are distinct BFS nodes and never merge. The call
    /// graph itself is keyed by callee *name*, so when a callee name is defined in
    /// more than one file (`name_definition_file_count > 1`) the callers reached
    /// through it cannot be attributed to a single definition — they are flagged
    /// `ambiguous` (candidates), not dropped and not silently merged. Actually
    /// resolving which definition each call targets is deferred to the resolved
    /// call graph (Phase 3); this keeps the name-keyed view honest.
    ///
    /// KNOWN LIMITATION (name-keyed): every definition of the target name is seeded
    /// as visited so the target is not reported as its own caller (and cycles
    /// terminate). When the target name is itself multiply-defined, a *collision
    /// sibling* — a same-named function in another file that genuinely calls the
    /// target — is therefore not re-reported as a caller. Distinguishing that from
    /// self-recursion is impossible without resolved edges; Phase 3 fixes it. The
    /// effect is a rare under-report, never a wrong-direction edge.
    pub fn transitive_callers(
        &self,
        function_name: &str,
        max_depth: usize,
    ) -> Vec<TransitiveCaller> {
        let mut results: Vec<TransitiveCaller> = Vec::new();
        // Visited nodes keyed by (file_path, function_name).
        let mut visited: HashSet<(String, String)> = HashSet::new();
        // Every definition of the target is "visited" so a self-recursive call
        // does not re-enqueue the target as its own caller.
        if let Some(indices) = self.function_index.get(function_name) {
            for &idx in indices {
                visited.insert((self.files[idx].file_path.clone(), function_name.to_string()));
            }
        }

        // BFS frontier of caller names whose callers we still expand. Expansion is
        // name-keyed (the call graph only knows callee names); the file half of
        // each node exists for visited-dedup. The target is seeded by name with an
        // empty file placeholder, which never collides with a real caller node.
        let mut frontier: Vec<(String, String)> = vec![(String::new(), function_name.to_string())];
        let mut depth: usize = 1;

        while !frontier.is_empty() {
            if max_depth != 0 && depth > max_depth {
                break;
            }

            let mut next_frontier: Vec<(String, String)> = Vec::new();

            for (_from_file, callee) in &frontier {
                // A callee name defined in more than one file cannot be attributed
                // to one function; callers reached through it are candidates. Uses
                // distinct-file count so a trait signature + its impl in one file
                // is not a false collision.
                let ambiguous = self.name_definition_file_count(callee) > 1;

                for call_site in self.find_function_callers(callee) {
                    // Skip top-level/module-level calls with no enclosing function.
                    let caller = match &call_site.caller_function {
                        Some(name) => name.clone(),
                        None => continue,
                    };

                    let key = (call_site.file_path.clone(), caller.clone());
                    if visited.contains(&key) {
                        continue;
                    }
                    visited.insert(key);

                    results.push(TransitiveCaller {
                        function_name: caller.clone(),
                        file_path: call_site.file_path.clone(),
                        depth,
                        ambiguous,
                    });
                    next_frontier.push((call_site.file_path.clone(), caller));
                }
            }

            frontier = next_frontier;
            depth += 1;
        }

        results.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then_with(|| a.function_name.cmp(&b.function_name))
                .then_with(|| a.file_path.cmp(&b.file_path))
        });
        results
    }

    /// Get repository metadata
    pub fn get_metadata(&self) -> &RepoMapMetadata {
        &self.metadata
    }

    /// Get changed files since a specific time
    pub fn get_changed_files(&self, since: SystemTime) -> Vec<&TreeNode> {
        self.files
            .iter()
            .filter(|file| file.last_modified > since)
            .collect()
    }

    /// Search across all content using fuzzy matching
    pub fn fuzzy_search(&self, query: &str, limit: Option<usize>) -> Vec<(String, f64)> {
        let matcher = SkimMatcherV2::default();
        let mut results = Vec::new();

        // Search function names
        for file in &self.files {
            for func in &file.functions {
                if let Some(score) = matcher.fuzzy_match(&func.name, query) {
                    results.push((format!("fn {}", func.name), score as f64));
                }
            }

            // Search struct names
            for struct_def in &file.structs {
                if let Some(score) = matcher.fuzzy_match(&struct_def.name, query) {
                    results.push((format!("struct {}", struct_def.name), score as f64));
                }
            }
        }

        // Sort by score (higher is better)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(limit) = limit {
            results.truncate(limit);
        }

        results
    }

    /// Get memory usage statistics
    pub fn get_memory_usage(&self) -> usize {
        // Rough estimation of memory usage
        let base_size = std::mem::size_of::<Self>();
        let files_size = self.files.len() * std::mem::size_of::<TreeNode>();
        let indexes_size = self.file_index.len() * 64 // Rough estimate for HashMap entries
            + self.function_index.len() * 64
            + self.struct_index.len() * 64
            + self.import_index.len() * 64
            + self.export_index.len() * 64
            + self.language_index.len() * 64;

        base_size + files_size + indexes_size
    }

    /// Clear query cache
    pub fn clear_cache(&mut self) {
        self.query_cache.clear();
    }

    /// Find imports by pattern
    pub fn find_imports(&self, pattern: &str, limit: usize) -> Vec<&ImportStatement> {
        let mut results = Vec::new();

        for file in &self.files {
            for import in &file.imports {
                if self.matches_pattern(&import.module_path, pattern) {
                    results.push(import);
                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Find exports by pattern
    pub fn find_exports(&self, pattern: &str, limit: usize) -> Vec<&ExportStatement> {
        let mut results = Vec::new();

        for file in &self.files {
            for export in &file.exports {
                if self.matches_pattern(&export.exported_item, pattern) {
                    results.push(export);
                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Get the number of files in the repository map
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Check if the repository map is empty
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.get_memory_usage()
    }

    /// Get the repository tree (building it if necessary)
    pub fn get_repository_tree(&self) -> Option<RepositoryTree> {
        // First try to read existing tree
        if let Ok(tree_guard) = self.repository_tree.read() {
            if tree_guard.is_some() {
                return tree_guard.clone();
            }
        }

        // If tree doesn't exist, we need to build it
        // Since we can't mutate self in this immutable method, return None
        // The caller should use build_repository_tree_if_needed instead
        None
    }

    /// Build repository tree if it doesn't exist (for mutable access)
    pub fn build_repository_tree_if_needed(&mut self) -> Result<()> {
        if self.repository_tree.read().unwrap().is_none() {
            self.build_repository_tree()?;
        }
        Ok(())
    }

    /// Build the complete repository tree structure from current files
    pub fn build_repository_tree(&mut self) -> Result<()> {
        let mut directory_map: HashMap<String, DirectoryNode> = HashMap::new();
        let mut file_nodes: Vec<FileNode> = Vec::new();

        // Generate file skeletons and organize by directory
        for tree_node in &self.files {
            let file_skeleton = self.generate_file_skeleton(tree_node)?;
            let file_node = FileNode {
                name: std::path::Path::new(&tree_node.file_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path: tree_node.file_path.clone(),
                skeleton: file_skeleton,
            };

            // Get directory path
            let dir_path = std::path::Path::new(&tree_node.file_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("/"))
                .to_string_lossy()
                .to_string();

            // Create directory node if it doesn't exist
            let dir_name = std::path::Path::new(&dir_path)
                .file_name()
                .unwrap_or_else(|| std::path::Path::new(&dir_path).as_os_str())
                .to_string_lossy()
                .to_string();

            // Seed a directory node for this file's parent directory, but do NOT
            // accumulate file_count / total_lines here. The authoritative roll-up
            // (both files-in-directory and nested-directory totals) is computed
            // once in `build_directory_hierarchy`. Counting here as well would
            // double-count every file. Only the language set is pre-populated,
            // which is idempotent (a HashSet) and harmless to merge again later.
            if let Some(dir_node) = directory_map.get_mut(&dir_path) {
                dir_node.languages.insert(tree_node.language.clone());
            } else {
                let mut languages = HashSet::new();
                languages.insert(tree_node.language.clone());

                directory_map.insert(
                    dir_path.clone(),
                    DirectoryNode {
                        name: dir_name,
                        path: dir_path,
                        children: Vec::new(),
                        file_count: 0,
                        total_lines: 0,
                        languages,
                    },
                );
            }

            file_nodes.push(file_node);
        }

        // Build hierarchical directory structure
        let root = self.build_directory_hierarchy(directory_map, file_nodes)?;

        // Generate repository summary
        let summary = self.generate_repository_summary()?;

        // Create repository tree
        let repository_tree = RepositoryTree {
            root,
            summary,
            generated_at: SystemTime::now(),
        };

        *self.repository_tree.write().unwrap() = Some(repository_tree);

        Ok(())
    }

    /// Generate a file skeleton from a TreeNode
    fn generate_file_skeleton(&self, tree_node: &TreeNode) -> Result<FileSkeleton> {
        let function_summaries: Vec<FunctionSummary> = tree_node
            .functions
            .iter()
            .map(|func| FunctionSummary {
                name: func.name.clone(),
                is_public: func.is_public,
                is_async: func.is_async,
                parameter_count: func.parameters.len(),
                return_type: func.return_type.clone(),
                line_number: func.start_line,
            })
            .collect();

        let struct_summaries: Vec<StructSummary> = tree_node
            .structs
            .iter()
            .map(|struct_def| StructSummary {
                name: struct_def.name.clone(),
                is_public: struct_def.is_public,
                field_count: struct_def.fields.len(),
                is_enum: struct_def.kind == TypeKind::Enum,
                line_number: struct_def.start_line,
            })
            .collect();

        let imports: Vec<String> = tree_node
            .imports
            .iter()
            .map(|import| import.module_path.clone())
            .collect();

        let exports: Vec<String> = tree_node
            .exports
            .iter()
            .map(|export| export.exported_item.clone())
            .collect();

        // Determine if file is public or test based on path and content
        let is_test = tree_node.file_path.contains("test")
            || tree_node.file_path.contains("tests")
            || function_summaries
                .iter()
                .any(|f| f.name.starts_with("test_"));

        let is_public = tree_node.file_path.contains("lib.rs")
            || tree_node.file_path.contains("main.rs")
            || exports.len() > 0;

        // Estimate file size and line count based on content
        let estimated_size =
            (tree_node.functions.len() * 100 + tree_node.structs.len() * 50) as u64;
        let estimated_lines = (tree_node.functions.len() * 10 + tree_node.structs.len() * 5) as u32;

        Ok(FileSkeleton {
            path: tree_node.file_path.clone(),
            language: tree_node.language.clone(),
            size_bytes: estimated_size,
            line_count: estimated_lines,
            functions: function_summaries,
            structs: struct_summaries,
            imports,
            exports,
            is_public,
            is_test,
            last_modified: Some(tree_node.last_modified),
        })
    }

    /// Build hierarchical directory structure from flat directory map
    fn build_directory_hierarchy(
        &self,
        mut directory_map: HashMap<String, DirectoryNode>,
        file_nodes: Vec<FileNode>,
    ) -> Result<DirectoryNode> {
        // Find the common root path for all files
        let root_path = self.find_common_root_path();

        // Build a proper nested directory structure
        let mut path_to_node: HashMap<String, DirectoryNode> = HashMap::new();

        // First, ensure all directory paths exist
        let mut all_dir_paths: HashSet<String> = HashSet::new();

        // Collect all directory paths from files
        for file_node in &file_nodes {
            let mut current_path = std::path::Path::new(&file_node.path);
            while let Some(parent) = current_path.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                if parent_str != root_path {
                    all_dir_paths.insert(parent_str);
                }
                current_path = parent;
            }
        }

        // Create directory nodes for all paths
        for dir_path in &all_dir_paths {
            let dir_name = std::path::Path::new(dir_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Check if we already have this directory from the directory_map
            let dir_node = directory_map
                .remove(dir_path)
                .unwrap_or_else(|| DirectoryNode {
                    name: dir_name,
                    path: dir_path.clone(),
                    children: Vec::new(),
                    file_count: 0,
                    total_lines: 0,
                    languages: HashSet::new(),
                });

            path_to_node.insert(dir_path.clone(), dir_node);
        }

        // Create root directory
        let root_name = std::path::Path::new(&root_path)
            .file_name()
            .unwrap_or_else(|| std::path::Path::new(&root_path).as_os_str())
            .to_string_lossy()
            .to_string();

        let mut root = directory_map
            .remove(&root_path)
            .unwrap_or_else(|| DirectoryNode {
                name: if root_name.is_empty() {
                    "root".to_string()
                } else {
                    root_name
                },
                path: root_path.clone(),
                children: Vec::new(),
                file_count: 0,
                total_lines: 0,
                languages: HashSet::new(),
            });

        // Add files to their respective directories
        for file_node in file_nodes {
            let file_dir_path = std::path::Path::new(&file_node.path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("/"))
                .to_string_lossy()
                .to_string();

            if file_dir_path == root_path {
                // File belongs directly in root - update stats before moving
                root.file_count += 1;
                if let Some(lang) = self.get_file_language(&file_node.skeleton) {
                    root.languages.insert(lang);
                }
                root.total_lines += file_node.skeleton.line_count;
                root.children.push(RepositoryTreeNode::File(file_node));
            } else if let Some(dir_node) = path_to_node.get_mut(&file_dir_path) {
                // File belongs in a subdirectory - update stats before moving
                dir_node.file_count += 1;
                if let Some(lang) = self.get_file_language(&file_node.skeleton) {
                    dir_node.languages.insert(lang);
                }
                dir_node.total_lines += file_node.skeleton.line_count;
                dir_node.children.push(RepositoryTreeNode::File(file_node));
            }
        }

        // Now build the nested structure by organizing directories hierarchically
        // Sort paths by depth (shallowest first) to ensure proper nesting
        let mut sorted_paths: Vec<_> = all_dir_paths.into_iter().collect();
        sorted_paths.sort_by_key(|path| path.matches('/').count());

        // Process directories from deepest to shallowest to build bottom-up
        for dir_path in sorted_paths.iter().rev() {
            if let Some(dir_node) = path_to_node.remove(dir_path) {
                // Find this directory's parent
                let parent_path = std::path::Path::new(dir_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string());

                match parent_path {
                    Some(parent_path) if parent_path == root_path => {
                        // This directory's parent is root - update stats before moving
                        root.file_count += dir_node.file_count;
                        root.total_lines += dir_node.total_lines;
                        for lang in &dir_node.languages {
                            root.languages.insert(lang.clone());
                        }
                        root.children.push(RepositoryTreeNode::Directory(dir_node));
                    }
                    Some(parent_path) => {
                        // This directory has another directory as parent
                        if let Some(parent_node) = path_to_node.get_mut(&parent_path) {
                            // Update parent stats before moving
                            parent_node.file_count += dir_node.file_count;
                            parent_node.total_lines += dir_node.total_lines;
                            for lang in &dir_node.languages {
                                parent_node.languages.insert(lang.clone());
                            }
                            parent_node
                                .children
                                .push(RepositoryTreeNode::Directory(dir_node));
                        }
                    }
                    None => {
                        // This shouldn't happen, but fallback to root
                        root.children.push(RepositoryTreeNode::Directory(dir_node));
                    }
                }
            }
        }

        Ok(root)
    }

    /// Helper to extract language from file skeleton
    fn get_file_language(&self, skeleton: &FileSkeleton) -> Option<String> {
        if skeleton.language.is_empty() {
            None
        } else {
            Some(skeleton.language.clone())
        }
    }

    /// Find the common root path of all files
    fn find_common_root_path(&self) -> String {
        if self.files.is_empty() {
            return "/".to_string();
        }

        if self.files.len() == 1 {
            return std::path::Path::new(&self.files[0].file_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("/"))
                .to_string_lossy()
                .to_string();
        }

        // Find common prefix of all file paths
        let first_path = &self.files[0].file_path;
        let mut common_prefix = first_path.clone();

        for file in &self.files[1..] {
            common_prefix = self.find_common_prefix(&common_prefix, &file.file_path);
        }

        // Ensure we end at a directory boundary and normalize the path
        let common_path = std::path::Path::new(&common_prefix);

        // If the common prefix ends with a file, get its parent directory
        if common_path.is_file() || !common_prefix.ends_with('/') {
            let parent = common_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("/"));
            parent.to_string_lossy().to_string()
        } else {
            // Remove trailing slash for consistency
            common_prefix.trim_end_matches('/').to_string()
        }
    }

    /// Find common prefix of two paths
    fn find_common_prefix(&self, path1: &str, path2: &str) -> String {
        let chars1: Vec<char> = path1.chars().collect();
        let chars2: Vec<char> = path2.chars().collect();

        let mut common_len = 0;
        for (c1, c2) in chars1.iter().zip(chars2.iter()) {
            if c1 == c2 {
                common_len += 1;
            } else {
                break;
            }
        }

        path1.chars().take(common_len).collect()
    }

    /// Generate repository summary statistics
    fn generate_repository_summary(&self) -> Result<RepositorySummary> {
        let mut language_counts: HashMap<String, usize> = HashMap::new();
        let mut largest_files: Vec<(String, u64)> = Vec::new();
        let mut function_distribution: HashMap<String, usize> = HashMap::new();
        let mut dependency_graph: HashMap<String, Vec<String>> = HashMap::new();

        let mut total_lines = 0;
        let mut total_functions = 0;
        let mut total_structs = 0;
        let mut directory_set: HashSet<String> = HashSet::new();

        for file in &self.files {
            // Language distribution
            *language_counts.entry(file.language.clone()).or_insert(0) += 1;

            // File sizes (estimated based on content)
            let estimated_file_size = (file.functions.len() * 100 + file.structs.len() * 50) as u64;
            largest_files.push((file.file_path.clone(), estimated_file_size));

            // Function distribution
            function_distribution.insert(file.file_path.clone(), file.functions.len());

            // Dependencies (imports)
            let dependencies: Vec<String> = file
                .imports
                .iter()
                .map(|imp| imp.module_path.clone())
                .collect();
            dependency_graph.insert(file.file_path.clone(), dependencies);

            // Statistics (estimated lines based on content)
            let estimated_lines = (file.functions.len() * 10 + file.structs.len() * 5) as u32;
            total_lines += estimated_lines;
            total_functions += file.functions.len();
            total_structs += file.structs.len();

            // Directory count
            if let Some(parent) = std::path::Path::new(&file.file_path).parent() {
                directory_set.insert(parent.to_string_lossy().to_string());
            }
        }

        // Sort largest files by size
        largest_files.sort_by(|a, b| b.1.cmp(&a.1));
        largest_files.truncate(10); // Keep top 10

        Ok(RepositorySummary {
            total_files: self.files.len(),
            total_directories: directory_set.len(),
            total_lines,
            total_functions,
            total_structs,
            languages: language_counts,
            largest_files,
            function_distribution,
            dependency_graph,
        })
    }

    /// Force rebuild of repository tree (useful when files are added/removed)
    pub fn rebuild_repository_tree(&mut self) -> Result<()> {
        self.repository_tree.write().unwrap().take();
        // The module graph is derived from the files too; invalidate it in lockstep.
        self.module_graph.write().unwrap().take();
        self.build_repository_tree()
    }

    /// Get repository tree as JSON for AI tools
    pub fn get_repository_tree_json(&self) -> Result<serde_json::Value> {
        if let Some(tree) = self.get_repository_tree() {
            Ok(serde_json::to_value(tree)?)
        } else {
            Err(AnalysisError::Other(
                "Repository tree not available".to_string(),
            ))
        }
    }

    // Private helper methods

    fn remove_file_by_index(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }

        let file = &self.files[index];
        let file_path = file.file_path.clone();

        // Remove from file index
        self.file_index.remove(&file_path);

        // Remove from other indexes
        self.remove_from_function_index(index);
        self.remove_from_struct_index(index);
        self.remove_from_import_index(index);
        self.remove_from_export_index(index);
        self.remove_from_language_index(index);
        // call_graph is keyed by call-site file_path (not numeric file index), so it is
        // pruned by path here and needs no shift in reindex_after_removal.
        self.remove_from_call_graph(&file_path);

        // Remove from files vector and update remaining indexes
        self.files.remove(index);
        self.reindex_after_removal(index);
    }

    /// Drop every call site originating in `file_path` from the call graph, removing any
    /// key whose call-site vector becomes empty. Without this, `add_file` re-adding a path
    /// appends its call sites a second time (duplicates) and `remove_file` leaves stale ones
    /// — poisoning `find_function_callers`, `trace_callers`, and `analyze_impact`.
    fn remove_from_call_graph(&mut self, file_path: &str) {
        self.call_graph.retain(|_name, sites| {
            sites.retain(|site| site.file_path != file_path);
            !sites.is_empty()
        });
    }

    fn update_indexes_for_file(&mut self, index: usize, tree_node: &TreeNode) -> Result<()> {
        let file_path = tree_node.file_path.clone();

        // Update file index
        self.file_index.insert(file_path, index);

        // Update function index
        for func in &tree_node.functions {
            self.function_index
                .entry(func.name.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update struct index
        for struct_def in &tree_node.structs {
            self.struct_index
                .entry(struct_def.name.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update import index
        for import in &tree_node.imports {
            self.import_index
                .entry(import.module_path.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update export index
        for export in &tree_node.exports {
            self.export_index
                .entry(export.exported_item.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update language index
        self.language_index
            .entry(tree_node.language.clone())
            .or_insert_with(Vec::new)
            .push(index);

        // Update call graph
        for call in &tree_node.function_calls {
            // Determine the enclosing function by line-range containment: the
            // function in this same file whose [start_line, end_line] contains the
            // call's line_number. If several functions contain it (nested/impl
            // blocks), pick the INNERMOST one (smallest containing range).
            let caller_function = tree_node
                .functions
                .iter()
                .filter(|f| f.start_line <= call.line_number && call.line_number <= f.end_line)
                .min_by_key(|f| f.end_line.saturating_sub(f.start_line))
                .map(|f| f.name.clone());

            let call_site = CallSite {
                file_path: tree_node.file_path.clone(),
                line_number: call.line_number,
                column: call.column,
                function_name: call.function_name.clone(),
                caller_function,
            };

            self.call_graph
                .entry(call.function_name.clone())
                .or_insert_with(Vec::new)
                .push(call_site);
        }

        Ok(())
    }

    fn remove_from_function_index(&mut self, file_index: usize) {
        let keys_to_update: Vec<String> = self.function_index.keys().cloned().collect();
        for key in keys_to_update {
            if let Some(indices) = self.function_index.get_mut(&key) {
                indices.retain(|&i| i != file_index);
                if indices.is_empty() {
                    self.function_index.remove(&key);
                }
            }
        }
    }

    fn remove_from_struct_index(&mut self, file_index: usize) {
        let keys_to_update: Vec<String> = self.struct_index.keys().cloned().collect();
        for key in keys_to_update {
            if let Some(indices) = self.struct_index.get_mut(&key) {
                indices.retain(|&i| i != file_index);
                if indices.is_empty() {
                    self.struct_index.remove(&key);
                }
            }
        }
    }

    fn remove_from_import_index(&mut self, file_index: usize) {
        let keys_to_update: Vec<String> = self.import_index.keys().cloned().collect();
        for key in keys_to_update {
            if let Some(indices) = self.import_index.get_mut(&key) {
                indices.retain(|&i| i != file_index);
                if indices.is_empty() {
                    self.import_index.remove(&key);
                }
            }
        }
    }

    fn remove_from_export_index(&mut self, file_index: usize) {
        let keys_to_update: Vec<String> = self.export_index.keys().cloned().collect();
        for key in keys_to_update {
            if let Some(indices) = self.export_index.get_mut(&key) {
                indices.retain(|&i| i != file_index);
                if indices.is_empty() {
                    self.export_index.remove(&key);
                }
            }
        }
    }

    fn remove_from_language_index(&mut self, file_index: usize) {
        let keys_to_update: Vec<String> = self.language_index.keys().cloned().collect();
        for key in keys_to_update {
            if let Some(indices) = self.language_index.get_mut(&key) {
                indices.retain(|&i| i != file_index);
                if indices.is_empty() {
                    self.language_index.remove(&key);
                }
            }
        }
    }

    fn reindex_after_removal(&mut self, removed_index: usize) {
        // Update all indexes to account for the removed file
        for indices in self.function_index.values_mut() {
            for index in indices.iter_mut() {
                if *index > removed_index {
                    *index -= 1;
                }
            }
        }

        for indices in self.struct_index.values_mut() {
            for index in indices.iter_mut() {
                if *index > removed_index {
                    *index -= 1;
                }
            }
        }

        for indices in self.import_index.values_mut() {
            for index in indices.iter_mut() {
                if *index > removed_index {
                    *index -= 1;
                }
            }
        }

        for indices in self.export_index.values_mut() {
            for index in indices.iter_mut() {
                if *index > removed_index {
                    *index -= 1;
                }
            }
        }

        for indices in self.language_index.values_mut() {
            for index in indices.iter_mut() {
                if *index > removed_index {
                    *index -= 1;
                }
            }
        }

        // Update file_index
        let files_to_update: Vec<(String, usize)> = self
            .file_index
            .iter()
            .filter_map(|(path, &index)| {
                if index > removed_index {
                    Some((path.clone(), index - 1))
                } else {
                    None
                }
            })
            .collect();

        for (path, new_index) in files_to_update {
            self.file_index.insert(path, new_index);
        }
    }

    fn update_metadata(&mut self) {
        self.metadata.total_files = self.files.len();
        self.metadata.total_functions = self.files.iter().map(|f| f.functions.len()).sum();
        self.metadata.total_structs = self.files.iter().map(|f| f.structs.len()).sum();
        self.metadata.total_imports = self.files.iter().map(|f| f.imports.len()).sum();
        self.metadata.total_exports = self.files.iter().map(|f| f.exports.len()).sum();
        self.metadata.languages = self.files.iter().map(|f| f.language.clone()).collect();
        self.metadata.last_updated = SystemTime::now();
        self.metadata.memory_usage_bytes = self.get_memory_usage();
    }

    fn matches_pattern(&self, text: &str, pattern: &str) -> bool {
        // Try exact match first
        if text == pattern {
            return true;
        }

        // Try case-insensitive match
        if text.to_lowercase() == pattern.to_lowercase() {
            return true;
        }

        // Try regex if pattern looks like regex (contains regex special chars)
        if pattern.contains([
            '*', '^', '$', '[', ']', '(', ')', '{', '}', '|', '+', '?', '\\',
        ]) {
            if let Ok(regex) = Regex::new(pattern) {
                return regex.is_match(text);
            }
        }

        // Try substring match
        text.to_lowercase().contains(&pattern.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ExportStatement, FunctionCall, FunctionSignature, ImportStatement, Parameter,
        StructSignature,
    };
    use std::time::SystemTime;

    fn create_test_tree_node(name: &str, language: &str) -> TreeNode {
        let mut node = TreeNode::new(format!("/test/{}.rs", name), language.to_string());

        // Add some test functions
        node.functions.push(
            FunctionSignature::new(format!("function_{}", name), node.file_path.clone())
                .with_parameters(vec![
                    Parameter::new("param1".to_string(), "i32".to_string()),
                    Parameter::new("param2".to_string(), "String".to_string()),
                ])
                .with_return_type("Result<(), Error>".to_string())
                .with_visibility(true)
                .with_async(true),
        );

        // Add some test structs
        node.structs.push(StructSignature::new(
            format!("Struct{}", name.to_uppercase()),
            node.file_path.clone(),
        ));

        // Add some test imports
        node.imports.push(
            ImportStatement::new(format!("crate::{}", name), node.file_path.clone())
                .with_external(false),
        );

        // Add some test exports
        node.exports.push(ExportStatement::new(
            format!("pub_{}", name),
            node.file_path.clone(),
        ));

        // Add some test function calls
        node.function_calls.push(FunctionCall::new(
            format!("call_{}", name),
            node.file_path.clone(),
            42,
        ));

        node.content_hash = format!("hash_{}", name);
        node
    }

    #[test]
    fn test_repo_map_creation() {
        let repo_map = RepoMap::new();
        assert_eq!(repo_map.get_all_files().len(), 0);
        assert_eq!(repo_map.get_metadata().total_files, 0);
        assert_eq!(repo_map.get_metadata().total_functions, 0);
    }

    #[test]
    fn test_repo_map_with_limits() {
        let repo_map = RepoMap::new().with_max_files(5).with_cache_ttl(60);

        assert_eq!(repo_map.max_files, Some(5));
        assert_eq!(repo_map.cache_ttl_seconds, 60);
    }

    #[test]
    fn test_add_file() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test1", "rust");

        let result = repo_map.add_file(node.clone());
        assert!(result.is_ok());

        assert_eq!(repo_map.get_all_files().len(), 1);
        assert_eq!(repo_map.get_metadata().total_files, 1);
        assert_eq!(repo_map.get_metadata().total_functions, 1);
        assert_eq!(repo_map.get_metadata().total_structs, 1);

        // Verify file can be retrieved
        let retrieved = repo_map.get_file(&node.file_path);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().file_path, node.file_path);
    }

    #[test]
    fn test_add_multiple_files() {
        let mut repo_map = RepoMap::new();

        for i in 0..5 {
            let node = create_test_tree_node(&format!("test{}", i), "rust");
            let result = repo_map.add_file(node);
            assert!(result.is_ok());
        }

        assert_eq!(repo_map.get_all_files().len(), 5);
        assert_eq!(repo_map.get_metadata().total_files, 5);
        assert_eq!(repo_map.get_metadata().total_functions, 5);
        assert_eq!(repo_map.get_metadata().total_structs, 5);
    }

    #[test]
    fn test_update_existing_file() {
        let mut repo_map = RepoMap::new();
        let mut node = create_test_tree_node("test", "rust");

        // Add initial file
        repo_map.add_file(node.clone()).unwrap();
        assert_eq!(repo_map.get_all_files().len(), 1);

        // Update the same file
        node.content_hash = "updated_hash".to_string();
        node.functions.push(FunctionSignature::new(
            "new_function".to_string(),
            node.file_path.clone(),
        ));

        repo_map.add_file(node.clone()).unwrap();

        // Should still have only one file but with updated content
        assert_eq!(repo_map.get_all_files().len(), 1);
        assert_eq!(repo_map.get_metadata().total_functions, 2); // Now has 2 functions

        let retrieved = repo_map.get_file(&node.file_path).unwrap();
        assert_eq!(retrieved.content_hash, "updated_hash");
        assert_eq!(retrieved.functions.len(), 2);
    }

    #[test]
    fn test_remove_file() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");
        let file_path = node.file_path.clone();

        // Add file
        repo_map.add_file(node).unwrap();
        assert_eq!(repo_map.get_all_files().len(), 1);

        // Remove file
        let result = repo_map.remove_file(&file_path);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Should return true indicating file was removed

        assert_eq!(repo_map.get_all_files().len(), 0);
        assert_eq!(repo_map.get_metadata().total_files, 0);
        assert!(repo_map.get_file(&file_path).is_none());

        // Try to remove non-existent file
        let result = repo_map.remove_file("non_existent.rs");
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false
    }

    #[test]
    fn test_max_files_limit() {
        let mut repo_map = RepoMap::new().with_max_files(2);

        // Add files up to limit
        for i in 0..2 {
            let node = create_test_tree_node(&format!("test{}", i), "rust");
            let result = repo_map.add_file(node);
            assert!(result.is_ok());
        }

        // Try to add one more file - should fail
        let node = create_test_tree_node("overflow", "rust");
        let result = repo_map.add_file(node);
        assert!(result.is_err());
        assert_eq!(repo_map.get_all_files().len(), 2);
    }

    #[test]
    fn test_find_functions_exact_match() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");
        repo_map.add_file(node).unwrap();

        let result = repo_map.find_functions("function_test");
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "function_test");
        assert!(result.query_duration_ms < 100); // Should be fast
    }

    #[test]
    fn test_find_functions_pattern_match() {
        let mut repo_map = RepoMap::new();

        // Add multiple files with functions
        for i in 0..3 {
            let node = create_test_tree_node(&format!("test{}", i), "rust");
            repo_map.add_file(node).unwrap();
        }

        // Search for pattern that matches all functions
        let result = repo_map.find_functions("function_");
        assert_eq!(result.items.len(), 3);

        // Search for specific pattern
        let result = repo_map.find_functions("function_test1");
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "function_test1");
    }

    #[test]
    fn test_find_structs() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("example", "rust");
        repo_map.add_file(node).unwrap();

        let result = repo_map.find_structs("StructEXAMPLE");
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "StructEXAMPLE");
    }

    #[test]
    fn test_get_files_by_language() {
        let mut repo_map = RepoMap::new();

        // Add Rust files
        for i in 0..2 {
            let node = create_test_tree_node(&format!("rust{}", i), "rust");
            repo_map.add_file(node).unwrap();
        }

        // Add Python files
        for i in 0..3 {
            let mut node = create_test_tree_node(&format!("python{}", i), "python");
            node.file_path = format!("/test/python{}.py", i);
            repo_map.add_file(node).unwrap();
        }

        let rust_files = repo_map.get_files_by_language("rust");
        assert_eq!(rust_files.len(), 2);

        let python_files = repo_map.get_files_by_language("python");
        assert_eq!(python_files.len(), 3);

        let js_files = repo_map.get_files_by_language("javascript");
        assert_eq!(js_files.len(), 0);
    }

    #[test]
    fn test_get_file_dependencies() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");
        let file_path = node.file_path.clone();
        repo_map.add_file(node).unwrap();

        let dependencies = repo_map.get_file_dependencies(&file_path);
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0], "crate::test");
    }

    // P2-1: the derived module graph is wired into RepoMap — built lazily from the
    // files and rebuilt wholesale (not patched) when files change.
    #[test]
    fn test_module_graph_builds_and_rebuilds_on_change() {
        let mut repo_map = RepoMap::new();
        repo_map
            .add_file(create_test_tree_node("a", "rust"))
            .unwrap();
        repo_map
            .add_file(create_test_tree_node("b", "rust"))
            .unwrap();

        let g = repo_map.module_graph();
        assert_eq!(g.forward.len(), 2, "one forward-edge list per file");
        // Each test node carries one import; it is retained (resolved later by P2-4).
        assert_eq!(g.imports(0).len(), 1);

        // Adding a file invalidates and rebuilds the graph to the new file set.
        repo_map
            .add_file(create_test_tree_node("c", "rust"))
            .unwrap();
        assert_eq!(repo_map.module_graph().forward.len(), 3);

        // Removing a file rebuilds again — no stale entries for the gone file.
        repo_map.remove_file("/test/c.rs").unwrap();
        assert_eq!(repo_map.module_graph().forward.len(), 2);
    }

    #[test]
    fn test_find_function_callers() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");
        repo_map.add_file(node).unwrap();

        let callers = repo_map.find_function_callers("call_test");
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].function_name, "call_test");
        assert_eq!(callers[0].line_number, 42);
    }

    // A file defining the same name twice (e.g. a trait method signature + its
    // impl) lists that file twice in function_index; find_functions must not
    // return the matches multiple times.
    #[test]
    fn test_find_functions_no_duplicate_same_file_same_name() {
        let mut repo_map = RepoMap::new();
        let path = "/test/dup.rs".to_string();
        let mut node = TreeNode::new(path.clone(), "rust".to_string());
        node.functions
            .push(FunctionSignature::new("ingest".to_string(), path.clone()).with_location(1, 2));
        node.functions.push(
            FunctionSignature::new("ingest".to_string(), path.clone())
                .with_owner("Thing")
                .with_location(5, 7),
        );
        node.content_hash = "h_dup".to_string();
        repo_map.add_file(node).unwrap();

        let result = repo_map.find_functions("ingest");
        assert_eq!(
            result.items.len(),
            2,
            "each definition counted exactly once"
        );
        let owners: Vec<Option<&str>> = result.items.iter().map(|f| f.owner.as_deref()).collect();
        assert!(owners.contains(&None));
        assert!(owners.contains(&Some("Thing")));
    }

    // A file defining a name twice (trait signature + impl) is ONE definition
    // file, not a cross-file collision — so it must not be flagged ambiguous.
    #[test]
    fn test_name_defined_twice_in_one_file_is_not_a_collision() {
        let mut repo_map = RepoMap::new();
        let path = "/test/repo.rs".to_string();
        let mut node = TreeNode::new(path.clone(), "rust".to_string());
        node.functions
            .push(FunctionSignature::new("get".to_string(), path.clone()).with_location(1, 1));
        node.functions.push(
            FunctionSignature::new("get".to_string(), path.clone())
                .with_owner("S")
                .with_location(3, 5),
        );
        node.content_hash = "h".to_string();
        repo_map.add_file(node).unwrap();

        assert_eq!(repo_map.name_definition_file_count("get"), 1);
        assert_eq!(
            repo_map.function_definition_files("get"),
            vec!["/test/repo.rs"]
        );
    }

    // function_owner must not guess when a file has same-named methods with
    // different owners; it returns None rather than the first (possibly wrong) one.
    #[test]
    fn test_function_owner_ambiguous_within_file_returns_none() {
        let mut repo_map = RepoMap::new();
        let path = "/test/build.rs".to_string();
        let mut node = TreeNode::new(path.clone(), "rust".to_string());
        node.functions.push(
            FunctionSignature::new("build".to_string(), path.clone())
                .with_owner("Foo")
                .with_location(1, 3),
        );
        node.functions.push(
            FunctionSignature::new("build".to_string(), path.clone())
                .with_owner("Bar")
                .with_location(5, 7),
        );
        node.functions.push(
            FunctionSignature::new("only".to_string(), path.clone())
                .with_owner("Foo")
                .with_location(9, 10),
        );
        node.content_hash = "h".to_string();
        repo_map.add_file(node).unwrap();

        // Conflicting owners for `build` -> don't guess.
        assert_eq!(repo_map.function_owner(&path, "build"), None);
        assert_eq!(repo_map.qualified_function_name(&path, "build"), "build");
        // Unambiguous name still qualifies.
        assert_eq!(
            repo_map.function_owner(&path, "only"),
            Some("Foo".to_string())
        );
        assert_eq!(repo_map.qualified_function_name(&path, "only"), "Foo::only");
    }

    // P0-1 regression: call_graph must not accumulate stale/duplicate call sites across
    // file re-add and removal (previously remove_file_by_index skipped the call_graph).
    #[test]
    fn test_call_graph_no_duplicates_on_readd() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");

        repo_map.add_file(node.clone()).unwrap();
        let before = repo_map.find_function_callers("call_test").len();
        assert_eq!(before, 1);

        // Re-adding the same file (watch mode / re-scan) must not duplicate call sites.
        repo_map.add_file(node.clone()).unwrap();
        let after = repo_map.find_function_callers("call_test").len();
        assert_eq!(after, before, "re-adding a file duplicated its call sites");
    }

    #[test]
    fn test_call_graph_cleared_on_remove() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");
        let file_path = node.file_path.clone();

        repo_map.add_file(node).unwrap();
        assert_eq!(repo_map.find_function_callers("call_test").len(), 1);

        repo_map.remove_file(&file_path).unwrap();

        // No CallSite from the removed file may survive.
        let callers = repo_map.find_function_callers("call_test");
        assert!(
            callers.iter().all(|c| c.file_path != file_path),
            "removed file left stale call sites in the call graph"
        );
        assert_eq!(callers.len(), 0);
    }

    #[test]
    fn test_call_graph_drops_emptied_keys_on_remove() {
        let mut repo_map = RepoMap::new();
        let node = create_test_tree_node("test", "rust");
        let file_path = node.file_path.clone();

        repo_map.add_file(node).unwrap();
        repo_map.remove_file(&file_path).unwrap();

        // The only key ("call_test") had all its sites removed, so it must not linger as an
        // empty Vec keyed in the map.
        assert!(
            repo_map.call_graph.values().all(|sites| !sites.is_empty()),
            "call_graph retained an empty CallSite vector after removal"
        );
        assert!(!repo_map.call_graph.contains_key("call_test"));
    }

    /// Build a TreeNode with an explicitly positioned outer and (nested) inner
    /// function plus a call, so we can assert enclosing-function resolution.
    fn tree_node_with_nested_call() -> TreeNode {
        let mut node = TreeNode::new("/test/nested.rs".to_string(), "rust".to_string());

        // outer spans lines 1..=20, inner is nested inside it spanning 5..=15.
        node.functions.push(
            FunctionSignature::new("outer".to_string(), node.file_path.clone())
                .with_location(1, 20),
        );
        node.functions.push(
            FunctionSignature::new("inner".to_string(), node.file_path.clone())
                .with_location(5, 15),
        );

        // A call on line 10 sits inside BOTH outer and inner; innermost = inner.
        node.function_calls.push(FunctionCall::new(
            "target".to_string(),
            node.file_path.clone(),
            10,
        ));
        // A call on line 18 sits inside outer only.
        node.function_calls.push(FunctionCall::new(
            "outer_only".to_string(),
            node.file_path.clone(),
            18,
        ));
        // A call on line 25 is outside every function (module-level).
        node.function_calls.push(FunctionCall::new(
            "top_level".to_string(),
            node.file_path.clone(),
            25,
        ));

        node.content_hash = "hash_nested".to_string();
        node
    }

    #[test]
    fn test_caller_function_populated_for_nested_call() {
        let mut repo_map = RepoMap::new();
        repo_map.add_file(tree_node_with_nested_call()).unwrap();

        // Innermost enclosing function wins for the nested call.
        let target_callers = repo_map.find_function_callers("target");
        assert_eq!(target_callers.len(), 1);
        assert_eq!(
            target_callers[0].caller_function,
            Some("inner".to_string()),
            "nested call should resolve to the innermost enclosing function"
        );

        // A call inside only the outer function resolves to outer.
        let outer_callers = repo_map.find_function_callers("outer_only");
        assert_eq!(outer_callers[0].caller_function, Some("outer".to_string()));

        // A module-level call has no enclosing function.
        let top_callers = repo_map.find_function_callers("top_level");
        assert_eq!(top_callers[0].caller_function, None);
    }

    #[test]
    fn test_transitive_callers_multi_level() {
        // Build a chain across files: a -> b -> c -> parse_config
        // Each function calls the next; we assert BFS depth + cycle safety.
        let mut repo_map = RepoMap::new();

        let mut mk = |fname: &str, calls: &str, line: u32| {
            let path = format!("/test/{}.rs", fname);
            let mut node = TreeNode::new(path.clone(), "rust".to_string());
            node.functions
                .push(FunctionSignature::new(fname.to_string(), path.clone()).with_location(1, 10));
            node.function_calls
                .push(FunctionCall::new(calls.to_string(), path.clone(), line));
            node.content_hash = format!("hash_{}", fname);
            node
        };

        repo_map.add_file(mk("a", "b", 5)).unwrap();
        repo_map.add_file(mk("b", "c", 5)).unwrap();
        repo_map.add_file(mk("c", "parse_config", 5)).unwrap();

        let callers = repo_map.transitive_callers("parse_config", 0);
        let names: Vec<(&str, usize)> = callers
            .iter()
            .map(|c| (c.function_name.as_str(), c.depth))
            .collect();
        assert_eq!(names, vec![("c", 1), ("b", 2), ("a", 3)]);

        // max_depth limits the walk.
        let shallow = repo_map.transitive_callers("parse_config", 1);
        assert_eq!(shallow.len(), 1);
        assert_eq!(shallow[0].function_name, "c");
    }

    #[test]
    fn test_transitive_callers_cycle_safe() {
        // a -> b -> a (cycle). Traversal must terminate.
        let mut repo_map = RepoMap::new();
        let mut mk = |fname: &str, calls: &str| {
            let path = format!("/test/{}.rs", fname);
            let mut node = TreeNode::new(path.clone(), "rust".to_string());
            node.functions
                .push(FunctionSignature::new(fname.to_string(), path.clone()).with_location(1, 10));
            node.function_calls
                .push(FunctionCall::new(calls.to_string(), path.clone(), 5));
            node.content_hash = format!("hash_{}", fname);
            node
        };
        repo_map.add_file(mk("a", "b")).unwrap();
        repo_map.add_file(mk("b", "a")).unwrap();

        let callers = repo_map.transitive_callers("a", 0);
        // b calls a (depth 1); a calls b but a is the target (visited) -> stop.
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].function_name, "b");
    }

    // P0-3: two functions named `load` in different files, each on its own caller
    // chain. Tracing `load` cannot attribute the direct callers to one definition
    // (name-keyed call graph), so both are flagged ambiguous — but the deeper,
    // uniquely-named callers on each chain resolve exactly and stay disjoint.
    #[test]
    fn test_transitive_callers_same_name_flagged_ambiguous() {
        let mut repo_map = RepoMap::new();

        // Build a file defining `load` plus a two-link caller chain into it.
        let mk_chain = |file: &str, caller: &str, top: &str| {
            let path = format!("/test/{}.rs", file);
            let mut node = TreeNode::new(path.clone(), "rust".to_string());
            node.functions
                .push(FunctionSignature::new("load".to_string(), path.clone()).with_location(1, 3));
            node.functions.push(
                FunctionSignature::new(caller.to_string(), path.clone()).with_location(5, 10),
            );
            node.functions
                .push(FunctionSignature::new(top.to_string(), path.clone()).with_location(12, 18));
            // caller() calls load(); top() calls caller().
            node.function_calls
                .push(FunctionCall::new("load".to_string(), path.clone(), 7));
            node.function_calls
                .push(FunctionCall::new(caller.to_string(), path.clone(), 14));
            node.content_hash = format!("hash_{}", file);
            node
        };

        repo_map
            .add_file(mk_chain("coll_a", "caller_a", "top_a"))
            .unwrap();
        repo_map
            .add_file(mk_chain("coll_b", "caller_b", "top_b"))
            .unwrap();

        let callers = repo_map.transitive_callers("load", 0);

        // Direct callers of `load` (depth 1): both chains cross here and both are
        // ambiguous, because `load` has two definitions.
        let direct: Vec<&TransitiveCaller> = callers.iter().filter(|c| c.depth == 1).collect();
        assert_eq!(direct.len(), 2, "both direct callers present");
        assert!(
            direct.iter().all(|c| c.ambiguous),
            "direct callers of an ambiguous name must be flagged, not exact"
        );
        let direct_names: HashSet<&str> = direct.iter().map(|c| c.function_name.as_str()).collect();
        assert_eq!(direct_names, HashSet::from(["caller_a", "caller_b"]));

        // Deeper callers (depth 2) have unique names, so they resolve exactly and
        // each stays attributed to its own file — the chains do not cross.
        let deep: Vec<&TransitiveCaller> = callers.iter().filter(|c| c.depth == 2).collect();
        assert_eq!(deep.len(), 2);
        assert!(
            deep.iter().all(|c| !c.ambiguous),
            "unique-named callers are exact"
        );
        let top_a = deep.iter().find(|c| c.function_name == "top_a").unwrap();
        assert_eq!(top_a.file_path, "/test/coll_a.rs");
        let top_b = deep.iter().find(|c| c.function_name == "top_b").unwrap();
        assert_eq!(top_b.file_path, "/test/coll_b.rs");
    }

    #[test]
    fn test_get_changed_files() {
        let mut repo_map = RepoMap::new();
        let timestamp = SystemTime::now();

        // Add a file before timestamp
        let mut old_node = create_test_tree_node("old", "rust");
        old_node.last_modified = timestamp - std::time::Duration::from_secs(60);
        repo_map.add_file(old_node).unwrap();

        // Add a file after timestamp
        let mut new_node = create_test_tree_node("new", "rust");
        new_node.last_modified = timestamp + std::time::Duration::from_secs(60);
        repo_map.add_file(new_node).unwrap();

        let changed_files = repo_map.get_changed_files(timestamp);
        assert_eq!(changed_files.len(), 1);
        assert!(changed_files[0].file_path.contains("new"));
    }

    #[test]
    fn test_fuzzy_search() {
        let mut repo_map = RepoMap::new();

        // Add files with various function and struct names
        let mut node = create_test_tree_node("example", "rust");
        node.functions.push(FunctionSignature::new(
            "calculate_hash".to_string(),
            node.file_path.clone(),
        ));
        node.functions.push(FunctionSignature::new(
            "parse_content".to_string(),
            node.file_path.clone(),
        ));
        node.structs.push(StructSignature::new(
            "Parser".to_string(),
            node.file_path.clone(),
        ));
        node.structs.push(StructSignature::new(
            "Calculator".to_string(),
            node.file_path.clone(),
        ));
        repo_map.add_file(node).unwrap();

        // Fuzzy search for "calc"
        let results = repo_map.fuzzy_search("calc", Some(10));
        assert!(!results.is_empty());

        // Should find both calculate_hash function and Calculator struct
        let calc_results: Vec<_> = results
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains("calc"))
            .collect();
        assert!(!calc_results.is_empty());
    }

    #[test]
    fn test_memory_usage() {
        let mut repo_map = RepoMap::new();
        let initial_usage = repo_map.get_memory_usage();

        // Add some files
        for i in 0..10 {
            let node = create_test_tree_node(&format!("test{}", i), "rust");
            repo_map.add_file(node).unwrap();
        }

        let after_usage = repo_map.get_memory_usage();
        assert!(after_usage > initial_usage);
        assert_eq!(repo_map.get_metadata().memory_usage_bytes, after_usage);
    }

    #[test]
    fn test_query_cache() {
        let mut repo_map = RepoMap::new().with_cache_ttl(1); // 1 second TTL
        let node = create_test_tree_node("test", "rust");
        repo_map.add_file(node).unwrap();

        // First query - should be uncached
        let result1 = repo_map.find_functions("function_test");
        assert_eq!(result1.items.len(), 1);

        // Clear cache manually
        repo_map.clear_cache();

        // Query again - should work the same
        let result2 = repo_map.find_functions("function_test");
        assert_eq!(result2.items.len(), 1);
    }

    #[test]
    fn test_metadata_updates() {
        let mut repo_map = RepoMap::new();

        // Initial metadata
        let metadata = repo_map.get_metadata();
        assert_eq!(metadata.total_files, 0);
        assert_eq!(metadata.total_functions, 0);
        assert_eq!(metadata.total_structs, 0);
        assert!(metadata.languages.is_empty());

        // Add a file and check metadata updates
        let node = create_test_tree_node("test", "rust");
        repo_map.add_file(node).unwrap();

        let metadata = repo_map.get_metadata();
        assert_eq!(metadata.total_files, 1);
        assert_eq!(metadata.total_functions, 1);
        assert_eq!(metadata.total_structs, 1);
        assert_eq!(metadata.total_imports, 1);
        assert_eq!(metadata.total_exports, 1);
        assert!(metadata.languages.contains("rust"));
    }

    #[test]
    fn test_complex_indexing_scenario() {
        let mut repo_map = RepoMap::new();

        // Add multiple files with overlapping function names
        for i in 0..5 {
            let mut node = create_test_tree_node(&format!("file{}", i), "rust");

            // Add a common function name
            node.functions.push(FunctionSignature::new(
                "common_function".to_string(),
                node.file_path.clone(),
            ));

            // Add unique function
            node.functions.push(FunctionSignature::new(
                format!("unique_func_{}", i),
                node.file_path.clone(),
            ));

            repo_map.add_file(node).unwrap();
        }

        // Search for common function - should find 5 instances
        let results = repo_map.find_functions("common_function");
        assert_eq!(results.items.len(), 5);

        // Search for unique function - should find 1 instance
        let results = repo_map.find_functions("unique_func_2");
        assert_eq!(results.items.len(), 1);

        // Remove one file and verify indexes are updated correctly
        repo_map.remove_file("/test/file2.rs").unwrap();

        // Common function should now have 4 instances
        let results = repo_map.find_functions("common_function");
        assert_eq!(results.items.len(), 4);

        // unique_func_2 should no longer exist
        let results = repo_map.find_functions("unique_func_2");
        assert_eq!(results.items.len(), 0);

        // But unique_func_3 should still exist
        let results = repo_map.find_functions("unique_func_3");
        assert_eq!(results.items.len(), 1);
    }

    #[test]
    fn test_pattern_matching() {
        let repo_map = RepoMap::new();

        // Test exact match
        assert!(repo_map.matches_pattern("test_function", "test_function"));

        // Test case insensitive match
        assert!(repo_map.matches_pattern("TestFunction", "testfunction"));

        // Test substring match
        assert!(repo_map.matches_pattern("test_function_with_params", "function"));

        // Test regex pattern (if it looks like regex)
        assert!(repo_map.matches_pattern("test_function", "test_.*"));

        // Test non-matches
        assert!(!repo_map.matches_pattern("other_function", "test"));
    }
}
