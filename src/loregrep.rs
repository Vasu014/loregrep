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
use crate::storage::persistence::PersistenceManager;

/// The main struct for interacting with LoreGrep
#[derive(Clone)]
pub struct LoreGrep {
    repo_map: Arc<Mutex<RepoMap>>,
    scanner: RepositoryScanner,
    tools: LocalAnalysisTools,
    config: LoreGrepConfig,
    language_registry: Arc<DefaultLanguageRegistry>,
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

impl Default for LoreGrepConfig {
    fn default() -> Self {
        Self {
            max_files: Some(10000),
            cache_ttl_seconds: 300, // 5 minutes
            include_patterns: vec![
                "**/*.rs".to_string(),
                "**/*.py".to_string(),
                "**/*.pyx".to_string(),
                "**/*.pyi".to_string(),
                "**/*.ts".to_string(),
                "**/*.tsx".to_string(),
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
        for language in &detected_languages {
            builder = match language.as_str() {
                "rust" => builder.with_rust_analyzer(),
                "python" => builder.with_python_analyzer(),
                "typescript" => builder.with_typescript_analyzer(),
                // Future language support:
                // "javascript" => builder.with_javascript_analyzer(),
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
            // Note: Analyzers not yet implemented, but detection is ready
            if path.join("tsconfig.json").exists() {
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

    /// Scan a repository and build the in-memory index
    /// This should be called by the host application, not exposed as a tool
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
        let discovered_files = scan_result.files;

        if discovered_files.is_empty() {
            eprintln!("No files found in the specified path");
            eprintln!("Check that the path exists and contains supported file types");
            return Ok(ScanResult::new(
                0,
                0,
                0,
                start_time.elapsed().as_millis() as u64,
                Vec::new(),
            ));
        }

        eprintln!("Found {} files to analyze", discovered_files.len());

        let mut files_scanned = 0;
        let mut functions_found = 0;
        let mut structs_found = 0;
        let mut languages = std::collections::HashSet::new();
        let mut analysis_results = Vec::new();

        // Analyze each file (without holding the mutex)
        for file_info in discovered_files {
            if let Some(max_files) = self.config.max_files {
                if files_scanned >= max_files {
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
            let path_str = file_info.path.to_string_lossy().to_string();
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

        // Now add all analysis results to repo map (holding mutex only briefly)
        {
            let mut repo_map = self.repo_map.lock().map_err(|e| {
                LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e))
            })?;

            for tree_node in analysis_results {
                if let Err(e) = repo_map.add_file(tree_node) {
                    eprintln!("Warning: Failed to store analysis: {}", e);
                }
            }
        } // Mutex guard is dropped here

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

        // Convert from ai_tools::ToolResult to core::types::ToolResult
        if ai_result.success {
            Ok(ToolResult::success(ai_result.data))
        } else {
            Ok(ToolResult::error(
                ai_result
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ))
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

    /// Persist the current in-memory index to `cache_path` (gzip-compressed
    /// JSON via [`PersistenceManager`]). Creates the parent directory if needed.
    ///
    /// The written cache embeds the crate version and a content hash so a stale
    /// or version-mismatched cache is rejected on load rather than silently
    /// serving wrong results.
    pub fn save_index(&self, cache_path: &Path) -> Result<()> {
        let (dir, name) = Self::cache_dir_and_name(cache_path)?;
        let manager = PersistenceManager::new(&dir)?;

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
    /// [`PersistenceManager::load_from_disk`] enforces the crate-version gate:
    /// a cache written by a different version is rejected with an error so the
    /// caller can fall back to rescanning.
    pub fn load_index(&self, cache_path: &Path) -> Result<()> {
        let (dir, name) = Self::cache_dir_and_name(cache_path)?;
        let manager = PersistenceManager::new(&dir)?;
        let loaded = manager.load_from_disk(&name)?;

        let mut repo_map = self
            .repo_map
            .lock()
            .map_err(|e| LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e)))?;
        *repo_map = loaded;
        Ok(())
    }

    /// Freshness gate for a persisted cache. Returns `true` only when the cache
    /// file exists and is at least as new as the repository `root`'s directory
    /// modification time (the same heuristic used by the persistence layer).
    ///
    /// This is intentionally conservative: when either modification time cannot
    /// be read, it returns `false` so the caller re-scans rather than trusting a
    /// cache it cannot validate. The crate-version / content-hash gate is
    /// additionally enforced at [`LoreGrep::load_index`] time.
    pub fn is_cache_fresh(&self, cache_path: &Path, root: &Path) -> bool {
        let cache_modified = match std::fs::metadata(cache_path).and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => return false, // missing/unreadable cache -> not fresh
        };

        // The cache is stale if ANY source file under `root` is newer than it.
        // A directory's mtime does NOT change when an existing file's *content*
        // is edited, so checking only the root dir mtime would serve stale
        // results after an edit. Walk the files instead (O(files) stat, no reads),
        // skipping heavy/irrelevant dirs and our own cache directory.
        let mut newest: Option<std::time::SystemTime> = None;
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".loregrep" | ".git" | "target" | "node_modules"
                )
            })
            .flatten()
        {
            if entry.file_type().is_file() {
                if let Ok(md) = entry.metadata() {
                    if let Ok(m) = md.modified() {
                        if newest.is_none_or(|n| m > n) {
                            newest = Some(m);
                        }
                    }
                }
            }
        }

        match newest {
            Some(newest_file) => cache_modified >= newest_file,
            None => true, // no source files -> trivially fresh
        }
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
                "typescript" => {
                    patterns.extend(vec!["**/*.ts".to_string(), "**/*.tsx".to_string()])
                }
                "javascript" => patterns.extend(vec![
                    "**/*.js".to_string(),
                    "**/*.jsx".to_string(),
                    "**/*.mjs".to_string(),
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
        let tools = LocalAnalysisTools::new(repo_map.clone(), registry_handle);

        let loregrep = LoreGrep {
            repo_map,
            scanner,
            tools,
            config: self.config,
            language_registry: Arc::new(self.registry),
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

        // Editing a file's CONTENT must invalidate the cache. The file's mtime
        // advances past the cache even though the directory's mtime does not, so a
        // dir-mtime-only gate would wrongly report "fresh". Sleep past coarse
        // filesystem mtime granularity so the edit is strictly newer.
        std::thread::sleep(std::time::Duration::from_millis(1100));
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

    #[test]
    fn test_auto_discover_js_only_registers_available_analyzers() {
        use std::fs;
        use tempfile::TempDir;

        // A JavaScript-only project: package.json + a .js file, but no
        // tsconfig.json, so only "javascript" is detected. JavaScript has no
        // analyzer yet, so auto_discover must fall through to registering the
        // full available set (rust + python + typescript) rather than silently
        // ending up with only the default Rust analyzer.
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        fs::write(temp_dir.path().join("index.js"), "function f() {}").unwrap();

        let loregrep = LoreGrep::auto_discover(temp_dir.path()).unwrap();
        let langs = loregrep.language_registry.list_supported_languages();

        assert!(
            langs.contains(&"rust".to_string()),
            "rust analyzer should be registered, got {:?}",
            langs
        );
        assert!(
            langs.contains(&"python".to_string()),
            "python analyzer should be registered, got {:?}",
            langs
        );
        assert!(
            langs.contains(&"typescript".to_string()),
            "typescript analyzer should be registered, got {:?}",
            langs
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
}
