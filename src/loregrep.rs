use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::analyzers::{
    python::PythonAnalyzer,
    registry::{DefaultLanguageRegistry, LanguageAnalyzerRegistry, RegistryHandle},
    rust::RustAnalyzer,
    traits::LanguageAnalyzer,
    typescript::TypeScriptAnalyzer,
};
use crate::core::{LoreGrepError, Result, ScanResult, ToolResult, ToolSchema};
use crate::internal::{ai_tools::LocalAnalysisTools, config::FileScanningConfig};
use crate::scanner::discovery::RepositoryScanner;
use crate::storage::memory::RepoMap;
use crate::storage::persistence::{CoverageHandle, IndexCoverage, PersistenceManager};

/// The main struct for interacting with LoreGrep
#[derive(Clone)]
pub struct LoreGrep {
    repo_map: Arc<Mutex<RepoMap>>,
    scanner: RepositoryScanner,
    tools: LocalAnalysisTools,
    config: LoreGrepConfig,
    language_registry: Arc<DefaultLanguageRegistry>,
    /// How much of the repository the current index covers. Written by
    /// [`LoreGrep::scan`] and by a cache load; read when rendering tool results
    /// so a truncated index can never answer as if it were complete.
    coverage: CoverageHandle,
}

/// Configuration for LoreGrep
#[derive(Debug, Clone)]
pub struct LoreGrepConfig {
    pub max_files: Option<usize>,
    pub cache_ttl_seconds: u64,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub max_file_size: u64,
    pub max_depth: Option<u32>,
    pub follow_symlinks: bool,
    pub respect_gitignore: bool,
}

impl LoreGrepConfig {
    /// The one default file limit, used by every entry point that does not take
    /// an explicit one (library builder and CLI alike).
    ///
    /// 10,000 rather than 5,000: the limit exists to bound memory on a runaway
    /// tree, not to sample a repository, and a *silently* partial index is the
    /// expensive failure — the lower value truncated real repositories (the
    /// Linux kernel, large monorepos, node_modules-heavy trees) well before
    /// memory became a problem. Truncation is now loud and uncacheable (see
    /// [`LoreGrep::scan`]), so the higher limit costs a little memory in the
    /// worst case and buys a complete index in the common one. Raise it with
    /// `LoreGrepBuilder::max_files` when a repository legitimately exceeds it.
    pub const DEFAULT_MAX_FILES: usize = 10_000;
}

impl Default for LoreGrepConfig {
    fn default() -> Self {
        Self {
            max_files: Some(Self::DEFAULT_MAX_FILES),
            cache_ttl_seconds: 300, // 5 minutes
            include_patterns: vec![
                "**/*.rs".to_string(),
                "**/*.py".to_string(),
                "**/*.pyx".to_string(),
                "**/*.pyi".to_string(),
                "**/*.ts".to_string(),
                "**/*.tsx".to_string(),
                "**/*.mts".to_string(),
                "**/*.cts".to_string(),
                "**/*.js".to_string(),
                "**/*.jsx".to_string(),
                "**/*.mjs".to_string(),
                "**/*.cjs".to_string(),
            ],
            exclude_patterns: vec![
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/node_modules/**".to_string(),
                "**/test-repos/**".to_string(),
                // TypeScript declaration files are generated type stubs (no
                // executable code) and only add noise to the index.
                "**/*.d.ts".to_string(),
                // Python virtual environments
                "**/venv/**".to_string(),
                "**/env/**".to_string(),
                "**/.venv/**".to_string(),
                "**/.env/**".to_string(),
                "**/mcpenv/**".to_string(),
                "**/site-packages/**".to_string(),
                "**/__pycache__/**".to_string(),
                "**/*.pyc".to_string(),
                "**/.pytest_cache/**".to_string(),
                // Build and cache directories
                "**/dist/**".to_string(),
                "**/build/**".to_string(),
                "**/.cache/**".to_string(),
            ],
            max_file_size: 1024 * 1024, // 1MB
            max_depth: Some(20),
            follow_symlinks: false,
            respect_gitignore: true,
        }
    }
}

impl LoreGrep {
    /// Create a new builder for configuring LoreGrep
    pub fn builder() -> LoreGrepBuilder {
        LoreGrepBuilder::new()
    }

    /// Automatically detect project type and configure appropriate analyzers
    pub fn auto_discover<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let detected_languages = Self::detect_project_languages(&path);

        if detected_languages.is_empty() {
            eprintln!(
                "No known project types detected in {}",
                path.as_ref().display()
            );
            eprintln!("Using default configuration (Rust + Python)");
        } else {
            eprintln!(
                "Detected project languages: {}",
                detected_languages.join(", ")
            );
        }

        let mut builder = Self::builder();

        // Register analyzers based on detection. Track whether every detected
        // language has a matching analyzer: languages such as "javascript" or
        // "go" are detected but have no analyzer yet, and a project made up only
        // of those would otherwise fall through registering nothing useful (and
        // then only the default Rust analyzer would be added at build time,
        // silently dropping all supported files).
        let mut all_detected_have_analyzer = true;
        let mut has_ts_analyzer = false;
        for language in &detected_languages {
            builder = match language.as_str() {
                "rust" => builder.with_rust_analyzer(),
                "python" => builder.with_python_analyzer(),
                // JavaScript is handled by the same analyzer (TSX grammar), so a
                // project detected as both registers it once — registering twice
                // is an error.
                "typescript" | "javascript" if has_ts_analyzer => builder,
                "typescript" | "javascript" => {
                    has_ts_analyzer = true;
                    builder.with_typescript_analyzer()
                }
                _ => {
                    all_detected_have_analyzer = false;
                    builder
                }
            };
        }

        // If nothing was detected, or a detected language has no analyzer yet,
        // register the full set of available analyzers so the instance never
        // ends up indexing nothing for supported file types. (A JS-only project
        // still gets rust+python+typescript; JS itself remains unhandled until
        // an analyzer exists, which is fine.)
        if detected_languages.is_empty() || !all_detected_have_analyzer {
            builder = builder.with_all_analyzers();
        }

        // Configure file patterns based on detected languages
        builder = builder.configure_patterns_for_languages(&detected_languages);

        builder.build()
    }

    /// Preset for Rust projects (Cargo projects)
    pub fn rust_project<P: AsRef<std::path::Path>>(_path: P) -> Result<Self> {
        Self::builder()
            .with_rust_analyzer()
            .include_patterns(vec!["**/*.rs".to_string(), "**/*.toml".to_string()])
            .exclude_patterns(vec!["**/target/**".to_string()])
            .build()
    }

    /// Preset for Python projects  
    pub fn python_project<P: AsRef<std::path::Path>>(_path: P) -> Result<Self> {
        Self::builder()
            .with_python_analyzer()
            .include_patterns(vec![
                "**/*.py".to_string(),
                "**/*.pyx".to_string(),
                "**/*.pyi".to_string(),
            ])
            .exclude_patterns(vec![
                "**/__pycache__/**".to_string(),
                "**/venv/**".to_string(),
                "**/.venv/**".to_string(),
                "**/env/**".to_string(),
                "**/.env/**".to_string(),
            ])
            .build()
    }

    /// Preset for polyglot projects (multiple languages)
    pub fn polyglot_project<P: AsRef<std::path::Path>>(_path: P) -> Result<Self> {
        Self::builder()
            .with_rust_analyzer()
            .with_python_analyzer()
            .with_typescript_analyzer()
            .build()
    }

    /// Detect project languages based on file indicators  
    fn detect_project_languages<P: AsRef<std::path::Path>>(path: P) -> Vec<String> {
        let mut languages = Vec::new();
        let path = path.as_ref();

        // Rust project indicators
        if path.join("Cargo.toml").exists() || path.join("Cargo.lock").exists() {
            languages.push("rust".to_string());
        }

        // Python project indicators
        if path.join("pyproject.toml").exists()
            || path.join("requirements.txt").exists()
            || path.join("setup.py").exists()
            || path.join("poetry.lock").exists()
            || Self::has_python_files(path)
        {
            languages.push("python".to_string());
        }

        // TypeScript/JavaScript project indicators (for future support)
        if path.join("package.json").exists()
            || path.join("tsconfig.json").exists()
            || Self::has_ts_js_files(path)
        {
            // Treat the project as TypeScript whenever a tsconfig.json is present
            // OR any .ts/.tsx source file exists. A TS project without a
            // tsconfig.json (common for tooling that infers config) must still be
            // detected as "typescript" so that configure_patterns_for_languages
            // keeps .ts/.tsx in the include globs and the registered TypeScript
            // analyzer actually indexes them. Detecting only "javascript" here
            // would overwrite the include patterns with JS-only globs and silently
            // drop every .ts/.tsx file.
            if path.join("tsconfig.json").exists() || Self::has_ts_files(path) {
                languages.push("typescript".to_string());
            }
            languages.push("javascript".to_string());
        }

        // Go project indicators (for future support)
        if path.join("go.mod").exists() || path.join("go.sum").exists() {
            languages.push("go".to_string());
        }

        languages
    }

    /// Quick scan for Python files in common locations
    fn has_python_files<P: AsRef<std::path::Path>>(path: P) -> bool {
        let common_dirs = ["src", "lib", "app", ".", "scripts", "tests"];
        common_dirs.iter().any(|dir| {
            let dir_path = path.as_ref().join(dir);
            if let Ok(mut entries) = std::fs::read_dir(dir_path) {
                entries.any(|entry| {
                    if let Ok(entry) = entry {
                        entry
                            .path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map_or(false, |ext| ext == "py")
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        })
    }

    /// Quick scan for TypeScript/JavaScript files in common locations
    fn has_ts_js_files<P: AsRef<std::path::Path>>(path: P) -> bool {
        let common_dirs = ["src", "lib", "app", ".", "components", "pages"];
        let js_extensions = ["ts", "tsx", "js", "jsx", "mjs"];

        common_dirs.iter().any(|dir| {
            let dir_path = path.as_ref().join(dir);
            if let Ok(mut entries) = std::fs::read_dir(dir_path) {
                entries.any(|entry| {
                    if let Ok(entry) = entry {
                        entry
                            .path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map_or(false, |ext| js_extensions.contains(&ext))
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        })
    }

    /// Quick scan for TypeScript (.ts/.tsx) files in common locations.
    ///
    /// Distinct from [`has_ts_js_files`](Self::has_ts_js_files): this only looks
    /// for TypeScript sources so that a project with `.ts`/`.tsx` files is
    /// labeled "typescript" (and keeps its `.ts`/`.tsx` include globs) even when
    /// no `tsconfig.json` is present.
    fn has_ts_files<P: AsRef<std::path::Path>>(path: P) -> bool {
        let common_dirs = ["src", "lib", "app", ".", "components", "pages"];
        let ts_extensions = ["ts", "tsx"];

        common_dirs.iter().any(|dir| {
            let dir_path = path.as_ref().join(dir);
            if let Ok(mut entries) = std::fs::read_dir(dir_path) {
                entries.any(|entry| {
                    if let Ok(entry) = entry {
                        entry
                            .path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map_or(false, |ext| ts_extensions.contains(&ext))
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        })
    }

    /// Scan a repository and build the in-memory index.
    ///
    /// This should be called by the host application, not exposed as a tool.
    ///
    /// # The index reflects exactly one root
    ///
    /// `scan(root)` **replaces** the entire index with the contents of `root`.
    /// It is idempotent: rescanning the same root yields the same index, and a
    /// file deleted since the last scan disappears from it. Scanning a
    /// *different* root replaces the index rather than accumulating into it.
    ///
    /// Accumulating would be defensible for a multi-repo index, but not here:
    /// `RepoMap` carries a single `scan_root`, and that root is what path
    /// containment for agent-supplied parameters is enforced against
    /// (`internal::paths`). An index holding files from two roots would have
    /// symbols that the tools must refuse to read — a worse failure than
    /// rescanning. Multi-root support is a `RepoMap` change (a root per file),
    /// not a `scan` change. Until then: one index, one root.
    ///
    /// # Truncation
    ///
    /// When `max_files` cuts the scan short, the index is *partial*. That is
    /// reported on stderr, recorded in [`LoreGrep::index_coverage`], attached to
    /// every tool result (`truncated: true` plus a human-readable note), and
    /// makes the index refuse to be cached — a partial index must never be
    /// reloaded as if it were authoritative.
    pub async fn scan(&mut self, path: &str) -> Result<ScanResult> {
        let start_time = std::time::Instant::now();

        let supported_langs = self.language_registry.list_supported_languages();
        if !supported_langs.is_empty() {
            eprintln!("Registered analyzers: {}", supported_langs.join(", "));
        }

        // Discover files
        let scan_result = self
            .scanner
            .scan(path)
            .map_err(|e| LoreGrepError::InternalError(format!("File scanning failed: {}", e)))?;
        // The scanner canonicalized the root; from here on THAT is the root, not
        // whatever string the caller typed. Every stored path is relative to it.
        let root = scan_result.root.to_string_lossy().to_string();
        let discovered_files = scan_result.files;

        if discovered_files.is_empty() {
            eprintln!("No files found in the specified path");
            eprintln!("Check that the path exists and contains supported file types");
            // An empty root still *replaces* the index: a rescan of a root whose
            // files all vanished must not keep serving them.
            self.replace_index(&root, Vec::new())?;
            self.coverage.set(IndexCoverage::complete(0));
            return Ok(ScanResult::new(
                0,
                0,
                0,
                start_time.elapsed().as_millis() as u64,
                Vec::new(),
            ));
        }

        let files_discovered = discovered_files.len();
        eprintln!("Found {} files to analyze", files_discovered);

        let mut files_scanned = 0;
        let mut functions_found = 0;
        let mut structs_found = 0;
        let mut languages = std::collections::HashSet::new();
        let mut analysis_results = Vec::new();

        // Analyze each file (without holding the mutex)
        let mut truncated_at: Option<usize> = None;
        for file_info in discovered_files {
            if let Some(max_files) = self.config.max_files {
                if files_scanned >= max_files {
                    truncated_at = Some(max_files);
                    break;
                }
            }

            // Read file content
            let content = match std::fs::read_to_string(&file_info.path) {
                Ok(content) => content,
                Err(_) => continue, // Skip files we can't read
            };

            // Dispatch to the analyzer registered for this file's language.
            // Analyzers are shared `Arc`s owned by the registry, so this reuses a
            // single instance per language instead of constructing one per file.
            // The file's IDENTITY is its normalized root-relative path — one
            // string per file, independent of how `--path` was spelled and of
            // where the caller's shell stood. The absolute `file_info.path` is
            // used to READ, never to name.
            let path_str = file_info.index_path().into_string();
            let analysis_result = match self.language_registry.get_analyzer_for_path(&path_str) {
                Some(analyzer) => analyzer.analyze_file(&content, &path_str).await,
                None => {
                    // No analyzer registered for this file's language. Files with
                    // unrecognized languages are skipped by discovery already; this
                    // guards against any that slip through.
                    let supported_langs = self.language_registry.list_supported_languages();
                    if supported_langs.is_empty() {
                        eprintln!(
                            "No language analyzers registered! Use LoreGrep::builder().with_rust_analyzer() or .with_python_analyzer()"
                        );
                    } else {
                        eprintln!(
                            "No analyzer available for '{}' files. Supported: {}",
                            file_info.language,
                            supported_langs.join(", ")
                        );
                    }
                    continue;
                }
            };

            match analysis_result {
                Ok(analysis) => {
                    functions_found += analysis.tree_node.functions.len();
                    structs_found += analysis.tree_node.structs.len();
                    languages.insert(file_info.language.clone());

                    // Store analysis for later addition to repo map
                    analysis_results.push(analysis.tree_node);
                    files_scanned += 1;
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to analyze {}: {}",
                        file_info.path.display(),
                        e
                    );
                }
            }
        }

        // Install the results as THE index for this root (holding mutex only
        // briefly). This is a replacement, not an accumulation — see the doc
        // comment on this method.
        self.replace_index(&root, analysis_results)?;

        // Record how much of the repository this index actually covers, and say
        // so loudly when it is partial: "Found 10050 files" followed by "Files
        // analyzed: 10000" is not a warning, it is a puzzle.
        let coverage = match truncated_at {
            Some(limit) => {
                eprintln!(
                    "WARNING: index is TRUNCATED — analyzed {} of {} discovered files \
                     (max_files = {}).",
                    files_scanned, files_discovered, limit
                );
                eprintln!(
                    "         Searches may report a symbol as missing when it merely lives in \
                     the {} files that were dropped.",
                    files_discovered.saturating_sub(files_scanned)
                );
                eprintln!(
                    "         Raise the limit (max_files) and rescan for a complete index; \
                     a truncated index is not cached."
                );
                IndexCoverage::partial(files_scanned, files_discovered)
            }
            None => IndexCoverage::complete(files_scanned),
        };
        self.coverage.set(coverage);

        let duration = start_time.elapsed();

        // Print scan summary with enhanced feedback
        self.print_scan_summary(
            files_scanned,
            functions_found,
            structs_found,
            &languages,
            duration,
        );

        Ok(ScanResult::new(
            files_scanned,
            functions_found,
            structs_found,
            duration.as_millis() as u64,
            languages.into_iter().collect(),
        ))
    }

    /// Replace the whole index with `files`, recorded as the contents of `root`.
    ///
    /// This is what makes [`LoreGrep::scan`] idempotent: everything the previous
    /// scan left behind — including files that have since been deleted, and
    /// files belonging to a previously scanned root — is gone before the new
    /// contents are installed.
    fn replace_index(&self, root: &str, files: Vec<crate::types::TreeNode>) -> Result<()> {
        let mut repo_map = self
            .repo_map
            .lock()
            .map_err(|e| LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e)))?;

        *repo_map = RepoMap::new();

        // Agent-supplied paths resolve against this, and nothing outside it
        // may be read (internal::paths). Recorded here because it is the one
        // place the caller's intent is unambiguous.
        repo_map.set_scan_root(root);

        for tree_node in files {
            if let Err(e) = repo_map.add_file(tree_node) {
                eprintln!("Warning: Failed to store analysis: {}", e);
            }
        }
        Ok(())
    }

    /// How much of the repository the current index covers.
    ///
    /// `truncated` is true when a `max_files` limit stopped the scan short. Any
    /// consumer that reports "not found" to a human or an agent should consult
    /// this first — see [`IndexCoverage::note`].
    pub fn index_coverage(&self) -> IndexCoverage {
        self.coverage.get()
    }

    /// The shared coverage handle backing [`LoreGrep::index_coverage`].
    ///
    /// Exposed so the tool layer (`internal::ai_tools::LocalAnalysisTools`) can
    /// hold the same handle and stamp coverage onto the responses it builds,
    /// rather than depending on `LoreGrep` itself. Cloning it is cheap; both
    /// sides observe the same value.
    pub fn coverage_handle(&self) -> CoverageHandle {
        self.coverage.clone()
    }

    /// Get tool definitions for adding to LLM system prompts
    /// Returns JSON Schema compatible tool definitions
    pub fn get_tool_definitions() -> Vec<ToolSchema> {
        // Create a temporary instance to get schemas. Tool schemas are static and
        // do not depend on any registered analyzer, so an empty registry handle
        // suffices.
        let temp_repo_map = Arc::new(Mutex::new(RepoMap::new()));
        let temp_registry = DefaultLanguageRegistry::new();
        let temp_tools =
            LocalAnalysisTools::new(temp_repo_map, RegistryHandle::new(&temp_registry));
        temp_tools.get_tool_schemas()
    }

    /// Execute a tool call from the LLM
    /// Takes tool name and parameters, returns JSON result
    pub async fn execute_tool(&self, name: &str, params: Value) -> Result<ToolResult> {
        let ai_result = self
            .tools
            .execute_tool(name, params)
            .await
            .map_err(|e| LoreGrepError::ToolError(format!("Tool execution failed: {}", e)))?;

        // Convert from ai_tools::ToolResult to core::types::ToolResult.
        //
        // This boundary used to read ONLY `.error` and discard `data`, so a
        // handler reporting failure via `error_with_data` (which leaves `.error`
        // as None by design) had its diagnostic replaced with the literal string
        // "Unknown error" — the agent-facing symptom of the analyze_file bug was
        // manufactured here, not there. A failure must never reach a caller
        // without a message it can act on, so: prefer the explicit error, fall
        // back to a message carried in `data`, and if a handler somehow supplies
        // neither, say which tool failed and hand back the data instead of
        // swallowing it.
        // A result computed from a TRUNCATED index must say so. Without this a
        // search for a symbol that lives in the dropped tail returns
        // `success: true` with a handful of prefix near-misses and teaches the
        // caller a false fact with full confidence.
        let ai_result = self.annotate_coverage(ai_result);

        if ai_result.success {
            Ok(ToolResult::success(ai_result.data))
        } else {
            let message = ai_result
                .error
                .clone()
                .or_else(|| {
                    ai_result
                        .data
                        .get("error")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| format!("tool '{}' failed without reporting a reason", name));
            Ok(ToolResult {
                success: false,
                data: ai_result.data,
                error: Some(message),
            })
        }
    }

    /// Stamp index coverage onto a tool result.
    ///
    /// When the index is complete this is the identity function — complete is
    /// the normal case and its responses stay untouched. When the index is
    /// truncated, every response carries `truncated: true` and a
    /// `index_coverage` sentence stating how many files of how many are covered,
    /// so "no results" is never mistaken for "does not exist".
    ///
    /// NOTE FOR THE TOOL LAYER: this is the outermost, belt-and-braces stamp,
    /// applied at the public API boundary. The per-tool handlers in
    /// `internal::ai_tools` should additionally consult
    /// [`LoreGrep::coverage_handle`] so that results built there (and the
    /// summaries they render) are coverage-aware at the point they are
    /// constructed. Both stamps write the same two keys with the same values,
    /// so applying both is harmless.
    fn annotate_coverage(
        &self,
        mut result: crate::internal::ai_tools::ToolResult,
    ) -> crate::internal::ai_tools::ToolResult {
        let coverage = self.coverage.get();
        let Some(note) = coverage.note() else {
            return result;
        };
        if let Value::Object(map) = &mut result.data {
            map.insert("truncated".to_string(), Value::Bool(true));
            map.insert("index_coverage".to_string(), Value::String(note));
            map.insert(
                "indexed_file_count".to_string(),
                Value::from(coverage.files_indexed),
            );
            map.insert(
                "discovered_file_count".to_string(),
                Value::from(coverage.files_discovered),
            );
        }
        result
    }

    /// Record the analysis root on the index. Needed after a cache load, which
    /// does not carry it; a scan sets it itself.
    pub fn set_scan_root(&mut self, root: &str) {
        if let Ok(mut repo_map) = self.repo_map.lock() {
            repo_map.set_scan_root(root);
        }
    }

    /// Check if repository has been scanned
    pub fn is_scanned(&self) -> bool {
        let repo_map = match self.repo_map.lock() {
            Ok(map) => map,
            Err(_) => return false,
        };
        repo_map.get_metadata().total_files > 0
    }

    /// Return the stable, per-repository cache path for a scanned root:
    /// `<root>/.loregrep/index.cache`.
    ///
    /// This is the location the CLI reads from / writes to so that repeated
    /// invocations on the same repository can reuse a previously persisted
    /// index instead of rescanning. The containing `.loregrep/` directory is
    /// created on demand by [`LoreGrep::save_index`].
    pub fn default_cache_path<P: AsRef<Path>>(root: P) -> PathBuf {
        root.as_ref().join(".loregrep").join("index.cache")
    }

    /// Split a full cache file path (e.g. `<root>/.loregrep/index.cache`) into
    /// the `(directory, name)` pair expected by [`PersistenceManager`], where
    /// `name` is the file stem (`index`) and the manager re-appends `.cache`.
    fn cache_dir_and_name(cache_path: &Path) -> Result<(PathBuf, String)> {
        let dir = cache_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let name = cache_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                LoreGrepError::InternalError(format!("Invalid cache path: {:?}", cache_path))
            })?
            .to_string();
        Ok((dir, name))
    }

    /// Fingerprint of everything that determines *which* files land in the index
    /// and *how* they are analyzed: the include/exclude globs, the size/depth
    /// limits, symlink and gitignore handling, the file limit, and the set of
    /// registered analyzers.
    ///
    /// A cache is only reusable by a run with an identical fingerprint. Without
    /// this, a cache built by a `*.rs`-only configuration is happily served to a
    /// run that also indexes Python, and the Python files stay silently absent.
    fn config_fingerprint(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        let mut field = |key: &str, value: &str| {
            hasher.update(key.as_bytes());
            hasher.update(b"=");
            hasher.update(value.as_bytes());
            hasher.update(b";");
        };

        // Patterns are order-insensitive to the scanner, so sort them: two
        // configurations that differ only in ordering are the same cache.
        let mut include = self.config.include_patterns.clone();
        include.sort();
        let mut exclude = self.config.exclude_patterns.clone();
        exclude.sort();
        let mut analyzers = self.language_registry.list_supported_languages();
        analyzers.sort();

        field("include", &include.join(","));
        field("exclude", &exclude.join(","));
        field("max_file_size", &self.config.max_file_size.to_string());
        field("max_depth", &format!("{:?}", self.config.max_depth));
        field("max_files", &format!("{:?}", self.config.max_files));
        field("follow_symlinks", &self.config.follow_symlinks.to_string());
        field(
            "respect_gitignore",
            &self.config.respect_gitignore.to_string(),
        );
        field("analyzers", &analyzers.join(","));

        hasher.finalize().to_hex().to_string()
    }

    /// Persist the current in-memory index to `cache_path` (gzip-compressed
    /// JSON via [`PersistenceManager`]). Creates the parent directory if needed.
    ///
    /// The written cache embeds the cache format version, the crate version, a
    /// fingerprint of the configuration that produced it and a per-file content
    /// hash, so a cache this build must not trust is rejected on load rather
    /// than silently serving wrong results.
    ///
    /// A **truncated** index is refused: it is a partial view of the repository,
    /// and persisting it would let a later run reload it as if it were the whole
    /// thing. Callers treat the error as "nothing to cache", not as a failure.
    pub fn save_index(&self, cache_path: &Path) -> Result<()> {
        let coverage = self.coverage.get();
        if coverage.truncated {
            return Err(LoreGrepError::InternalError(format!(
                "refusing to cache a truncated index ({} of {} files); \
                 raise max_files and rescan",
                coverage.files_indexed, coverage.files_discovered
            )));
        }

        let (dir, name) = Self::cache_dir_and_name(cache_path)?;
        let manager = PersistenceManager::new(&dir)?
            .with_config_fingerprint(self.config_fingerprint())
            .with_coverage(coverage);

        let repo_map = self
            .repo_map
            .lock()
            .map_err(|e| LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e)))?;
        manager.save_to_disk(&repo_map, &name)?;
        Ok(())
    }

    /// Load a persisted index from `cache_path`, replacing the current in-memory
    /// index. After a successful load, [`LoreGrep::is_scanned`] reflects the
    /// loaded state.
    ///
    /// [`PersistenceManager::load_index`] enforces the identity gates (cache
    /// format version, crate version, configuration fingerprint, header/payload
    /// agreement). It does NOT compare the cache against the files on disk —
    /// use [`LoreGrep::load_index_if_fresh`] for that.
    pub fn load_index(&self, cache_path: &Path) -> Result<()> {
        let (dir, name) = Self::cache_dir_and_name(cache_path)?;
        let manager =
            PersistenceManager::new(&dir)?.with_config_fingerprint(self.config_fingerprint());
        let loaded = manager.load_index(&name)?;

        if loaded.coverage.truncated {
            return Err(LoreGrepError::InternalError(
                "cache holds a truncated index; refusing to load it as authoritative".to_string(),
            ));
        }

        let mut repo_map = self
            .repo_map
            .lock()
            .map_err(|e| LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e)))?;
        *repo_map = loaded.repo_map;
        self.coverage.set(loaded.coverage);
        Ok(())
    }

    /// Load `cache_path` only if it still describes `root` exactly. Returns
    /// `Ok(true)` when the index was installed, `Ok(false)` when the cache was
    /// missing or stale (the current index is left untouched).
    ///
    /// This is the single validated entry point callers should use; it reads the
    /// cache once rather than validating and then loading again.
    pub fn load_index_if_fresh(&self, cache_path: &Path, root: &Path) -> Result<bool> {
        match self.validated_cache(cache_path, root) {
            Ok(loaded) => {
                let mut repo_map = self.repo_map.lock().map_err(|e| {
                    LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e))
                })?;
                *repo_map = loaded.repo_map;
                // The cache DOES carry the analysis root now (and was refused
                // above unless it matched this run's), so this only re-states
                // it in canonical form.
                repo_map.set_scan_root(root.to_string_lossy().to_string());
                self.coverage.set(loaded.coverage);
                Ok(true)
            }
            Err(reason) => {
                if !reason.is_empty() {
                    eprintln!("Cache not used: {}", reason);
                }
                Ok(false)
            }
        }
    }

    /// Freshness gate for a persisted cache: `true` only when the cache both
    /// belongs to this build/configuration and still describes the files under
    /// `root` exactly.
    ///
    /// # Why not mtimes
    ///
    /// This used to compare the cache's mtime against the newest indexed source
    /// mtime. That is not a freshness test, it is an *edit* test, and it misses
    /// two whole classes of change:
    ///
    /// * a file **added** with a preserved older mtime (`cp -p`, `rsync -a`,
    ///   `tar -x`, git worktree materialization) is never newer than the cache,
    ///   so it stays permanently invisible;
    /// * a **deleted** file makes nothing newer than the cache, so its symbols
    ///   are served forever, with a `file_path` that no longer exists.
    ///
    /// The replacement is by construction rather than by heuristic:
    ///
    /// 1. the discovered path SET (same walker, same filters as a real scan) is
    ///    compared against the indexed path set — additions and deletions cannot
    ///    survive this, whatever their timestamps;
    /// 2. every file's content is hashed and compared against the `content_hash`
    ///    the index recorded for it — edits and mtime-preserving restores cannot
    ///    survive this either. (This hash was already computed and serialized on
    ///    every scan and never validated; here is where it earns its keep.)
    ///
    /// Step 1 is a metadata-only walk and rejects most stale caches before any
    /// file is read. Step 2 reads and hashes the sources, which costs a fraction
    /// of a rescan — parsing, not I/O, dominates a scan — so the cache still
    /// pays for itself.
    ///
    /// Known limitation: a discovered file that the scan itself would skip
    /// (unreadable, or rejected by its analyzer) is absent from the index by
    /// design. Unreadable files are recognized and ignored here; a file that
    /// only its analyzer rejects makes this return `false`, costing a rescan
    /// with the same outcome, never a wrong answer.
    pub fn is_cache_fresh(&self, cache_path: &Path, root: &Path) -> bool {
        self.validated_cache(cache_path, root).is_ok()
    }

    /// Load `cache_path` and check it against the files under `root`.
    /// `Err(reason)` explains why the cache cannot be used.
    fn validated_cache(
        &self,
        cache_path: &Path,
        root: &Path,
    ) -> std::result::Result<crate::storage::persistence::LoadedIndex, String> {
        if !cache_path.exists() {
            return Err(String::new()); // no cache is not an anomaly; stay quiet
        }

        let (dir, name) =
            Self::cache_dir_and_name(cache_path).map_err(|e| format!("bad cache path: {}", e))?;
        // The cache lives at `<root>/.loregrep/index.cache`, and every spelling
        // of `<root>` — a symlink to it, a run from another cwd — resolves to
        // that same file. Requiring the header's root to equal THIS run's
        // canonical root is what stops one invocation's vocabulary from being
        // served to another (K1/K9).
        let canonical_root = crate::scanner::discovery::canonical_root(root);
        let manager = PersistenceManager::new(&dir)
            .map_err(|e| format!("cache directory unusable: {}", e))?
            .with_config_fingerprint(self.config_fingerprint())
            .with_scan_root(canonical_root.to_string_lossy().to_string());
        let loaded = manager.load_index(&name).map_err(|e| e.to_string())?;

        if loaded.coverage.truncated {
            return Err("cache holds a truncated index".to_string());
        }

        // 1. Path sets must match exactly.
        let discovered = self
            .scanner
            .scan(root)
            .map_err(|e| format!("could not walk {}: {}", root.display(), e))?
            .files;

        let indexed: std::collections::HashMap<&str, &str> = loaded
            .repo_map
            .get_all_files()
            .iter()
            .map(|f| (f.file_path.as_str(), f.content_hash.as_str()))
            .collect();

        let mut seen = 0usize;
        for file in &discovered {
            // Compare in the index's vocabulary, not the walker's.
            let path_str = file.index_path().into_string();
            let Some(indexed_hash) = indexed.get(path_str.as_str()) else {
                // Not in the index. If a scan could not have read it either, the
                // absence is expected; otherwise the file is genuinely new.
                match std::fs::read_to_string(&file.path) {
                    Ok(_) => {
                        return Err(format!("{} is not in the index", path_str));
                    }
                    Err(_) => continue,
                }
            };
            seen += 1;

            // 2. Contents must match the hash recorded at index time. This is
            //    what catches an mtime-preserving restore.
            let content = match std::fs::read_to_string(&file.path) {
                Ok(content) => content,
                Err(e) => return Err(format!("{} is no longer readable: {}", path_str, e)),
            };
            if blake3::hash(content.as_bytes()).to_hex().to_string() != *indexed_hash {
                return Err(format!("{} changed since it was indexed", path_str));
            }
        }

        // Anything indexed but not discovered is a deletion (or a file that has
        // moved out of the configured set).
        if seen != indexed.len() {
            return Err(format!(
                "{} indexed file(s) no longer exist under {}",
                indexed.len() - seen,
                root.display()
            ));
        }

        Ok(loaded)
    }

    /// Return the first indexed file path that no longer exists on disk, if any.
    ///
    /// [`LoreGrep::is_cache_fresh`] already rejects a cache whose path set no
    /// longer matches the repository, so this is no longer needed as a deletion
    /// guard around a cache load. It remains as a cheap consistency probe for
    /// callers that hold an index of unknown provenance (e.g. one loaded via the
    /// unvalidated [`LoreGrep::load_index`]).
    pub fn first_missing_indexed_path(&self) -> Option<String> {
        let repo_map = self.repo_map.lock().ok()?;
        // Stored paths are root-relative, so existence is checked against the
        // recorded analysis root — never against the process cwd, which is the
        // whole class of bug this refactor closed.
        repo_map
            .get_all_files()
            .iter()
            .map(|file| file.file_path.clone())
            .find(|path| match repo_map.absolute_path(path) {
                Some(abs) => !abs.exists(),
                None => !Path::new(path).exists(),
            })
    }

    /// Reset the in-memory index to empty, including its coverage.
    ///
    /// [`LoreGrep::scan`] now replaces the index wholesale, so this is no longer
    /// required before a rescan; it remains for hosts that want to drop an index
    /// (and its memory) without immediately building another.
    ///
    /// NOTE: this is not exposed on the Python bindings (`src/lib.rs`), so a
    /// Python host has no way to drop an index short of dropping the object.
    pub fn clear_index(&self) {
        if let Ok(mut repo_map) = self.repo_map.lock() {
            *repo_map = RepoMap::new();
        }
        self.coverage.set(IndexCoverage::default());
    }

    /// Print a comprehensive scan summary with language breakdown
    fn print_scan_summary(
        &self,
        files_scanned: usize,
        functions_found: usize,
        structs_found: usize,
        languages: &std::collections::HashSet<String>,
        duration: std::time::Duration,
    ) {
        if files_scanned == 0 {
            eprintln!("Scan Summary:");
            eprintln!("  No files found matching your criteria");
            eprintln!("  Check your include/exclude patterns or language analyzers");
            eprintln!(
                "  Supported languages: {:?}",
                self.language_registry.list_supported_languages()
            );
            return;
        }

        eprintln!("Scan Summary:");
        eprintln!("  Files analyzed: {}", files_scanned);
        eprintln!("  Functions found: {}", functions_found);
        eprintln!("  Structs found: {}", structs_found);
        eprintln!(
            "  Languages detected: {:?}",
            languages.iter().cloned().collect::<Vec<_>>()
        );
        eprintln!("  Scan duration: {:.2}s", duration.as_secs_f64());
    }

    /// Get repository statistics
    pub fn get_stats(&self) -> Result<ScanResult> {
        let repo_map = self
            .repo_map
            .lock()
            .map_err(|e| LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e)))?;

        let metadata = repo_map.get_metadata();

        Ok(ScanResult::new(
            metadata.total_files,
            metadata.total_functions,
            metadata.total_structs,
            0, // No duration for stats
            metadata.languages.iter().cloned().collect(),
        ))
    }
}

/// Builder for configuring LoreGrep instances
#[derive(Clone)]
pub struct LoreGrepBuilder {
    config: LoreGrepConfig,
    registry: DefaultLanguageRegistry,
}

impl LoreGrepBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: LoreGrepConfig::default(),
            registry: DefaultLanguageRegistry::new(),
        }
    }

    /// Configure file patterns based on detected languages
    pub fn configure_patterns_for_languages(mut self, languages: &[String]) -> Self {
        let mut patterns = Vec::new();

        for language in languages {
            match language.as_str() {
                "rust" => patterns.extend(vec!["**/*.rs".to_string()]),
                "python" => patterns.extend(vec![
                    "**/*.py".to_string(),
                    "**/*.pyx".to_string(),
                    "**/*.pyi".to_string(),
                ]),
                "typescript" => patterns.extend(vec![
                    "**/*.ts".to_string(),
                    "**/*.tsx".to_string(),
                    "**/*.mts".to_string(),
                    "**/*.cts".to_string(),
                ]),
                "javascript" => patterns.extend(vec![
                    "**/*.js".to_string(),
                    "**/*.jsx".to_string(),
                    "**/*.mjs".to_string(),
                    "**/*.cjs".to_string(),
                ]),
                "go" => patterns.extend(vec!["**/*.go".to_string()]),
                _ => {}
            }
        }

        if !patterns.is_empty() {
            eprintln!(
                "Configuring file patterns for detected languages: {}",
                patterns.join(", ")
            );
            self.config.include_patterns = patterns;
        }

        self
    }

    /// Enable all available analyzers
    pub fn with_all_analyzers(self) -> Self {
        self.with_rust_analyzer()
            .with_python_analyzer()
            .with_typescript_analyzer()
    }

    /// Quick setup for common exclusions
    pub fn exclude_common_build_dirs(mut self) -> Self {
        self.config.exclude_patterns.extend(vec![
            "**/target/**".to_string(),       // Rust
            "**/build/**".to_string(),        // General
            "**/dist/**".to_string(),         // JavaScript/TypeScript
            "**/.build/**".to_string(),       // Xcode
            "**/node_modules/**".to_string(), // JavaScript/TypeScript
            "**/__pycache__/**".to_string(),  // Python
            "**/venv/**".to_string(),         // Python virtual env
            "**/.venv/**".to_string(),        // Python virtual env
        ]);
        self
    }

    /// Quick setup for common include patterns
    pub fn include_source_files(mut self) -> Self {
        self.config.include_patterns.extend(vec![
            "**/src/**/*.rs".to_string(), // Rust source
            "**/lib/**/*.py".to_string(), // Python libs
            "**/app/**/*.js".to_string(), // JavaScript apps
        ]);
        self
    }

    /// Quick setup for common test directories exclusion
    pub fn exclude_test_dirs(mut self) -> Self {
        self.config.exclude_patterns.extend(vec![
            "**/tests/**".to_string(),     // General test dirs
            "**/test/**".to_string(),      // General test dirs
            "**/*_test.rs".to_string(),    // Rust test files
            "**/*_test.py".to_string(),    // Python test files
            "**/test_*.py".to_string(),    // Python test files
            "**/*.test.js".to_string(),    // JavaScript test files
            "**/*.test.ts".to_string(),    // TypeScript test files
            "**/__tests__/**".to_string(), // Jest test dirs
        ]);
        self
    }

    /// Quick setup for vendor/dependency directories exclusion
    pub fn exclude_vendor_dirs(mut self) -> Self {
        self.config.exclude_patterns.extend(vec![
            "**/vendor/**".to_string(),      // General vendor dirs
            "**/vendors/**".to_string(),     // Alternative vendor naming
            "**/third_party/**".to_string(), // Third party code
            "**/extern/**".to_string(),      // External dependencies
            "**/.cargo/**".to_string(),      // Cargo cache
            "**/Pods/**".to_string(),        // CocoaPods
        ]);
        self
    }

    /// Include configuration files (useful for understanding project structure)
    pub fn include_config_files(mut self) -> Self {
        self.config.include_patterns.extend(vec![
            "**/Cargo.toml".to_string(),     // Rust config
            "**/pyproject.toml".to_string(), // Python config
            "**/package.json".to_string(),   // Node.js config
            "**/tsconfig.json".to_string(),  // TypeScript config
            "**/*.toml".to_string(),         // General TOML configs
            "**/*.yaml".to_string(),         // YAML configs
            "**/*.yml".to_string(),          // YAML configs
        ]);
        self
    }

    /// Configure for performance - exclude large/binary files and limit depth
    pub fn optimize_for_performance(mut self) -> Self {
        self.config.max_file_size = 512 * 1024; // 512KB limit
        self.config.max_depth = Some(8); // Reasonable depth limit
        self.config.exclude_patterns.extend(vec![
            "**/*.lock".to_string(),  // Lock files (often large)
            "**/*.log".to_string(),   // Log files
            "**/*.tmp".to_string(),   // Temporary files
            "**/*.cache".to_string(), // Cache files
            "**/*.bin".to_string(),   // Binary files
            "**/*.so".to_string(),    // Shared libraries
            "**/*.dll".to_string(),   // Windows libraries
            "**/*.exe".to_string(),   // Executables
        ]);
        self
    }

    /// Configure for comprehensive analysis - include more file types and increase limits
    pub fn comprehensive_analysis(mut self) -> Self {
        self.config.max_file_size = 5 * 1024 * 1024; // 5MB limit
        self.config.max_depth = Some(20); // Deep traversal
        self.config.include_patterns.extend(vec![
            "**/*.md".to_string(),   // Documentation
            "**/*.txt".to_string(),  // Text files
            "**/*.json".to_string(), // JSON configs
            "**/*.xml".to_string(),  // XML files
            "**/*.toml".to_string(), // TOML configs
            "**/*.yaml".to_string(), // YAML configs
            "**/*.yml".to_string(),  // YAML configs
        ]);
        self
    }

    /// Add Rust language analyzer (enabled by default)
    pub fn with_rust_analyzer(mut self) -> Self {
        match RustAnalyzer::new() {
            Ok(analyzer) => {
                if let Err(e) = self.registry.register(Box::new(analyzer)) {
                    if !e.to_string().contains("already registered") {
                        eprintln!("Failed to register Rust analyzer: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Rust analyzer unavailable: {}", e);
                eprintln!("Rust files (.rs) will be skipped during scanning");
            }
        }
        self
    }

    /// Add Python language analyzer
    pub fn with_python_analyzer(mut self) -> Self {
        match PythonAnalyzer::new() {
            Ok(analyzer) => {
                if let Err(e) = self.registry.register(Box::new(analyzer)) {
                    if !e.to_string().contains("already registered") {
                        eprintln!("Failed to register Python analyzer: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Python analyzer unavailable: {}", e);
                eprintln!("Python files (.py, .pyx, .pyi) will be skipped during scanning");
            }
        }
        self
    }

    /// Add TypeScript/TSX language analyzer
    pub fn with_typescript_analyzer(mut self) -> Self {
        match TypeScriptAnalyzer::new() {
            Ok(analyzer) => {
                if let Err(e) = self.registry.register(Box::new(analyzer)) {
                    if !e.to_string().contains("already registered") {
                        eprintln!("Failed to register TypeScript analyzer: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("TypeScript analyzer unavailable: {}", e);
                eprintln!("TypeScript files (.ts, .tsx) will be skipped during scanning");
            }
        }
        self
    }

    /// Add Go language analyzer (future)
    pub fn with_go_analyzer(self) -> Self {
        // TODO: Implement when Go analyzer is available
        self
    }

    /// Set maximum number of files to index
    pub fn max_files(mut self, limit: usize) -> Self {
        self.config.max_files = Some(limit);
        self
    }

    /// Set cache TTL for query results
    pub fn cache_ttl(mut self, seconds: u64) -> Self {
        self.config.cache_ttl_seconds = seconds;
        self
    }

    /// Add include patterns for file scanning
    pub fn include_patterns(mut self, patterns: Vec<String>) -> Self {
        self.config.include_patterns = patterns;
        self
    }

    /// Add file patterns to include (alias for include_patterns)
    pub fn file_patterns(self, patterns: Vec<String>) -> Self {
        self.include_patterns(patterns)
    }

    /// Add exclude patterns for file scanning
    pub fn exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.config.exclude_patterns = patterns;
        self
    }

    /// Set maximum file size to analyze (in bytes)
    pub fn max_file_size(mut self, size: u64) -> Self {
        self.config.max_file_size = size;
        self
    }

    /// Set maximum directory depth to scan
    pub fn max_depth(mut self, depth: u32) -> Self {
        self.config.max_depth = Some(depth);
        self
    }

    /// Enable or disable following symbolic links
    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.config.follow_symlinks = follow;
        self
    }

    /// Enable or disable respecting .gitignore files
    pub fn respect_gitignore(mut self, respect: bool) -> Self {
        self.config.respect_gitignore = respect;
        self
    }

    /// Disable maximum depth limit
    pub fn unlimited_depth(mut self) -> Self {
        self.config.max_depth = None;
        self
    }

    /// Build the LoreGrep instance with validation
    pub fn build(mut self) -> Result<LoreGrep> {
        // The Rust analyzer is enabled by default: if the caller did not register
        // any analyzer explicitly, register Rust so that `.rs` files are analyzed
        // out of the box. This keeps the builder's registry empty until build()
        // (see `with_rust_analyzer` handling of "already registered").
        if self.registry.list_supported_languages().is_empty() {
            self = self.with_rust_analyzer();
        }

        // Validate that at least one analyzer is registered
        let supported_languages = self.registry.list_supported_languages();
        if supported_languages.is_empty() {
            eprintln!("Warning: No language analyzers registered!");
            eprintln!("Consider adding: .with_rust_analyzer() or .with_python_analyzer()");
            eprintln!("Files will be discovered but not analyzed");
        }
        let repo_map = Arc::new(Mutex::new(RepoMap::new()));
        let default_config = FileScanningConfig {
            include_patterns: self.config.include_patterns.clone(),
            exclude_patterns: self.config.exclude_patterns.clone(),
            follow_symlinks: self.config.follow_symlinks,
            max_file_size: self.config.max_file_size,
            max_depth: self.config.max_depth,
            respect_gitignore: self.config.respect_gitignore,
        };
        // A shared handle into the analyzer registry. Both the scanner (for
        // language labeling) and the analysis tools (for dispatch) resolve
        // languages through this single source of truth, so adding a language
        // only requires registering its analyzer.
        let registry_handle = RegistryHandle::new(&self.registry);

        let scanner =
            RepositoryScanner::new_with_registry(&default_config, registry_handle.clone(), None)
                .map_err(|e| {
                    LoreGrepError::InternalError(format!("Scanner creation failed: {}", e))
                })?;

        // Create tools with reference to repo_map and the registry handle.
        //
        // HOOK (index coverage): `LocalAnalysisTools` should also receive
        // `coverage.clone()` — e.g. `.with_coverage(coverage.clone())` — so the
        // per-tool handlers in `internal::ai_tools` can stamp `truncated` /
        // `index_coverage` onto the payloads they build. Until that exists,
        // `LoreGrep::annotate_coverage` stamps the same keys at the public API
        // boundary, so no caller is left uninformed.
        let tools = LocalAnalysisTools::new(repo_map.clone(), registry_handle);
        let coverage = CoverageHandle::new();

        let loregrep = LoreGrep {
            repo_map,
            scanner,
            tools,
            config: self.config,
            language_registry: Arc::new(self.registry),
            coverage,
        };

        Ok(loregrep)
    }
}

impl Default for LoreGrepBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Thread safety implementations
unsafe impl Send for LoreGrep {}
unsafe impl Sync for LoreGrep {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_loregrep_builder_default() {
        let builder = LoreGrepBuilder::new();
        assert_eq!(builder.config.cache_ttl_seconds, 300);
        assert_eq!(builder.config.max_files, Some(10000));
        // Registry should be initialized (can check by listing supported languages)
        assert!(builder.registry.list_supported_languages().is_empty()); // Empty initially
    }

    #[test]
    fn test_loregrep_builder_configuration() {
        let builder = LoreGrepBuilder::new()
            .max_files(5000)
            .cache_ttl(600)
            .include_patterns(vec!["**/*.rs".to_string(), "**/*.py".to_string()])
            .exclude_patterns(vec!["**/test/**".to_string()]);

        assert_eq!(builder.config.max_files, Some(5000));
        assert_eq!(builder.config.cache_ttl_seconds, 600);
        assert_eq!(builder.config.include_patterns.len(), 2);
        assert_eq!(builder.config.exclude_patterns.len(), 1);
    }

    #[test]
    fn test_loregrep_build() {
        let result = LoreGrep::builder().build();
        assert!(result.is_ok());

        let loregrep = result.unwrap();
        assert!(!loregrep.is_scanned());
    }

    #[test]
    fn test_tool_definitions() {
        let tools = LoreGrep::get_tool_definitions();
        assert!(!tools.is_empty());

        // Should have all our expected tools
        let tool_names: Vec<&String> = tools.iter().map(|t| &t.name).collect();
        assert!(tool_names.contains(&&"search_functions".to_string()));
        assert!(tool_names.contains(&&"search_structs".to_string()));
        assert!(tool_names.contains(&&"analyze_file".to_string()));
    }

    #[tokio::test]
    async fn test_execute_tool_invalid() {
        let loregrep = LoreGrep::builder().build().unwrap();

        let result = loregrep.execute_tool("invalid_tool", json!({})).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(!tool_result.success);
        assert!(tool_result.error.is_some());
        assert!(tool_result.error.as_ref().unwrap().contains("Unknown tool"));
    }

    #[test]
    fn test_config_default() {
        let config = LoreGrepConfig::default();
        assert_eq!(config.max_files, Some(10000));
        assert_eq!(config.cache_ttl_seconds, 300);
        assert!(!config.include_patterns.is_empty());
        assert!(!config.exclude_patterns.is_empty());
    }

    #[test]
    fn test_thread_safety() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<LoreGrep>();
        assert_sync::<LoreGrep>();
    }

    #[tokio::test]
    async fn test_get_stats_empty() {
        let loregrep = LoreGrep::builder().build().unwrap();
        let stats = loregrep.get_stats().unwrap();

        assert_eq!(stats.files_scanned, 0);
        assert_eq!(stats.functions_found, 0);
        assert_eq!(stats.structs_found, 0);
        assert!(stats.languages.is_empty());
    }

    #[test]
    fn test_builder_chaining() {
        let loregrep = LoreGrep::builder()
            .with_rust_analyzer()
            .max_files(1000)
            .cache_ttl(120)
            .build();

        assert!(loregrep.is_ok());
    }

    #[test]
    fn test_builder_file_scanning_config() {
        let builder = LoreGrep::builder()
            .max_file_size(512 * 1024) // 512KB
            .max_depth(10)
            .follow_symlinks(true)
            .unlimited_depth()
            .include_patterns(vec!["**/*.rs".to_string(), "**/*.toml".to_string()])
            .exclude_patterns(vec!["**/test/**".to_string()]);

        let loregrep = builder.build();
        assert!(loregrep.is_ok());
    }

    #[tokio::test]
    async fn test_integration_scan_and_query() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory with test files
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.rs");
        fs::write(
            &test_file,
            r#"
            pub fn hello_world() -> String {
                "Hello, World!".to_string()
            }
            
            pub struct TestStruct {
                pub name: String,
                pub value: i32,
            }
        "#,
        )
        .unwrap();

        // Create LoreGrep instance and scan
        let mut loregrep = LoreGrep::builder().max_files(100).build().unwrap();

        let scan_result = loregrep.scan(temp_dir.path().to_str().unwrap()).await;
        assert!(scan_result.is_ok());

        let result = scan_result.unwrap();
        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.functions_found, 1);
        assert_eq!(result.structs_found, 1);
        assert!(result.languages.contains(&"rust".to_string()));

        // Test tool execution
        let search_result = loregrep
            .execute_tool(
                "search_functions",
                json!({
                    "pattern": "hello",
                    "limit": 10
                }),
            )
            .await;

        assert!(search_result.is_ok());
        let result = search_result.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_tool_execution_workflow() {
        let loregrep = LoreGrep::builder().build().unwrap();

        // Test all available tools
        let tools = LoreGrep::get_tool_definitions();
        assert!(!tools.is_empty());

        let expected_tools = vec![
            "search_functions",
            "search_structs",
            "analyze_file",
            "get_dependencies",
            "find_callers",
            "get_repository_tree",
        ];

        for tool_name in expected_tools {
            assert!(
                tools.iter().any(|t| t.name == tool_name),
                "Missing tool: {}",
                tool_name
            );
        }

        // Test repository tree tool on empty repository
        let tree_result = loregrep
            .execute_tool(
                "get_repository_tree",
                json!({
                    "include_file_details": false,
                    "max_depth": 1
                }),
            )
            .await;

        assert!(tree_result.is_ok());
        let result = tree_result.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_config_comprehensive() {
        let config = LoreGrepConfig {
            max_files: Some(5000),
            cache_ttl_seconds: 600,
            include_patterns: vec!["**/*.rs".to_string(), "**/*.py".to_string()],
            exclude_patterns: vec!["**/test/**".to_string()],
            max_file_size: 2 * 1024 * 1024, // 2MB
            max_depth: Some(15),
            follow_symlinks: true,
            respect_gitignore: true,
        };

        assert_eq!(config.max_files, Some(5000));
        assert_eq!(config.cache_ttl_seconds, 600);
        assert_eq!(config.include_patterns.len(), 2);
        assert_eq!(config.exclude_patterns.len(), 1);
        assert_eq!(config.max_file_size, 2 * 1024 * 1024);
        assert_eq!(config.max_depth, Some(15));
        assert!(config.follow_symlinks);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let loregrep = LoreGrep::builder().build().unwrap();

        // Test invalid tool execution
        let result = loregrep.execute_tool("invalid_tool", json!({})).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.success);
        assert!(tool_result.error.is_some());

        // Test tool with missing required parameters
        let result = loregrep.execute_tool("search_functions", json!({})).await;
        // This might return an error or a tool result with error, both are acceptable
        match result {
            Ok(tool_result) => {
                assert!(!tool_result.success);
            }
            Err(_) => {
                // Also acceptable - parameter validation error
            }
        }
    }

    #[tokio::test]
    async fn test_scan_nonexistent_path() {
        let mut loregrep = LoreGrep::builder().build().unwrap();

        let result = loregrep
            .scan("/this/path/definitely/does/not/exist/anywhere")
            .await;
        // The scanner might handle nonexistent paths gracefully and return empty results
        // rather than failing, which is actually good behavior for a library
        match result {
            Ok(scan_result) => {
                // Should have 0 files scanned for nonexistent path
                assert_eq!(scan_result.files_scanned, 0);
            }
            Err(LoreGrepError::InternalError(_)) => {
                // Also acceptable - scanning error
            }
            _ => panic!("Unexpected error type"),
        }
    }

    #[tokio::test]
    async fn test_cache_stale_after_file_edit() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let src = temp_dir.path().join("lib.rs");
        fs::write(&src, "pub fn a() -> i32 { 1 }\n").unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        lg.scan(temp_dir.path().to_str().unwrap()).await.unwrap();
        let cache_path = LoreGrep::default_cache_path(temp_dir.path());
        lg.save_index(&cache_path).unwrap();

        // A freshly written cache is fresh.
        assert!(lg.is_cache_fresh(&cache_path, temp_dir.path()));

        // Editing a file's CONTENT must invalidate the cache. A directory's
        // mtime does not change when a file's contents are edited, so a
        // dir-mtime gate would wrongly report "fresh"; content hashing sees the
        // edit regardless of any timestamp.
        fs::write(&src, "pub fn b() -> i32 { 2 }\n").unwrap();
        assert!(
            !lg.is_cache_fresh(&cache_path, temp_dir.path()),
            "cache must be stale after a source file content edit"
        );
    }

    #[tokio::test]
    async fn test_save_and_load_index_round_trip() {
        use std::fs;
        use tempfile::TempDir;

        // Scan a temp repo and record a tool result.
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("lib.rs"),
            "pub fn persisted_fn() -> i32 { 42 }\npub struct PersistedStruct { pub x: i32 }\n",
        )
        .unwrap();

        let mut original = LoreGrep::builder().build().unwrap();
        original
            .scan(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert!(original.is_scanned());

        let before = original
            .execute_tool("search_functions", json!({"pattern": "persisted_fn"}))
            .await
            .unwrap();
        assert!(before.success);

        // Persist the index to disk.
        let cache_path = LoreGrep::default_cache_path(temp_dir.path());
        original.save_index(&cache_path).unwrap();
        assert!(cache_path.exists(), "cache file should be written");

        // A brand-new instance is empty until it loads the cache.
        let fresh = LoreGrep::builder().build().unwrap();
        assert!(!fresh.is_scanned());
        fresh.load_index(&cache_path).unwrap();
        assert!(
            fresh.is_scanned(),
            "is_scanned should reflect the loaded index"
        );

        // The same query returns the same functions from the loaded index.
        let after = fresh
            .execute_tool("search_functions", json!({"pattern": "persisted_fn"}))
            .await
            .unwrap();
        assert!(after.success);
        assert_eq!(
            before.data.get("count"),
            after.data.get("count"),
            "loaded index should return the same result count as the original"
        );
        assert_eq!(
            before.data.get("results"),
            after.data.get("results"),
            "loaded index should return identical results"
        );

        // Freshness gate: the just-written cache is fresh for its root.
        assert!(fresh.is_cache_fresh(&cache_path, temp_dir.path()));
        // A missing cache is never fresh.
        let missing = temp_dir.path().join("nope").join("index.cache");
        assert!(!fresh.is_cache_fresh(&missing, temp_dir.path()));
    }

    #[tokio::test]
    async fn test_builder_scan_indexes_every_js_and_ts_dialect() {
        use std::fs;
        use tempfile::TempDir;

        // The builder path uses LoreGrepConfig's default include patterns rather
        // than auto_discover's detected ones; every dialect the TypeScript
        // analyzer claims must be scannable through it too.
        let temp_dir = TempDir::new().unwrap();
        for (name, body) in [
            ("a.js", "export function jsFn() {}"),
            ("b.mjs", "export function mjsFn() {}"),
            ("c.cjs", "function cjsFn() {}\nmodule.exports = { cjsFn };"),
            ("d.jsx", "export function jsxFn() { return <p />; }"),
            ("e.mts", "export function mtsFn(): void {}"),
            ("f.ts", "export function tsFn(): void {}"),
        ] {
            fs::write(temp_dir.path().join(name), body).unwrap();
        }

        let mut loregrep = LoreGrep::builder()
            .with_typescript_analyzer()
            .build()
            .unwrap();
        let result = loregrep
            .scan(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(result.files_scanned, 6, "every dialect must be indexed");

        for pattern in ["jsFn", "mjsFn", "cjsFn", "jsxFn", "mtsFn", "tsFn"] {
            let found = loregrep
                .execute_tool("search_functions", serde_json::json!({"pattern": pattern}))
                .await
                .unwrap();
            assert!(
                found.data.to_string().contains(pattern),
                "{pattern} should be indexed and searchable: {}",
                found.data
            );
        }
    }

    #[tokio::test]
    async fn test_auto_discover_js_only_indexes_js_files() {
        use std::fs;
        use tempfile::TempDir;

        // A JavaScript-only project: package.json + a .js file, no tsconfig.json.
        // JavaScript is handled by the TypeScript analyzer (TSX grammar), so the
        // file must actually be indexed rather than scanned and dropped.
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        fs::write(
            temp_dir.path().join("index.js"),
            "export function greet(name) { return `hi ${name}`; }\n",
        )
        .unwrap();

        let mut loregrep = LoreGrep::auto_discover(temp_dir.path()).unwrap();
        let langs = loregrep.language_registry.list_supported_languages();
        assert!(
            langs.contains(&"typescript".to_string()),
            "the TypeScript analyzer handles JavaScript, got {:?}",
            langs
        );

        let result = loregrep
            .scan(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(result.files_scanned, 1, "the .js file must be indexed");

        let found = loregrep
            .execute_tool("search_functions", serde_json::json!({"pattern": "greet"}))
            .await
            .unwrap();
        assert!(
            found.data.to_string().contains("greet"),
            "the JS function should be searchable: {}",
            found.data
        );
    }

    #[tokio::test]
    async fn test_auto_discover_typescript_without_tsconfig_indexes_ts_files() {
        use std::fs;
        use tempfile::TempDir;

        // A TypeScript project WITHOUT a tsconfig.json: package.json plus a .ts
        // source file. Previously this was labeled only "javascript", which
        // overwrote the include patterns with JS-only globs and dropped the .ts
        // file even though the TS analyzer was registered. auto_discover must now
        // detect "typescript" (from the .ts file) and index it.
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        fs::write(
            temp_dir.path().join("index.ts"),
            "export function greet(name: string): string { return `hi ${name}`; }\n",
        )
        .unwrap();

        let mut loregrep = LoreGrep::auto_discover(temp_dir.path()).unwrap();

        // The TypeScript analyzer must be registered.
        let langs = loregrep.language_registry.list_supported_languages();
        assert!(
            langs.contains(&"typescript".to_string()),
            "typescript analyzer should be registered, got {:?}",
            langs
        );

        // Scanning must index the .ts file.
        let result = loregrep
            .scan(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(
            result.files_scanned, 1,
            "the .ts file should be indexed, files_scanned={}",
            result.files_scanned
        );
        assert!(
            result.languages.contains(&"typescript".to_string()),
            "scan should report typescript, got {:?}",
            result.languages
        );
    }

    // ---------------------------------------------------------------------
    // Index/scan-state regressions
    // ---------------------------------------------------------------------

    /// Count results in a tool payload.
    async fn count_matches(lg: &LoreGrep, pattern: &str) -> u64 {
        let result = lg
            .execute_tool("search_functions", json!({"pattern": pattern}))
            .await
            .unwrap();
        assert!(result.success, "search failed: {:?}", result.error);
        result
            .data
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    }

    /// Give a file an mtime far in the past, the way `cp -p`, `rsync -a`,
    /// `tar -x` and git worktree materialization preserve original timestamps.
    #[cfg(unix)]
    fn preserve_old_mtime(path: &Path) {
        let status = std::process::Command::new("touch")
            .args(["-t", "202001010000"])
            .arg(path)
            .status()
            .expect("touch should be available");
        assert!(status.success(), "failed to backdate {}", path.display());
        let mtime = std::fs::metadata(path).unwrap().modified().unwrap();
        assert!(
            mtime < std::time::SystemTime::now() - std::time::Duration::from_secs(86_400),
            "mtime was not actually backdated"
        );
    }

    #[tokio::test]
    async fn test_scanning_a_second_root_replaces_the_first() {
        use std::fs;
        use tempfile::TempDir;

        // K3 regression: `scan` used to only ever ADD files, so scanning repo A
        // and then repo B returned symbols from BOTH — with a single scan_root
        // recorded, half of them unreadable. One index, one root.
        let repo_a = TempDir::new().unwrap();
        fs::write(
            repo_a.path().join("a.rs"),
            "pub fn alpha_only() -> i32 { 1 }\n",
        )
        .unwrap();
        let repo_b = TempDir::new().unwrap();
        fs::write(
            repo_b.path().join("b.rs"),
            "pub fn beta_only() -> i32 { 2 }\n",
        )
        .unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        lg.scan(repo_a.path().to_str().unwrap()).await.unwrap();
        assert_eq!(count_matches(&lg, "alpha_only").await, 1);

        let second = lg.scan(repo_b.path().to_str().unwrap()).await.unwrap();
        assert_eq!(second.files_scanned, 1);

        assert_eq!(
            count_matches(&lg, "alpha_only").await,
            0,
            "symbols from the previously scanned root must not survive"
        );
        assert_eq!(count_matches(&lg, "beta_only").await, 1);
        assert_eq!(lg.get_stats().unwrap().files_scanned, 1);
    }

    #[tokio::test]
    async fn test_rescanning_the_same_root_drops_deleted_files() {
        use std::fs;
        use tempfile::TempDir;

        // K3 regression: deleting a file and rescanning THE SAME ROOT used to
        // keep returning the deleted symbol, with a file_path that no longer
        // exists on disk.
        let repo = TempDir::new().unwrap();
        fs::write(
            repo.path().join("keep.rs"),
            "pub fn kept_fn() -> i32 { 1 }\n",
        )
        .unwrap();
        let doomed = repo.path().join("doomed.rs");
        fs::write(&doomed, "pub fn doomed_fn() -> i32 { 2 }\n").unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        let root = repo.path().to_str().unwrap().to_string();
        lg.scan(&root).await.unwrap();
        assert_eq!(count_matches(&lg, "doomed_fn").await, 1);

        fs::remove_file(&doomed).unwrap();
        let rescan = lg.scan(&root).await.unwrap();

        assert_eq!(rescan.files_scanned, 1, "rescan should see only one file");
        assert_eq!(
            count_matches(&lg, "doomed_fn").await,
            0,
            "a deleted file's symbols must not survive a rescan of its root"
        );
        assert_eq!(count_matches(&lg, "kept_fn").await, 1);
        assert!(
            lg.first_missing_indexed_path().is_none(),
            "no indexed path may point at a file that does not exist"
        );

        // Idempotence: scanning again changes nothing.
        lg.scan(&root).await.unwrap();
        assert_eq!(lg.get_stats().unwrap().files_scanned, 1);
        assert_eq!(count_matches(&lg, "kept_fn").await, 1);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cache_detects_file_added_with_preserved_old_mtime() {
        use std::fs;
        use tempfile::TempDir;

        // K5 regression: freshness was `cache_mtime > max(source mtime)`. A file
        // added with a PRESERVED OLDER mtime is never newer than the cache, so
        // it was permanently invisible: two files on disk, one file indexed,
        // forever. The path-set comparison sees it regardless of timestamps.
        let repo = TempDir::new().unwrap();
        fs::write(
            repo.path().join("first.rs"),
            "pub fn first_fn() -> i32 { 1 }\n",
        )
        .unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        lg.scan(repo.path().to_str().unwrap()).await.unwrap();
        let cache_path = LoreGrep::default_cache_path(repo.path());
        lg.save_index(&cache_path).unwrap();
        assert!(lg.is_cache_fresh(&cache_path, repo.path()));

        // Restore a second file with its original (2020) timestamp.
        let restored = repo.path().join("restored.rs");
        fs::write(&restored, "pub fn restored_fn() -> i32 { 2 }\n").unwrap();
        preserve_old_mtime(&restored);
        assert!(
            fs::metadata(&restored).unwrap().modified().unwrap()
                < fs::metadata(&cache_path).unwrap().modified().unwrap(),
            "the added file must be OLDER than the cache for this to be the bug"
        );

        assert!(
            !lg.is_cache_fresh(&cache_path, repo.path()),
            "a file added with a preserved old mtime MUST invalidate the cache"
        );
        assert!(!lg.load_index_if_fresh(&cache_path, repo.path()).unwrap());

        // And a rescan actually indexes it.
        lg.scan(repo.path().to_str().unwrap()).await.unwrap();
        assert_eq!(count_matches(&lg, "restored_fn").await, 1);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cache_detects_content_restored_with_preserved_old_mtime() {
        use std::fs;
        use tempfile::TempDir;

        // K5, second form: an EXISTING file whose contents are replaced while
        // its mtime is pushed back into the past. No timestamp comparison can
        // see this; the recorded content hash does.
        let repo = TempDir::new().unwrap();
        let src = repo.path().join("lib.rs");
        fs::write(&src, "pub fn original_fn() -> i32 { 1 }\n").unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        lg.scan(repo.path().to_str().unwrap()).await.unwrap();
        let cache_path = LoreGrep::default_cache_path(repo.path());
        lg.save_index(&cache_path).unwrap();
        assert!(lg.is_cache_fresh(&cache_path, repo.path()));

        fs::write(&src, "pub fn swapped_fn() -> i32 { 2 }\n").unwrap();
        preserve_old_mtime(&src);

        assert!(
            !lg.is_cache_fresh(&cache_path, repo.path()),
            "content restored with an old mtime MUST invalidate the cache"
        );
    }

    #[tokio::test]
    async fn test_cache_is_rejected_when_configuration_changes() {
        use std::fs;
        use tempfile::TempDir;

        // K4 regression: the cache recorded nothing about the configuration that
        // built it, so a run configured for `*.rs` only wrote a cache that a
        // later default-configured run (which DOES index Python) happily loaded
        // — leaving the Python file silently absent.
        let repo = TempDir::new().unwrap();
        fs::write(
            repo.path().join("lib.rs"),
            "pub fn rust_fn() -> i32 { 1 }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("mod.py"),
            "def python_fn():\n    return 2\n",
        )
        .unwrap();

        let cache_path = LoreGrep::default_cache_path(repo.path());

        let mut rust_only = LoreGrep::builder()
            .with_all_analyzers()
            .include_patterns(vec!["**/*.rs".to_string()])
            .build()
            .unwrap();
        rust_only.scan(repo.path().to_str().unwrap()).await.unwrap();
        rust_only.save_index(&cache_path).unwrap();
        assert_eq!(count_matches(&rust_only, "python_fn").await, 0);
        assert!(
            rust_only.is_cache_fresh(&cache_path, repo.path()),
            "the cache is valid for the configuration that wrote it"
        );

        // A run whose configuration also indexes Python must not accept it.
        let mut polyglot = LoreGrep::builder().with_all_analyzers().build().unwrap();
        assert!(
            !polyglot.is_cache_fresh(&cache_path, repo.path()),
            "a cache built with different include patterns must be rejected"
        );
        assert!(polyglot.load_index(&cache_path).is_err());
        assert!(
            !polyglot
                .load_index_if_fresh(&cache_path, repo.path())
                .unwrap()
        );

        polyglot.scan(repo.path().to_str().unwrap()).await.unwrap();
        assert_eq!(
            count_matches(&polyglot, "python_fn").await,
            1,
            "the Python file must be indexed once the cache is correctly rejected"
        );
    }

    #[tokio::test]
    async fn test_cache_is_rejected_when_analyzer_set_changes() {
        use std::fs;
        use tempfile::TempDir;

        // The other half of K4: same patterns, different analyzers. A cache
        // written with only the Rust analyzer registered describes a different
        // index than one written with Python registered too.
        let repo = TempDir::new().unwrap();
        fs::write(
            repo.path().join("lib.rs"),
            "pub fn rust_fn() -> i32 { 1 }\n",
        )
        .unwrap();

        let cache_path = LoreGrep::default_cache_path(repo.path());
        let mut rust_only = LoreGrep::builder().with_rust_analyzer().build().unwrap();
        rust_only.scan(repo.path().to_str().unwrap()).await.unwrap();
        rust_only.save_index(&cache_path).unwrap();

        let polyglot = LoreGrep::builder()
            .with_rust_analyzer()
            .with_python_analyzer()
            .build()
            .unwrap();
        assert!(!polyglot.is_cache_fresh(&cache_path, repo.path()));
    }

    #[tokio::test]
    async fn test_truncated_index_is_loud_reported_and_uncacheable() {
        use std::fs;
        use tempfile::TempDir;

        // K6 regression: hitting `max_files` used to `break` silently. The
        // resulting partial index was persisted as if complete, and a search for
        // a symbol in the dropped tail answered `success: true` with prefix
        // near-misses — teaching a false fact with full confidence.
        let repo = TempDir::new().unwrap();
        for i in 0..4 {
            fs::write(
                repo.path().join(format!("f{i}.rs")),
                format!("pub fn target_{i}() -> i32 {{ {i} }}\n"),
            )
            .unwrap();
        }

        let mut lg = LoreGrep::builder().max_files(1).build().unwrap();
        lg.scan(repo.path().to_str().unwrap()).await.unwrap();

        let coverage = lg.index_coverage();
        assert!(coverage.truncated, "the index is partial and must say so");
        assert_eq!(coverage.files_indexed, 1);
        assert_eq!(coverage.files_discovered, 4);
        assert_eq!(
            coverage.note().unwrap(),
            "index covers 1 of 4 files (truncated by the max_files limit); \
             absence of a symbol does NOT prove it is missing from the repository"
        );

        // Every tool response derived from the truncated index is labelled.
        let result = lg
            .execute_tool("search_functions", json!({"pattern": "target_"}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.data.get("truncated"), Some(&json!(true)));
        assert_eq!(result.data.get("indexed_file_count"), Some(&json!(1)));
        assert_eq!(result.data.get("discovered_file_count"), Some(&json!(4)));
        assert!(
            result
                .data
                .get("index_coverage")
                .and_then(|v| v.as_str())
                .unwrap()
                .contains("index covers 1 of 4 files"),
            "the coverage note must be human-readable: {}",
            result.data
        );

        // A truncated index must not be persisted as authoritative.
        let cache_path = LoreGrep::default_cache_path(repo.path());
        let err = lg.save_index(&cache_path).unwrap_err();
        assert!(
            err.to_string().contains("truncated"),
            "unexpected error: {err}"
        );
        assert!(
            !cache_path.exists(),
            "no cache file may be written for a truncated index"
        );
    }

    #[tokio::test]
    async fn test_complete_index_reports_no_truncation() {
        use std::fs;
        use tempfile::TempDir;

        // The control for the test above: a complete index adds nothing to its
        // responses, so the truncation markers mean what they say.
        let repo = TempDir::new().unwrap();
        fs::write(repo.path().join("a.rs"), "pub fn only_fn() -> i32 { 1 }\n").unwrap();

        let mut lg = LoreGrep::builder().max_files(10).build().unwrap();
        lg.scan(repo.path().to_str().unwrap()).await.unwrap();

        assert!(!lg.index_coverage().truncated);
        let result = lg
            .execute_tool("search_functions", json!({"pattern": "only_fn"}))
            .await
            .unwrap();
        assert!(result.data.get("truncated").is_none());
        assert!(result.data.get("index_coverage").is_none());

        let cache_path = LoreGrep::default_cache_path(repo.path());
        lg.save_index(&cache_path).unwrap();
        assert!(cache_path.exists());
    }

    #[test]
    fn test_default_max_files_has_one_source_of_truth() {
        // The library default and the CLI default were separate literals; they
        // are now the same constant.
        assert_eq!(LoreGrepConfig::DEFAULT_MAX_FILES, 10_000);
        assert_eq!(
            LoreGrepConfig::default().max_files,
            Some(LoreGrepConfig::DEFAULT_MAX_FILES)
        );
    }

    #[test]
    fn test_builder_default_exclusions() {
        let _builder = LoreGrep::builder();

        // Verify default exclusions include test-repos
        let config = LoreGrepConfig::default();
        assert!(
            config
                .exclude_patterns
                .contains(&"**/test-repos/**".to_string())
        );
        assert!(
            config
                .exclude_patterns
                .contains(&"**/target/**".to_string())
        );
        assert!(config.exclude_patterns.contains(&"**/.git/**".to_string()));
        assert!(
            config
                .exclude_patterns
                .contains(&"**/node_modules/**".to_string())
        );
    }

    // ------------------------------------------------------------------ //
    // Canonical-path refactor regressions (F3 / F4 / K1 / K2 / K9)
    // ------------------------------------------------------------------ //

    /// Serializes the tests that change the process working directory. `cwd` is
    /// process-global, so two of them running concurrently would each observe
    /// the other's directory.
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    async fn emitted_paths(lg: &LoreGrep) -> Vec<String> {
        let result = lg
            .execute_tool("search_functions", json!({"pattern": ".*", "limit": 100}))
            .await
            .unwrap();
        let mut paths: Vec<String> = result.data["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["file_path"].as_str().unwrap().to_string())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// F3: one file used to come back as `./src/x.rs`, `/abs/.../src/x.rs` or
    /// `../../src/x.rs` depending purely on how `--path` was spelled and where
    /// the caller stood — and the `../..` form stopped resolving the moment the
    /// agent changed directory. All spellings, from any cwd, now emit one string.
    #[tokio::test]
    async fn every_path_spelling_and_cwd_emits_one_path_string() {
        use std::fs;
        use tempfile::TempDir;

        let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original_cwd = std::env::current_dir().unwrap();

        let repo = TempDir::new().unwrap();
        let root = fs::canonicalize(repo.path()).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn spelled_once() -> i32 { 1 }\n",
        )
        .unwrap();

        let expected = vec!["src/lib.rs".to_string()];

        // (cwd, spelling) pairs that all name the same directory.
        let cases: Vec<(std::path::PathBuf, String)> = vec![
            // Absolute, from outside the repo.
            (original_cwd.clone(), root.to_string_lossy().to_string()),
            // "." from inside the repo.
            (root.clone(), ".".to_string()),
            // "../.." climbing back out of a nested subdirectory.
            (root.join("src"), "../src/..".to_string()),
        ];

        for (cwd, spelling) in cases {
            std::env::set_current_dir(&cwd).unwrap();
            let mut lg = LoreGrep::builder().build().unwrap();
            lg.scan(&spelling).await.unwrap();
            let paths = emitted_paths(&lg).await;
            std::env::set_current_dir(&original_cwd).unwrap();
            assert_eq!(
                paths,
                expected,
                "spelling {spelling:?} from cwd {} emitted a different path",
                cwd.display()
            );
        }
    }

    /// K1 (verified pre-fix): scanning repo R1 and then querying it from inside
    /// R2 returned `repo_one_only -> ./src/lib.rs`, and that path, read from R2,
    /// names a DIFFERENT repository's file. An emitted path must identify a file
    /// in the repository that was scanned, unambiguously.
    #[tokio::test]
    async fn a_path_emitted_for_one_repo_never_resolves_into_another() {
        use std::fs;
        use tempfile::TempDir;

        let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original_cwd = std::env::current_dir().unwrap();

        let repo_one = TempDir::new().unwrap();
        let repo_two = TempDir::new().unwrap();
        let one = fs::canonicalize(repo_one.path()).unwrap();
        let two = fs::canonicalize(repo_two.path()).unwrap();
        for r in [&one, &two] {
            fs::create_dir(r.join("src")).unwrap();
        }
        fs::write(
            one.join("src/lib.rs"),
            "pub fn repo_one_only() -> i32 { 1 }\n",
        )
        .unwrap();
        // A DECOY at the same relative path in the other repo — this is the file
        // a cwd-relative path silently resolved to.
        fs::write(
            two.join("src/lib.rs"),
            "pub fn repo_two_decoy() -> i32 { 2 }\n",
        )
        .unwrap();

        // Scan R1 from R1, then move into R2 and query.
        std::env::set_current_dir(&one).unwrap();
        let mut lg = LoreGrep::builder().build().unwrap();
        lg.scan(".").await.unwrap();
        std::env::set_current_dir(&two).unwrap();

        let result = lg
            .execute_tool("search_functions", json!({"pattern": "repo_one_only"}))
            .await
            .unwrap();
        let emitted = result.data["results"][0]["file_path"]
            .as_str()
            .unwrap()
            .to_string();
        let analysis_root = result.data["analysis_root"].as_str().unwrap().to_string();
        std::env::set_current_dir(&original_cwd).unwrap();

        // The response is self-describing: a root-relative path PLUS the root it
        // is relative to. Together they name exactly one file on disk.
        assert_eq!(emitted, "src/lib.rs");
        assert_eq!(analysis_root, one.to_string_lossy());
        let absolutized = std::path::Path::new(&analysis_root).join(&emitted);
        assert_eq!(absolutized, one.join("src/lib.rs"));
        let content = std::fs::read_to_string(&absolutized).unwrap();
        assert!(
            content.contains("repo_one_only"),
            "the emitted path must name R1's file, got: {content}"
        );
    }

    /// K2: `RepoMap::file_index` deduped on the RAW key while
    /// `graph::FileSet::by_path` deduped on the NORMALIZED one, so two spellings
    /// of one root produced two index entries that collapsed into one graph slot
    /// — and `find_importers` answered 0 for a file that IS imported.
    #[tokio::test]
    async fn two_root_spellings_leave_one_index_entry_and_a_working_importer() {
        use std::fs;
        use tempfile::TempDir;

        let repo = TempDir::new().unwrap();
        let root = fs::canonicalize(repo.path()).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/config.rs"), "pub fn cfg() -> i32 { 1 }\n").unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod config;\nuse crate::config;\nfn main() { let _ = config::cfg(); }\n",
        )
        .unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        // Scan twice, under two different spellings of the same root.
        lg.scan(root.to_str().unwrap()).await.unwrap();
        let second = lg
            .scan(&format!("{}/src/..", root.display()))
            .await
            .unwrap();
        assert_eq!(second.files_scanned, 2, "one root, two files, not four");

        let importers = lg
            .execute_tool("find_importers", json!({"file": "src/config.rs"}))
            .await
            .unwrap();
        assert!(importers.success, "{:?}", importers.error);
        assert_eq!(
            importers.data["count"].as_u64().unwrap(),
            1,
            "an imported file reported 0 importers: {:?}",
            importers.data
        );
        assert_eq!(importers.data["importers"][0], "src/main.rs");
        // The echoed path is in the same vocabulary as the importers.
        assert_eq!(importers.data["file"], "src/config.rs");
    }

    /// K9: a symlinked root and the real root are one repository. They shared a
    /// cache file but produced different keys; both now canonicalize to one root.
    #[tokio::test]
    #[cfg(unix)]
    async fn a_symlinked_root_and_its_target_produce_identical_keys() {
        use std::fs;
        use tempfile::TempDir;

        let outer = TempDir::new().unwrap();
        let real = fs::canonicalize(outer.path()).unwrap().join("real");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("a.rs"), "pub fn linked_fn() -> i32 { 1 }\n").unwrap();
        let link = fs::canonicalize(outer.path()).unwrap().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let mut via_real = LoreGrep::builder().build().unwrap();
        via_real.scan(real.to_str().unwrap()).await.unwrap();
        let mut via_link = LoreGrep::builder().build().unwrap();
        via_link.scan(link.to_str().unwrap()).await.unwrap();

        assert_eq!(
            emitted_paths(&via_real).await,
            emitted_paths(&via_link).await
        );

        // And the analysis root itself is the same, so the cache they share is
        // accepted for both rather than rejected for one.
        let root_of = |lg: &LoreGrep| lg.repo_map.lock().unwrap().scan_root().unwrap().to_string();
        assert_eq!(root_of(&via_real), root_of(&via_link));
    }

    /// F4: the cache stored raw path strings and was keyed only on the directory,
    /// so whichever invocation populated it imposed its spelling on every later
    /// run. The header now carries the canonical root and a mismatch is refused.
    #[tokio::test]
    async fn a_cache_built_for_another_root_is_refused() {
        use std::fs;
        use tempfile::TempDir;

        let repo_one = TempDir::new().unwrap();
        let repo_two = TempDir::new().unwrap();
        fs::write(
            repo_one.path().join("a.rs"),
            "pub fn only_in_one() -> i32 { 1 }\n",
        )
        .unwrap();
        fs::write(
            repo_two.path().join("a.rs"),
            "pub fn only_in_two() -> i32 { 2 }\n",
        )
        .unwrap();

        let mut lg = LoreGrep::builder().build().unwrap();
        lg.scan(repo_one.path().to_str().unwrap()).await.unwrap();
        let cache = LoreGrep::default_cache_path(repo_one.path());
        lg.save_index(&cache).unwrap();
        assert!(lg.is_cache_fresh(&cache, repo_one.path()));

        // The same cache FILE, validated for a different root: refused.
        assert!(
            !lg.is_cache_fresh(&cache, repo_two.path()),
            "a cache built for one root must not be served to another"
        );
    }
}
