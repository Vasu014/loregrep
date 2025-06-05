use std::sync::{Arc, Mutex};
use serde_json::Value;

use crate::core::{LoreGrepError, Result, ToolSchema, ToolResult, ScanResult};
use crate::storage::memory::RepoMap;
use crate::scanner::discovery::RepositoryScanner;
use crate::analyzers::{
    rust::RustAnalyzer, 
    python::PythonAnalyzer,
    traits::LanguageAnalyzer,
    registry::{LanguageAnalyzerRegistry, DefaultLanguageRegistry}
};
use crate::internal::{ai_tools::LocalAnalysisTools, config::FileScanningConfig};

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
            ],
            exclude_patterns: vec![
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/node_modules/**".to_string(),
                "**/test-repos/**".to_string(),
            ],
            max_file_size: 1024 * 1024, // 1MB
            max_depth: Some(20),
            follow_symlinks: false,
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
            println!("⚠️  No known project types detected in {}", path.as_ref().display());
            println!("💡 Using default configuration (Rust + Python)");
        } else {
            println!("🔍 Detected project languages: {}", detected_languages.join(", "));
        }
        
        let mut builder = Self::builder();
        
        // Register analyzers based on detection
        for language in &detected_languages {
            builder = match language.as_str() {
                "rust" => builder.with_rust_analyzer(),
                "python" => builder.with_python_analyzer(),
                // Future language support:
                // "typescript" => builder.with_typescript_analyzer(),
                // "javascript" => builder.with_javascript_analyzer(),
                _ => builder,
            };
        }
        
        // If nothing detected, use sensible defaults
        if detected_languages.is_empty() {
            builder = builder.with_rust_analyzer().with_python_analyzer();
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
                "**/*.pyi".to_string()
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
            // Future: .with_typescript_analyzer() when available
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
        if path.join("pyproject.toml").exists() || 
           path.join("requirements.txt").exists() ||
           path.join("setup.py").exists() ||
           path.join("poetry.lock").exists() ||
           Self::has_python_files(path) {
            languages.push("python".to_string());
        }
        
        // TypeScript/JavaScript project indicators (for future support)
        if path.join("package.json").exists() ||
           path.join("tsconfig.json").exists() ||
           Self::has_ts_js_files(path) {
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
                        entry.path().extension()
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
                        entry.path().extension()
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
        
        println!("🔍 Starting repository scan for: {}", path);
        let supported_langs = self.language_registry.list_supported_languages();
        if !supported_langs.is_empty() {
            println!("🌐 Registered analyzers: {}", supported_langs.join(", "));
        }

        // Discover files
        let scan_result = self.scanner.scan(path)
            .map_err(|e| LoreGrepError::InternalError(format!("File scanning failed: {}", e)))?;
        let discovered_files = scan_result.files;
        
        if discovered_files.is_empty() {
            println!("⚠️  No files found in the specified path");
            println!("💡 Check that the path exists and contains supported file types");
            return Ok(ScanResult::new(0, 0, 0, start_time.elapsed().as_millis() as u64, Vec::new()));
        }
        
        println!("📁 Found {} files to analyze", discovered_files.len());

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

            // Analyze file with appropriate analyzer based on language
            let analysis_result = if self.language_registry.list_supported_languages().contains(&file_info.language) {
                // Create analyzer instance based on language (temporary approach)
                match file_info.language.as_str() {
                    "rust" => {
                        let temp_analyzer = RustAnalyzer::new()
                            .map_err(|e| LoreGrepError::InternalError(format!("Rust analyzer creation failed: {}", e)))?;
                        temp_analyzer.analyze_file(&content, &file_info.path.to_string_lossy()).await
                    },
                    "python" => {
                        let temp_analyzer = PythonAnalyzer::new()
                            .map_err(|e| LoreGrepError::InternalError(format!("Python analyzer creation failed: {}", e)))?;
                        temp_analyzer.analyze_file(&content, &file_info.path.to_string_lossy()).await
                    },
                    _ => {
                        eprintln!("⚠️  Analyzer for '{}' not yet implemented", file_info.language);
                        continue;
                    }
                }
            } else {
                // Provide helpful error message for unsupported languages
                let supported_langs = self.language_registry.list_supported_languages();
                if supported_langs.is_empty() {
                    eprintln!("⚠️  No language analyzers registered! Use LoreGrep::builder().with_rust_analyzer() or .with_python_analyzer()");
                } else {
                    eprintln!("⚠️  No analyzer available for '{}' files. Supported: {}", 
                             file_info.language, 
                             supported_langs.join(", "));
                    // Suggest appropriate analyzer method
                    let suggestion = match file_info.language.as_str() {
                        "rust" => "with_rust_analyzer()",
                        "python" => "with_python_analyzer()", 
                        "typescript" | "javascript" => "with_typescript_analyzer() (coming soon)",
                        "go" => "with_go_analyzer() (coming soon)",
                        _ => "a custom analyzer for this language"
                    };
                    eprintln!("💡 Add support with: LoreGrep::builder().{}", suggestion);
                }
                continue;
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
                    eprintln!("Warning: Failed to analyze {}: {}", file_info.path.display(), e);
                }
            }
        }

        // Now add all analysis results to repo map (holding mutex only briefly)
        {
            let mut repo_map = self.repo_map.lock()
                .map_err(|e| LoreGrepError::InternalError(format!("Failed to lock repo map: {}", e)))?;
            
            for tree_node in analysis_results {
                if let Err(e) = repo_map.add_file(tree_node) {
                    eprintln!("Warning: Failed to store analysis: {}", e);
                }
            }
        } // Mutex guard is dropped here

        let duration = start_time.elapsed();
        
        // Print scan summary with enhanced feedback
        self.print_scan_summary(files_scanned, functions_found, structs_found, &languages, duration);
        
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
        // Create a temporary instance to get schemas
        let temp_repo_map = Arc::new(Mutex::new(RepoMap::new()));
        let temp_analyzer = RustAnalyzer::new().unwrap(); // Safe to unwrap for temp instance
        let temp_tools = LocalAnalysisTools::new(temp_repo_map, temp_analyzer);
        let ai_schemas = temp_tools.get_tool_schemas();
        
        // Convert from anthropic::ToolSchema to core::types::ToolSchema
        ai_schemas.into_iter().map(|schema| {
            ToolSchema::new(
                schema.name,
                schema.description,
                schema.input_schema,
            )
        }).collect()
    }

    /// Execute a tool call from the LLM
    /// Takes tool name and parameters, returns JSON result
    pub async fn execute_tool(&self, name: &str, params: Value) -> Result<ToolResult> {
        let ai_result = self.tools.execute_tool(name, params).await
            .map_err(|e| LoreGrepError::ToolError(format!("Tool execution failed: {}", e)))?;
        
        // Convert from ai_tools::ToolResult to core::types::ToolResult
        if ai_result.success {
            Ok(ToolResult::success(ai_result.data))
        } else {
            Ok(ToolResult::error(ai_result.error.unwrap_or_else(|| "Unknown error".to_string())))
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

    /// Print a comprehensive scan summary with language breakdown
    fn print_scan_summary(&self, files_scanned: usize, functions_found: usize, structs_found: usize, languages: &std::collections::HashSet<String>, duration: std::time::Duration) {
        if files_scanned == 0 {
            println!("\n📊 Scan Summary:");
            println!("   ⚠️  No files found matching your criteria");
            println!("   💡 Check your include/exclude patterns or language analyzers");
            println!("   📁 Supported languages: {:?}", self.language_registry.list_supported_languages());
            return;
        }
        
        println!("\n📊 Scan Summary:");
        println!("   📁 Files analyzed: {}", files_scanned);
        println!("   🔧 Functions found: {}", functions_found);
        println!("   🏗️  Structs found: {}", structs_found);
        println!("   🌐 Languages detected: {:?}", languages.iter().cloned().collect::<Vec<_>>());
        println!("   ⏱️  Scan duration: {:.2}s", duration.as_secs_f64());
        
        if functions_found > 0 || structs_found > 0 {
            println!("   ✅ Repository successfully indexed and ready for queries!");
        }
    }

    /// Get repository statistics
    pub fn get_stats(&self) -> Result<ScanResult> {
        let repo_map = self.repo_map.lock()
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
                    "**/*.pyi".to_string()
                ]),
                "typescript" => patterns.extend(vec![
                    "**/*.ts".to_string(), 
                    "**/*.tsx".to_string()
                ]),
                "javascript" => patterns.extend(vec![
                    "**/*.js".to_string(), 
                    "**/*.jsx".to_string(),
                    "**/*.mjs".to_string()
                ]),
                "go" => patterns.extend(vec!["**/*.go".to_string()]),
                _ => {}
            }
        }
        
        if !patterns.is_empty() {
            println!("📁 Configuring file patterns for detected languages: {}", 
                     patterns.join(", "));
            self.config.include_patterns = patterns;
        }
        
        self
    }
    
    /// Enable all available analyzers
    pub fn with_all_analyzers(self) -> Self {
        self.with_rust_analyzer()
            .with_python_analyzer()
            // Future: .with_typescript_analyzer() when available
    }
    
    /// Quick setup for common exclusions
    pub fn exclude_common_build_dirs(mut self) -> Self {
        self.config.exclude_patterns.extend(vec![
            "**/target/**".to_string(),     // Rust
            "**/build/**".to_string(),      // General
            "**/dist/**".to_string(),       // JavaScript/TypeScript
            "**/.build/**".to_string(),     // Xcode
            "**/node_modules/**".to_string(), // JavaScript/TypeScript
            "**/__pycache__/**".to_string(), // Python
            "**/venv/**".to_string(),       // Python virtual env
            "**/.venv/**".to_string(),      // Python virtual env
        ]);
        self
    }
    
    /// Quick setup for common include patterns
    pub fn include_source_files(mut self) -> Self {
        self.config.include_patterns.extend(vec![
            "**/src/**/*.rs".to_string(),   // Rust source
            "**/lib/**/*.py".to_string(),   // Python libs
            "**/app/**/*.js".to_string(),   // JavaScript apps
        ]);
        self
    }
    
    /// Quick setup for common test directories exclusion
    pub fn exclude_test_dirs(mut self) -> Self {
        self.config.exclude_patterns.extend(vec![
            "**/tests/**".to_string(),      // General test dirs
            "**/test/**".to_string(),       // General test dirs  
            "**/*_test.rs".to_string(),     // Rust test files
            "**/*_test.py".to_string(),     // Python test files
            "**/test_*.py".to_string(),     // Python test files
            "**/*.test.js".to_string(),     // JavaScript test files
            "**/*.test.ts".to_string(),     // TypeScript test files
            "**/__tests__/**".to_string(),  // Jest test dirs
        ]);
        self
    }
    
    /// Quick setup for vendor/dependency directories exclusion
    pub fn exclude_vendor_dirs(mut self) -> Self {
        self.config.exclude_patterns.extend(vec![
            "**/vendor/**".to_string(),     // General vendor dirs
            "**/vendors/**".to_string(),    // Alternative vendor naming
            "**/third_party/**".to_string(), // Third party code
            "**/extern/**".to_string(),     // External dependencies
            "**/.cargo/**".to_string(),     // Cargo cache
            "**/Pods/**".to_string(),       // CocoaPods
        ]);
        self
    }
    
    /// Include configuration files (useful for understanding project structure)
    pub fn include_config_files(mut self) -> Self {
        self.config.include_patterns.extend(vec![
            "**/Cargo.toml".to_string(),    // Rust config
            "**/pyproject.toml".to_string(), // Python config
            "**/package.json".to_string(),  // Node.js config
            "**/tsconfig.json".to_string(), // TypeScript config
            "**/*.toml".to_string(),        // General TOML configs
            "**/*.yaml".to_string(),        // YAML configs
            "**/*.yml".to_string(),         // YAML configs
        ]);
        self
    }
    
    /// Configure for performance - exclude large/binary files and limit depth
    pub fn optimize_for_performance(mut self) -> Self {
        self.config.max_file_size = 512 * 1024; // 512KB limit
        self.config.max_depth = Some(8);        // Reasonable depth limit
        self.config.exclude_patterns.extend(vec![
            "**/*.lock".to_string(),        // Lock files (often large)
            "**/*.log".to_string(),         // Log files
            "**/*.tmp".to_string(),         // Temporary files
            "**/*.cache".to_string(),       // Cache files
            "**/*.bin".to_string(),         // Binary files
            "**/*.so".to_string(),          // Shared libraries
            "**/*.dll".to_string(),         // Windows libraries
            "**/*.exe".to_string(),         // Executables
        ]);
        self
    }
    
    /// Configure for comprehensive analysis - include more file types and increase limits
    pub fn comprehensive_analysis(mut self) -> Self {
        self.config.max_file_size = 5 * 1024 * 1024; // 5MB limit
        self.config.max_depth = Some(20);            // Deep traversal
        self.config.include_patterns.extend(vec![
            "**/*.md".to_string(),          // Documentation
            "**/*.txt".to_string(),         // Text files
            "**/*.json".to_string(),        // JSON configs
            "**/*.xml".to_string(),         // XML files
            "**/*.toml".to_string(),        // TOML configs
            "**/*.yaml".to_string(),        // YAML configs
            "**/*.yml".to_string(),         // YAML configs
        ]);
        self
    }

    /// Add Rust language analyzer (enabled by default)
    pub fn with_rust_analyzer(mut self) -> Self {
        match RustAnalyzer::new() {
            Ok(analyzer) => {
                if let Err(e) = self.registry.register(Box::new(analyzer)) {
                    eprintln!("❌ Failed to register Rust analyzer: {}", e);
                    if e.to_string().contains("already registered") {
                        eprintln!("💡 Rust analyzer is already registered - no action needed");
                    }
                } else {
                    println!("✅ Rust analyzer registered successfully");
                    println!("📄 Supports: .rs files");
                }
            }
            Err(e) => {
                eprintln!("❌ Rust analyzer unavailable: {}", e);
                eprintln!("⚠️  Rust files (.rs) will be skipped during scanning");
            }
        }
        self
    }

    /// Add Python language analyzer
    pub fn with_python_analyzer(mut self) -> Self {
        match PythonAnalyzer::new() {
            Ok(analyzer) => {
                if let Err(e) = self.registry.register(Box::new(analyzer)) {
                    eprintln!("❌ Failed to register Python analyzer: {}", e);
                    if e.to_string().contains("already registered") {
                        eprintln!("💡 Python analyzer is already registered - no action needed");
                    }
                } else {
                    println!("✅ Python analyzer registered successfully");
                    println!("📄 Supports: .py, .pyx, .pyi files");
                }
            }
            Err(e) => {
                eprintln!("❌ Python analyzer unavailable: {}", e);
                eprintln!("⚠️  Python files (.py, .pyx, .pyi) will be skipped during scanning");
            }
        }
        self
    }

    /// Add TypeScript/JavaScript analyzer (future)
    pub fn with_typescript_analyzer(self) -> Self {
        // TODO: Implement when TypeScript analyzer is available
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

    /// Enable or disable respecting .gitignore files (alias for follow_symlinks for now)
    pub fn respect_gitignore(self, respect: bool) -> Self {
        // For now, map this to follow_symlinks as a placeholder
        // In a more complete implementation, this would be a separate config field
        self.follow_symlinks(!respect)
    }

    /// Disable maximum depth limit
    pub fn unlimited_depth(mut self) -> Self {
        self.config.max_depth = None;
        self
    }

    /// Build the LoreGrep instance with validation
    pub fn build(self) -> Result<LoreGrep> {
        // Validate that at least one analyzer is registered
        let supported_languages = self.registry.list_supported_languages();
        if supported_languages.is_empty() {
            eprintln!("⚠️  Warning: No language analyzers registered!");
            eprintln!("💡 Consider adding: .with_rust_analyzer() or .with_python_analyzer()");
            eprintln!("📁 Files will be discovered but not analyzed");
        } else {
            println!("🎆 LoreGrep configured with {} language(s): {}", 
                     supported_languages.len(), 
                     supported_languages.join(", "));
        }
        let repo_map = Arc::new(Mutex::new(RepoMap::new()));
        let default_config = FileScanningConfig {
            include_patterns: self.config.include_patterns.clone(),
            exclude_patterns: self.config.exclude_patterns.clone(),
            follow_symlinks: self.config.follow_symlinks,
            max_file_size: self.config.max_file_size,
            max_depth: self.config.max_depth,
        };
        let scanner = RepositoryScanner::new(&default_config, None)
            .map_err(|e| LoreGrepError::InternalError(format!("Scanner creation failed: {}", e)))?;
        let analyzer = RustAnalyzer::new()
            .map_err(|e| LoreGrepError::InternalError(format!("Analyzer creation failed: {}", e)))?;
        
        // Create tools with reference to repo_map
        let tools = LocalAnalysisTools::new(
            repo_map.clone(),
            analyzer,
        );

        let loregrep = LoreGrep {
            repo_map,
            scanner,
            tools,
            config: self.config,
            language_registry: Arc::new(self.registry),
        };
        
        println!("✅ LoreGrep instance created successfully!");
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
        use tempfile::TempDir;
        use std::fs;
        
        // Create a temporary directory with test files
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.rs");
        fs::write(&test_file, r#"
            pub fn hello_world() -> String {
                "Hello, World!".to_string()
            }
            
            pub struct TestStruct {
                pub name: String,
                pub value: i32,
            }
        "#).unwrap();
        
        // Create LoreGrep instance and scan
        let mut loregrep = LoreGrep::builder()
            .max_files(100)
            .build()
            .unwrap();
        
        let scan_result = loregrep.scan(temp_dir.path().to_str().unwrap()).await;
        assert!(scan_result.is_ok());
        
        let result = scan_result.unwrap();
        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.functions_found, 1);
        assert_eq!(result.structs_found, 1);
        assert!(result.languages.contains(&"rust".to_string()));
        
        // Test tool execution
        let search_result = loregrep.execute_tool("search_functions", json!({
            "pattern": "hello",
            "limit": 10
        })).await;
        
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
            "get_repository_tree"
        ];
        
        for tool_name in expected_tools {
            assert!(tools.iter().any(|t| t.name == tool_name), "Missing tool: {}", tool_name);
        }
        
        // Test repository tree tool on empty repository
        let tree_result = loregrep.execute_tool("get_repository_tree", json!({
            "include_file_details": false,
            "max_depth": 1
        })).await;
        
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
        
        let result = loregrep.scan("/this/path/definitely/does/not/exist/anywhere").await;
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
            _ => panic!("Unexpected error type")
        }
    }

    #[test]
    fn test_builder_default_exclusions() {
        let _builder = LoreGrep::builder();
        
        // Verify default exclusions include test-repos
        let config = LoreGrepConfig::default();
        assert!(config.exclude_patterns.contains(&"**/test-repos/**".to_string()));
        assert!(config.exclude_patterns.contains(&"**/target/**".to_string()));
        assert!(config.exclude_patterns.contains(&"**/.git/**".to_string()));
        assert!(config.exclude_patterns.contains(&"**/node_modules/**".to_string()));
    }
}