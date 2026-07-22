use anyhow::{Context, Result};
use serde_json;
use std::path::Path;
use std::time::Instant;
use tracing::info;

// Use public API instead of direct internal access
use crate::{
    LoreGrep,
    core::types::ScanResult as PublicScanResult,
    internal::{
        cli_types::{AnalyzeArgs, ExecToolArgs, ScanArgs, SearchArgs},
        config::CliConfig,
    },
    loregrep::LoreGrepConfig,
};

/// Lightweight search result used for plain, machine-facing output.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub result_type: String,
    pub content: String,
    pub file_path: String,
    pub line: Option<u32>,
    pub context: Option<String>,
}

impl SearchResult {
    pub fn new(result_type: String, content: String, file_path: String, line: Option<u32>) -> Self {
        Self {
            result_type,
            content,
            file_path,
            line,
            context: None,
        }
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

pub struct CliApp {
    config: CliConfig,
    loregrep: LoreGrep,
    verbose: bool,
}

impl CliApp {
    pub async fn new(config: CliConfig, verbose: bool, _colors_enabled: bool) -> Result<Self> {
        info!("Initializing Loregrep CLI");

        // Create LoreGrep instance using public API
        let mut builder = LoreGrep::builder()
            .with_all_analyzers() // Rust, Python, TypeScript/TSX
            // One default file limit for the whole project (see the const's
            // documentation for why 10,000).
            .max_files(LoreGrepConfig::DEFAULT_MAX_FILES)
            .include_patterns(config.file_scanning.include_patterns.clone())
            .exclude_patterns(config.file_scanning.exclude_patterns.clone())
            .max_file_size(config.file_scanning.max_file_size)
            .follow_symlinks(config.file_scanning.follow_symlinks);

        // Configure depth limit
        if let Some(depth) = config.file_scanning.max_depth {
            builder = builder.max_depth(depth);
        } else {
            builder = builder.unlimited_depth();
        }

        let loregrep = builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create LoreGrep instance: {}", e))?;

        // Create the cache directory itself if it doesn't exist. (This used to
        // create `config.cache.path.parent()` — the *parent* of the cache
        // directory — which is not a directory anything ever writes to.)
        if config.cache.enabled {
            tokio::fs::create_dir_all(&config.cache.path)
                .await
                .context("Failed to create cache directory")?;
        }

        if verbose {
            eprintln!("LoreGrep initialized with public API");
        }

        Ok(Self {
            config,
            loregrep,
            verbose,
        })
    }

    pub async fn scan(&mut self, args: ScanArgs) -> Result<()> {
        let start_time = Instant::now();

        // Show absolute path for clarity
        let abs_path = args
            .path
            .canonicalize()
            .unwrap_or_else(|_| args.path.clone());

        if self.verbose {
            eprintln!("Scanning directory: {}", abs_path.display());
            eprintln!(
                "Include patterns: {:?}",
                self.config.file_scanning.include_patterns
            );
            eprintln!(
                "Exclude patterns: {:?}",
                self.config.file_scanning.exclude_patterns
            );
        }

        // Use public API to scan the repository
        let scan_result = self
            .loregrep
            .scan(&args.path.to_string_lossy())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scan repository: {}", e))?;

        // Display scan results using public API data
        self.print_public_scan_results(&scan_result);

        // Cache results if enabled
        if args.cache && self.config.cache.enabled {
            self.save_cache(&args.path);
        }

        if self.verbose {
            eprintln!("Total scan time: {:?}", start_time.elapsed());
        }

        Ok(())
    }

    /// The persisted index cache file for `root`, or `None` when caching is off.
    ///
    /// The cache lives under the user's own cache directory
    /// (`config.cache.path`), never inside the analysed tree: a query is a read,
    /// and a read must not create files in someone else's repository (K11).
    fn index_cache_path(&self, root: &Path) -> Option<std::path::PathBuf> {
        if !self.config.cache.enabled {
            return None;
        }
        match LoreGrep::cache_path_for(&self.config.cache.path, root) {
            Ok(path) => Some(path),
            Err(e) => {
                eprintln!("Cache disabled for this run: {}", e);
                None
            }
        }
    }

    /// Make sure an index for `root` is in memory: reuse a validated persisted
    /// cache if there is one, otherwise scan and persist the result.
    ///
    /// Both `exec-tool` and `search` need exactly this, and having them share
    /// one method is what keeps them from drifting apart — `search` previously
    /// had no load-or-scan step at all and so reported "Repository not scanned"
    /// on every single invocation (K7).
    ///
    /// Validation only runs when the cache file exists — on the first run there
    /// is no cache, so we go straight to the single scan rather than walking the
    /// tree twice.
    ///
    /// `load_index_if_fresh` validates the cache against this build, this
    /// configuration and the files actually on disk (path set + content hashes)
    /// before installing it, and checks the recorded analysis root. Any mismatch
    /// — an added file with a preserved old mtime, a deleted file, a changed
    /// configuration, a cache belonging to a different tree — falls through to a
    /// rescan.
    async fn ensure_index(&mut self, root: &Path) -> Result<()> {
        let cache_path = self.index_cache_path(root);

        let loaded_from_cache = match &cache_path {
            Some(path) => match self.loregrep.load_index_if_fresh(path, root) {
                Ok(true) => {
                    eprintln!("Loaded index cache from {}", path.display());
                    true
                }
                Ok(false) => false,
                Err(e) => {
                    eprintln!("Cache load failed ({}); rescanning", e);
                    false
                }
            },
            None => false,
        };

        if !loaded_from_cache {
            self.loregrep
                .scan(&root.to_string_lossy())
                .await
                .map_err(|e| anyhow::anyhow!("Scan failed: {}", e))?;

            // Persist the freshly built index so the next invocation can skip
            // the scan. Uses the shared, non-fatal save path.
            self.save_cache(root);
        }

        Ok(())
    }

    /// Execute a single analysis tool and print its `ToolResult` as JSON to stdout.
    ///
    /// The index is obtained through [`CliApp::ensure_index`] (validated cache,
    /// else scan). Cache use is opportunistic (no flag required) and honours
    /// `config.cache.enabled`. All diagnostics go to stderr, so stdout carries
    /// only the JSON result (for agent/tool consumption).
    pub async fn exec_tool(&mut self, args: ExecToolArgs) -> Result<()> {
        self.ensure_index(&args.path).await?;

        let params: serde_json::Value = serde_json::from_str(&args.params)
            .map_err(|e| anyhow::anyhow!("Invalid --params JSON: {}", e))?;

        let result = self
            .loregrep
            .execute_tool(&args.tool, params)
            .await
            .map_err(|e| anyhow::anyhow!("Tool execution failed: {}", e))?;

        println!("{}", serde_json::to_string_pretty(&result)?);

        if !result.success {
            std::process::exit(1);
        }
        Ok(())
    }

    /// Search the index for `args.query`.
    ///
    /// This used to bail out with "Repository not scanned. Run 'scan' first" —
    /// unconditionally, because every process starts with an empty index and
    /// nothing here ever loaded the persisted one, so the advice was both
    /// useless and impossible to follow (K7). It now obtains an index the same
    /// way `exec-tool` does.
    pub async fn search(&mut self, args: SearchArgs) -> Result<()> {
        self.ensure_index(&args.path).await?;

        let start_time = Instant::now();

        if self.verbose {
            eprintln!("Query: {}", args.query);
            eprintln!("Search type: {}", args.r#type);
            eprintln!(
                "Fuzzy matching: {}",
                if args.fuzzy { "enabled" } else { "disabled" }
            );
        }

        // Perform search using public API tools
        let results =
            match args.r#type.as_str() {
                "function" | "func" => {
                    let tool_result = self
                        .loregrep
                        .execute_tool(
                            "search_functions",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit
                            }),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("Function search failed: {}", e))?;

                    if tool_result.success {
                        self.convert_tool_result_to_search_results(tool_result.data, "function")
                    } else {
                        eprintln!("Search failed: {:?}", tool_result.error);
                        Vec::new()
                    }
                }
                "struct" => {
                    let tool_result = self
                        .loregrep
                        .execute_tool(
                            "search_structs",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit
                            }),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("Struct search failed: {}", e))?;

                    if tool_result.success {
                        self.convert_tool_result_to_search_results(tool_result.data, "struct")
                    } else {
                        eprintln!("Search failed: {:?}", tool_result.error);
                        Vec::new()
                    }
                }
                "all" => {
                    let mut all_results = Vec::new();

                    // Search functions
                    if let Ok(func_result) = self
                        .loregrep
                        .execute_tool(
                            "search_functions",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit / 2
                            }),
                        )
                        .await
                    {
                        if func_result.success {
                            all_results.extend(self.convert_tool_result_to_search_results(
                                func_result.data,
                                "function",
                            ));
                        }
                    }

                    // Search structs
                    if let Ok(struct_result) = self
                        .loregrep
                        .execute_tool(
                            "search_structs",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit / 2
                            }),
                        )
                        .await
                    {
                        if struct_result.success {
                            all_results.extend(self.convert_tool_result_to_search_results(
                                struct_result.data,
                                "struct",
                            ));
                        }
                    }

                    all_results
                }
                _ => {
                    eprintln!(
                        "Unknown search type: {}. Available types: function, struct, all",
                        args.r#type
                    );
                    return Ok(());
                }
            };

        // Print results (data goes to stdout)
        for result in &results {
            match result.line {
                Some(line) => println!("{}:{}: {}", result.file_path, line, result.content),
                None => println!("{}: {}", result.file_path, result.content),
            }
        }

        if self.verbose && !results.is_empty() {
            eprintln!("Search completed in {:?}", start_time.elapsed());
        }

        Ok(())
    }

    pub async fn analyze(&mut self, args: AnalyzeArgs) -> Result<()> {
        if !args.file.exists() {
            eprintln!("Path not found: {}", args.file.display());
            return Ok(());
        }

        let start_time = Instant::now();

        if args.file.is_dir() {
            // Directory analysis - analyze all files in the directory
            if self.verbose {
                eprintln!("Analyzing directory: {}", args.file.display());
                eprintln!("Output format: {}", args.format);
            }

            // Scan the directory first to populate the in-memory RepoMap;
            // `get_repository_tree` reads from that index, so without a scan a
            // real run would print an empty tree.
            self.loregrep
                .scan(&args.file.to_string_lossy())
                .await
                .map_err(|e| anyhow::anyhow!("Directory scan failed: {}", e))?;

            // There is no dedicated "analyze_directory" tool; the repository
            // tree tool is the natural fit for a directory overview.
            let tool_result = self
                .loregrep
                .execute_tool(
                    "get_repository_tree",
                    serde_json::json!({
                        "include_file_details": true,
                        "max_depth": 0
                    }),
                )
                .await
                .map_err(|e| anyhow::anyhow!("Directory analysis failed: {}", e))?;

            if !tool_result.success {
                eprintln!("Analysis failed: {:?}", tool_result.error);
                return Ok(());
            }

            // Display directory results
            self.display_directory_analysis(&tool_result.data, &args);

            if self.verbose {
                eprintln!("Directory analysis completed in {:?}", start_time.elapsed());
            }
        } else {
            // Single file analysis
            if self.verbose {
                eprintln!("Analyzing file: {}", args.file.display());
                eprintln!("Output format: {}", args.format);
            }

            // A human naming a file on the command line IS the authorization, so
            // the file's own directory becomes the analysis root for this
            // invocation. The containment rule exists to stop AGENT-supplied
            // parameters — which may be relaying untrusted text — from reaching
            // arbitrary paths; it is not here to second-guess an explicit
            // argument the user typed.
            if let Some(parent) = args.file.parent() {
                let root = if parent.as_os_str().is_empty() {
                    std::path::Path::new(".")
                } else {
                    parent
                };
                self.loregrep.set_scan_root(&root.to_string_lossy());
            }

            // Use public API to analyze file
            let tool_result = self
                .loregrep
                .execute_tool(
                    "analyze_file",
                    serde_json::json!({
                        "file_path": args.file.to_string_lossy(),
                        "include_content": true
                    }),
                )
                .await
                .map_err(|e| anyhow::anyhow!("File analysis failed: {}", e))?;

            if !tool_result.success {
                eprintln!("Analysis failed: {:?}", tool_result.error);
                return Ok(());
            }

            // Display results based on format
            match args.format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&tool_result.data)
                        .context("Failed to serialize analysis to JSON")?;
                    println!("{}", json);
                }
                "text" => {
                    self.display_tool_analysis_text(&tool_result.data, &args);
                }
                "tree" => {
                    self.display_tool_analysis_tree(&tool_result.data);
                }
                _ => {
                    eprintln!("Unknown output format: {}", args.format);
                    return Ok(());
                }
            }

            if self.verbose {
                eprintln!("Analysis completed in {:?}", start_time.elapsed());
            }
        }

        Ok(())
    }

    pub async fn show_config(&self) -> Result<()> {
        let config_json = serde_json::to_string_pretty(&self.config)
            .context("Failed to serialize configuration")?;

        println!("{}", config_json);

        Ok(())
    }

    // Helper methods for public API conversion
    fn print_public_scan_results(&self, scan_result: &PublicScanResult) {
        println!("Files scanned: {}", scan_result.files_scanned);
        println!("Functions found: {}", scan_result.functions_found);
        println!("Structs found: {}", scan_result.structs_found);
        println!("Duration: {}ms", scan_result.duration_ms);

        if !scan_result.languages.is_empty() {
            println!("Languages: {:?}", scan_result.languages);
        }
    }

    fn convert_tool_result_to_search_results(
        &self,
        data: serde_json::Value,
        result_type: &str,
    ) -> Vec<SearchResult> {
        let mut results = Vec::new();

        // The search tools return an object of the shape
        // `{ "status": .., "pattern": .., "results": [ ..items.. ], "count": N }`,
        // so read the items out of the `results` array (not the top-level value).
        if let Some(items) = data.get("results").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    let file_path = item
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    // Items are serialized `FunctionSignature`/`StructSignature`,
                    // which expose `start_line`/`end_line` rather than `line_number`.
                    let line_number = item
                        .get("start_line")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32);

                    let signature = match result_type {
                        "function" => {
                            let params = item
                                .get("parameters")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            let return_type = item
                                .get("return_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if return_type.is_empty() {
                                format!("fn {}(...) [{}params]", name, params)
                            } else {
                                format!("fn {}(...) -> {} [{}params]", name, return_type, params)
                            }
                        }
                        "struct" => {
                            let fields = item
                                .get("fields")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            format!("struct {} {{ {}fields }}", name, fields)
                        }
                        _ => name.to_string(),
                    };

                    results.push(SearchResult::new(
                        result_type.to_string(),
                        signature,
                        file_path,
                        line_number,
                    ));
                }
            }
        }

        results
    }

    /// The path to SHOW a human for a tool result.
    ///
    /// Tool JSON is root-relative by contract (stable across machines, cheap in
    /// tokens); a person reading a terminal wants the path they can paste, so the
    /// display layer rejoins it with the `analysis_root` the response carries.
    fn displayable_path(data: &serde_json::Value, file_path: &str) -> String {
        match data.get("analysis_root").and_then(|v| v.as_str()) {
            Some(root) if !root.is_empty() && !Path::new(file_path).is_absolute() => {
                Path::new(root)
                    .join(file_path)
                    .to_string_lossy()
                    .to_string()
            }
            _ => file_path.to_string(),
        }
    }

    fn display_tool_analysis_text(&self, data: &serde_json::Value, args: &AnalyzeArgs) {
        if let Some(file_path) = data.get("file_path").and_then(|v| v.as_str()) {
            println!("File: {}", Self::displayable_path(data, file_path));
        }

        if let Some(language) = data.get("language").and_then(|v| v.as_str()) {
            println!("Language: {}", language);
        }

        // Display functions
        if args.functions || (!args.structs && !args.imports) {
            if let Some(functions) = data.get("functions").and_then(|v| v.as_array()) {
                if !functions.is_empty() {
                    println!("Functions:");
                    for func in functions {
                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                            let params = func
                                .get("parameters")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            let return_type = func
                                .get("return_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            println!(
                                "  fn {}({} params) -> {}",
                                name,
                                params,
                                if return_type.is_empty() {
                                    "()"
                                } else {
                                    return_type
                                }
                            );
                        }
                    }
                }
            }
        }

        // Display structs
        if args.structs || (!args.functions && !args.imports) {
            if let Some(structs) = data.get("structs").and_then(|v| v.as_array()) {
                if !structs.is_empty() {
                    println!("Structs:");
                    for struct_item in structs {
                        if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                            let fields = struct_item
                                .get("fields")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            println!("  struct {} {{ {} fields }}", name, fields);
                        }
                    }
                }
            }
        }
    }

    fn display_tool_analysis_tree(&self, data: &serde_json::Value) {
        if let Some(file_path) = data.get("file_path").and_then(|v| v.as_str()) {
            println!("{}", Self::displayable_path(data, file_path));

            if let Some(functions) = data.get("functions").and_then(|v| v.as_array()) {
                for func in functions {
                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                        println!("  fn {}", name);
                    }
                }
            }

            if let Some(structs) = data.get("structs").and_then(|v| v.as_array()) {
                for struct_item in structs {
                    if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                        println!("  struct {}", name);
                    }
                }
            }
        }
    }

    /// Collect all `File` nodes from a `get_repository_tree` directory node,
    /// walking directories recursively. Each returned value is the `FileNode`
    /// JSON object (with a `skeleton` field).
    fn collect_tree_files<'a>(node: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                match child.get("type").and_then(|v| v.as_str()) {
                    Some("File") => out.push(child),
                    Some("Directory") => Self::collect_tree_files(child, out),
                    _ => {}
                }
            }
        }
    }

    fn display_directory_analysis(&self, data: &serde_json::Value, args: &AnalyzeArgs) {
        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
                println!("{}", json);
            }
            "text" => {
                let tree = data
                    .get("repository_tree")
                    .unwrap_or(&serde_json::Value::Null);
                let mut files = Vec::new();
                Self::collect_tree_files(tree, &mut files);

                let mut total_functions = 0;
                let mut total_structs = 0;

                for file_data in &files {
                    let skeleton = file_data
                        .get("skeleton")
                        .unwrap_or(&serde_json::Value::Null);

                    let file_path = skeleton
                        .get("path")
                        .and_then(|v| v.as_str())
                        .or_else(|| file_data.get("path").and_then(|v| v.as_str()))
                        .unwrap_or("unknown");
                    println!("File: {}", file_path);

                    if let Some(language) = skeleton.get("language").and_then(|v| v.as_str()) {
                        if !language.is_empty() {
                            println!("Language: {}", language);
                        }
                    }

                    // Display functions
                    if args.functions || (!args.structs && !args.imports) {
                        if let Some(functions) =
                            skeleton.get("functions").and_then(|v| v.as_array())
                        {
                            if !functions.is_empty() {
                                println!("Functions:");
                                for func in functions {
                                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                        let params = func
                                            .get("parameter_count")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        let return_type = func
                                            .get("return_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");

                                        println!(
                                            "  fn {}({} params) -> {}",
                                            name,
                                            params,
                                            if return_type.is_empty() {
                                                "()"
                                            } else {
                                                return_type
                                            }
                                        );
                                        total_functions += 1;
                                    }
                                }
                            }
                        }
                    }

                    // Display structs
                    if args.structs || (!args.functions && !args.imports) {
                        if let Some(structs) = skeleton.get("structs").and_then(|v| v.as_array()) {
                            if !structs.is_empty() {
                                println!("Structs:");
                                for struct_item in structs {
                                    if let Some(name) =
                                        struct_item.get("name").and_then(|v| v.as_str())
                                    {
                                        let fields = struct_item
                                            .get("field_count")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        println!("  struct {} {{ {} fields }}", name, fields);
                                        total_structs += 1;
                                    }
                                }
                            }
                        }
                    }

                    println!(); // Blank line between files
                }

                // Summary
                println!(
                    "Summary: {} functions, {} structs across {} files",
                    total_functions,
                    total_structs,
                    files.len()
                );
            }
            "tree" => {
                let tree = data
                    .get("repository_tree")
                    .unwrap_or(&serde_json::Value::Null);

                // The tree's own `root_path` is now the root-relative ".", which
                // tells a human nothing; the absolute root travels alongside as
                // `analysis_root`. Prefer it, and fall back for a response that
                // predates it.
                if let Some(root_path) = data
                    .get("analysis_root")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        data.get("metadata")
                            .and_then(|m| m.get("root_path"))
                            .and_then(|v| v.as_str())
                    })
                    .or_else(|| tree.get("path").and_then(|v| v.as_str()))
                {
                    println!("{}", root_path);
                }

                let mut files = Vec::new();
                Self::collect_tree_files(tree, &mut files);

                for file_data in &files {
                    let skeleton = file_data
                        .get("skeleton")
                        .unwrap_or(&serde_json::Value::Null);
                    let file_name = file_data
                        .get("name")
                        .and_then(|v| v.as_str())
                        .or_else(|| skeleton.get("path").and_then(|v| v.as_str()))
                        .unwrap_or("unknown");
                    println!("  {}", file_name);

                    if let Some(functions) = skeleton.get("functions").and_then(|v| v.as_array()) {
                        for func in functions {
                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                println!("    fn {}", name);
                            }
                        }
                    }

                    if let Some(structs) = skeleton.get("structs").and_then(|v| v.as_array()) {
                        for struct_item in structs {
                            if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                                println!("    struct {}", name);
                            }
                        }
                    }
                }
            }
            _ => {
                eprintln!("Unknown output format: {}", args.format);
            }
        }
    }

    // Helper methods

    /// Persist the current in-memory index to this repository's entry in the
    /// user's cache directory so later invocations can skip rescanning.
    ///
    /// This is the single save path shared by `scan` and `ensure_index`: it owns
    /// the cache-path computation and the error policy. Cache saving is a
    /// best-effort optimization, so a failure is non-fatal — it warns on stderr
    /// and returns rather than aborting the command. Diagnostics go to stderr
    /// only; stdout is untouched (machine-first output).
    /// A truncated index is deliberately not persisted: caching a partial view
    /// of the repository would let the next run reload it as authoritative.
    fn save_cache(&self, root_path: &Path) {
        let Some(cache_path) = self.index_cache_path(root_path) else {
            return; // caching disabled
        };

        let coverage = self.loregrep.index_coverage();
        if coverage.truncated {
            eprintln!(
                "Not caching a truncated index ({} of {} files); raise max_files and rescan",
                coverage.files_indexed, coverage.files_discovered
            );
            return;
        }

        match self.loregrep.save_index(&cache_path) {
            Ok(()) => eprintln!("Saved index cache to {}", cache_path.display()),
            Err(e) => eprintln!("Warning: failed to save index cache: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::test;

    /// A config whose index cache lives somewhere disposable.
    ///
    /// `CliConfig::default()` points at the *developer's real* cache directory,
    /// so tests that ran a scan used to write there. Every test gets its own
    /// root; tests that need two `CliApp`s to share a cache use
    /// [`create_test_config_with_cache`] instead.
    fn create_test_config() -> CliConfig {
        let mut config = CliConfig::default();
        config.cache.path = unique_temp_cache_root();
        config
    }

    fn create_test_config_with_cache(cache_dir: &TempDir) -> CliConfig {
        let mut config = CliConfig::default();
        config.cache.path = cache_dir.path().to_path_buf();
        config
    }

    fn unique_temp_cache_root() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "loregrep-test-cache-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// Snapshot of every path under `root` and the bytes of every file, used to
    /// prove a command did not modify the tree it read.
    fn tree_snapshot(root: &Path) -> Vec<(std::path::PathBuf, Option<Vec<u8>>)> {
        fn walk(dir: &Path, out: &mut Vec<(std::path::PathBuf, Option<Vec<u8>>)>) {
            let mut entries: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    out.push((path.clone(), None));
                    walk(&path, out);
                } else {
                    out.push((path.clone(), Some(fs::read(&path).unwrap())));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, &mut out);
        out
    }

    fn create_test_rust_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.path().join(name);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    #[test]
    async fn test_cli_app_creation() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await;
        assert!(app.is_ok());
    }

    #[test]
    async fn test_analyze_simple_rust_file() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = r#"
pub fn hello_world() -> String {
    "Hello, World!".to_string()
}

pub struct TestStruct {
    pub name: String,
    pub value: i32,
}

use std::collections::HashMap;
"#;
        let file_path = create_test_rust_file(&temp_dir, "test.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();
        // Scan first, as every real invocation does: analyze_file resolves its
        // parameter against the analysis root and refuses when none is known,
        // rather than reading relative to the process cwd.
        app.loregrep
            .scan(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();

        // Use public API to analyze file
        let result = app
            .loregrep
            .execute_tool(
                "analyze_file",
                serde_json::json!({
                    "file_path": file_path.to_string_lossy(),
                    "include_source": false
                }),
            )
            .await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.success);

        // Check that we got analysis data
        assert!(tool_result.data.get("language").is_some());
        assert!(tool_result.data.get("functions").is_some());
        assert!(tool_result.data.get("structs").is_some());
    }

    #[test]
    async fn test_scan_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple Rust files
        create_test_rust_file(&temp_dir, "main.rs", "fn main() {}");
        create_test_rust_file(&temp_dir, "lib.rs", "pub fn lib_func() {}");
        create_test_rust_file(&temp_dir, "utils.rs", "pub struct Utils {}");

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let scan_args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            cache: false,
        };

        let result = app.scan(scan_args).await;
        assert!(result.is_ok());

        // Check that repository was scanned using public API
        assert!(app.loregrep.is_scanned());
        let stats = app.loregrep.get_stats().unwrap();
        assert!(stats.files_scanned > 0);
    }

    #[test]
    async fn test_analyze_command() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = r#"
pub fn test_function(x: i32, y: String) -> bool {
    x > 0 && !y.is_empty()
}

struct PrivateStruct {
    field: String,
}
"#;
        let file_path = create_test_rust_file(&temp_dir, "test.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: file_path,
            format: "text".to_string(),
            functions: true,
            structs: true,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_search_empty_repo_map() {
        // An empty directory, not "." — `search` now indexes the path it is
        // given, and pointing a unit test at the process working directory
        // would scan whatever tree the test runner happens to sit in.
        let empty = TempDir::new().unwrap();
        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let search_args = SearchArgs {
            query: "test".to_string(),
            path: empty.path().to_path_buf(),
            r#type: "function".to_string(),
            limit: 10,
            fuzzy: false,
        };

        let result = app.search(search_args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_config_display() {
        let config = create_test_config();
        let app = CliApp::new(config, false, false).await.unwrap();

        let result = app.show_config().await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_convert_tool_result_reads_results_array() {
        // Regression test: search tools return an OBJECT with a `results`
        // array (not a bare array), and items carry `start_line` (not
        // `line_number`). This test would fail against the old code that read
        // `data.as_array()` / `line_number`.
        let config = create_test_config();
        let app = CliApp::new(config, false, false).await.unwrap();

        let tool_data = serde_json::json!({
            "status": "success",
            "pattern": "foo",
            "results": [
                {
                    "name": "foo_bar",
                    "file_path": "/src/foo.rs",
                    "start_line": 42,
                    "end_line": 50,
                    "parameters": [{"name": "x"}, {"name": "y"}],
                    "return_type": "bool"
                }
            ],
            "count": 1
        });

        let results = app.convert_tool_result_to_search_results(tool_data, "function");
        assert_eq!(results.len(), 1, "should read items from the results array");
        assert_eq!(results[0].file_path, "/src/foo.rs");
        assert_eq!(
            results[0].line,
            Some(42),
            "line should come from start_line"
        );
        assert!(results[0].content.contains("foo_bar"));
    }

    #[test]
    async fn test_exec_tool_scans_and_executes() {
        // exec-tool should scan the given path then run the named tool and succeed.
        let temp_dir = TempDir::new().unwrap();
        create_test_rust_file(
            &temp_dir,
            "sample.rs",
            "pub fn exec_target() -> i32 { 1 }\n",
        );

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let args = ExecToolArgs {
            tool: "search_functions".to_string(),
            params: r#"{"pattern":"exec_target"}"#.to_string(),
            path: temp_dir.path().to_path_buf(),
        };
        // Success path returns Ok (prints JSON to stdout; no process exit).
        assert!(app.exec_tool(args).await.is_ok());

        // The exec-tool scan populated the index; the tool found the function.
        let result = app
            .loregrep
            .execute_tool(
                "search_functions",
                serde_json::json!({"pattern": "exec_target"}),
            )
            .await
            .unwrap();
        assert!(result.success);
        let count = result
            .data
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(count >= 1, "expected exec-tool scan to index exec_target");
    }

    #[test]
    async fn exec_tool_leaves_the_analysed_tree_byte_for_byte_unchanged() {
        // K11: `exec-tool` is a read. It used to write
        // `<path>/.loregrep/index.cache` unconditionally — creating a directory
        // inside a repository the user only asked a question about, and
        // ignoring `cache.enabled` entirely.
        let repo = TempDir::new().unwrap();
        create_test_rust_file(
            &repo,
            "read_only.rs",
            "pub fn read_only_fn() -> i32 { 1 }\n",
        );
        let nested = repo.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("more.rs"), "pub fn more_fn() -> i32 { 2 }\n").unwrap();

        let before = tree_snapshot(repo.path());

        let cache_dir = TempDir::new().unwrap();
        let mut app = CliApp::new(create_test_config_with_cache(&cache_dir), false, false)
            .await
            .unwrap();
        let args = ExecToolArgs {
            tool: "search_functions".to_string(),
            params: r#"{"pattern":"read_only_fn"}"#.to_string(),
            path: repo.path().to_path_buf(),
        };
        assert!(app.exec_tool(args).await.is_ok());

        assert_eq!(
            tree_snapshot(repo.path()),
            before,
            "exec-tool must not modify the repository it analyses"
        );
        assert!(
            !repo.path().join(".loregrep").exists(),
            "no .loregrep directory may appear in the analysed tree"
        );
        // The index went to the user's own cache directory instead.
        assert!(
            LoreGrep::cache_path_for(cache_dir.path(), repo.path())
                .unwrap()
                .exists(),
            "the index cache belongs under the configured cache root"
        );
    }

    #[test]
    async fn exec_tool_honours_the_cache_disabled_setting() {
        // With caching off, nothing is written anywhere — neither in the
        // analysed tree nor in the cache root. `exec_tool` used to consult no
        // setting at all.
        let repo = TempDir::new().unwrap();
        create_test_rust_file(&repo, "nocache.rs", "pub fn nocache_fn() -> i32 { 1 }\n");
        let before = tree_snapshot(repo.path());

        let cache_dir = TempDir::new().unwrap();
        let mut config = create_test_config_with_cache(&cache_dir);
        config.cache.enabled = false;

        let mut app = CliApp::new(config, false, false).await.unwrap();
        let args = ExecToolArgs {
            tool: "search_functions".to_string(),
            params: r#"{"pattern":"nocache_fn"}"#.to_string(),
            path: repo.path().to_path_buf(),
        };
        assert!(app.exec_tool(args).await.is_ok());

        assert_eq!(tree_snapshot(repo.path()), before);
        assert!(
            !LoreGrep::cache_path_for(cache_dir.path(), repo.path())
                .unwrap()
                .exists(),
            "cache.enabled = false must mean no cache file"
        );
    }

    #[test]
    async fn search_works_without_a_prior_scan_in_the_same_process() {
        // K7: `search` opened with `if !is_scanned() { "run scan first" }`, and
        // since each process starts empty that branch was taken every time —
        // advice that could not be followed. It must obtain an index itself.
        let repo = TempDir::new().unwrap();
        create_test_rust_file(
            &repo,
            "searchable.rs",
            "pub fn searchable_fn() -> i32 { 5 }\n",
        );

        let cache_dir = TempDir::new().unwrap();
        let mut app = CliApp::new(create_test_config_with_cache(&cache_dir), false, false)
            .await
            .unwrap();
        assert!(!app.loregrep.is_scanned(), "a fresh process starts empty");

        let args = SearchArgs {
            query: "searchable_fn".to_string(),
            path: repo.path().to_path_buf(),
            r#type: "function".to_string(),
            limit: 10,
            fuzzy: false,
        };
        assert!(app.search(args).await.is_ok());

        assert!(
            app.loregrep.is_scanned(),
            "search must populate the index instead of telling the user to scan"
        );
        let result = app
            .loregrep
            .execute_tool(
                "search_functions",
                serde_json::json!({"pattern": "searchable_fn"}),
            )
            .await
            .unwrap();
        let count = result
            .data
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(count >= 1, "search should find the function it indexed");
    }

    #[test]
    async fn search_reuses_the_cache_exec_tool_wrote() {
        // The two commands share one load-or-scan path, so an index built by
        // either is usable by the other.
        let repo = TempDir::new().unwrap();
        create_test_rust_file(&repo, "shared.rs", "pub fn shared_fn() -> i32 { 3 }\n");

        let cache_dir = TempDir::new().unwrap();
        let cache_path = LoreGrep::cache_path_for(cache_dir.path(), repo.path()).unwrap();

        {
            let mut app = CliApp::new(create_test_config_with_cache(&cache_dir), false, false)
                .await
                .unwrap();
            let args = ExecToolArgs {
                tool: "search_functions".to_string(),
                params: r#"{"pattern":"shared_fn"}"#.to_string(),
                path: repo.path().to_path_buf(),
            };
            assert!(app.exec_tool(args).await.is_ok());
        }
        assert!(cache_path.exists());

        let mut app2 = CliApp::new(create_test_config_with_cache(&cache_dir), false, false)
            .await
            .unwrap();
        assert!(app2.loregrep.is_cache_fresh(&cache_path, repo.path()));

        let args = SearchArgs {
            query: "shared_fn".to_string(),
            path: repo.path().to_path_buf(),
            r#type: "all".to_string(),
            limit: 10,
            fuzzy: false,
        };
        assert!(app2.search(args).await.is_ok());
        assert!(app2.loregrep.is_scanned());
    }

    #[test]
    async fn test_exec_tool_persists_and_reuses_cache() {
        // First exec-tool invocation scans and writes a per-repo index cache;
        // a second, fresh CliApp on the same path finds that cache fresh and
        // loads it (proving repeated use does not require a rescan).
        let temp_dir = TempDir::new().unwrap();
        create_test_rust_file(
            &temp_dir,
            "cached.rs",
            "pub fn cached_target() -> i32 { 7 }\n",
        );

        let cache_dir = TempDir::new().unwrap();
        let cache_path = LoreGrep::cache_path_for(cache_dir.path(), temp_dir.path()).unwrap();
        assert!(
            !cache_path.exists(),
            "no cache should exist before first run"
        );

        // First invocation: scans, then persists the index.
        {
            let config = create_test_config_with_cache(&cache_dir);
            let mut app = CliApp::new(config, false, false).await.unwrap();
            let args = ExecToolArgs {
                tool: "search_functions".to_string(),
                params: r#"{"pattern":"cached_target"}"#.to_string(),
                path: temp_dir.path().to_path_buf(),
            };
            assert!(app.exec_tool(args).await.is_ok());
        }
        assert!(
            cache_path.exists(),
            "first exec-tool run should have written the index cache"
        );

        // Second invocation with a brand-new app: the cache is fresh and used.
        let config = create_test_config_with_cache(&cache_dir);
        let mut app2 = CliApp::new(config, false, false).await.unwrap();
        assert!(
            app2.loregrep.is_cache_fresh(&cache_path, temp_dir.path()),
            "the just-written cache should be considered fresh"
        );

        let args = ExecToolArgs {
            tool: "search_functions".to_string(),
            params: r#"{"pattern":"cached_target"}"#.to_string(),
            path: temp_dir.path().to_path_buf(),
        };
        assert!(app2.exec_tool(args).await.is_ok());

        // The cache-loaded index resolves the function (loaded, not empty).
        assert!(app2.loregrep.is_scanned());
        let result = app2
            .loregrep
            .execute_tool(
                "search_functions",
                serde_json::json!({"pattern": "cached_target"}),
            )
            .await
            .unwrap();
        assert!(result.success);
        let count = result
            .data
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            count >= 1,
            "cache-loaded index should contain cached_target"
        );
    }

    #[test]
    async fn test_exec_tool_cache_invalidated_on_file_deletion() {
        // Regression: mtime-based freshness cannot see a *deleted* source file
        // (removing it makes no remaining file newer than the cache), so a naive
        // cache would keep returning the deleted file's symbols. exec_tool must
        // detect the missing indexed path, discard the cache, and rescan.
        let temp_dir = TempDir::new().unwrap();
        create_test_rust_file(&temp_dir, "keep.rs", "pub fn alpha_keep() -> i32 { 1 }\n");
        let doomed = create_test_rust_file(
            &temp_dir,
            "doomed.rs",
            "pub fn beta_doomed() -> i32 { 2 }\n",
        );

        let cache_dir = TempDir::new().unwrap();
        let cache_path = LoreGrep::cache_path_for(cache_dir.path(), temp_dir.path()).unwrap();

        // First run: scan both files and persist the cache.
        {
            let mut app = CliApp::new(create_test_config_with_cache(&cache_dir), false, false)
                .await
                .unwrap();
            let args = ExecToolArgs {
                tool: "search_functions".to_string(),
                params: r#"{"pattern":"beta_doomed"}"#.to_string(),
                path: temp_dir.path().to_path_buf(),
            };
            assert!(app.exec_tool(args).await.is_ok());
        }
        assert!(cache_path.exists(), "first run should write the cache");

        // Delete one indexed file. Its removal does NOT make any surviving file
        // newer than the cache, so the old max(mtime) gate reported "fresh"
        // forever. Comparing the indexed path set against what is on disk sees
        // it immediately.
        fs::remove_file(&doomed).unwrap();

        let mut app2 = CliApp::new(create_test_config_with_cache(&cache_dir), false, false)
            .await
            .unwrap();
        assert!(
            !app2.loregrep.is_cache_fresh(&cache_path, temp_dir.path()),
            "a deleted indexed file MUST make the cache stale"
        );

        // Second run: the cache is rejected and the path rescanned, so the
        // deleted file's symbols are gone.
        let args = ExecToolArgs {
            tool: "search_functions".to_string(),
            params: r#"{"pattern":"beta_doomed"}"#.to_string(),
            path: temp_dir.path().to_path_buf(),
        };
        assert!(app2.exec_tool(args).await.is_ok());

        let beta = app2
            .loregrep
            .execute_tool(
                "search_functions",
                serde_json::json!({"pattern": "beta_doomed"}),
            )
            .await
            .unwrap();
        let beta_count = beta.data.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(
            beta_count, 0,
            "deleted file's symbols must not survive in the rescanned index"
        );

        // Sanity: the surviving file is still indexed.
        let alpha = app2
            .loregrep
            .execute_tool(
                "search_functions",
                serde_json::json!({"pattern": "alpha_keep"}),
            )
            .await
            .unwrap();
        let alpha_count = alpha
            .data
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(alpha_count >= 1, "surviving file should remain indexed");
    }

    #[test]
    async fn test_freshness_ignores_files_in_excluded_dirs() {
        // Regression: the freshness check must consider the SAME files the
        // scanner indexes. A regenerated file under an excluded directory must
        // NOT mark the cache stale (otherwise the cache never helps), while a
        // real edit to an indexed file MUST.
        let temp_dir = TempDir::new().unwrap();
        create_test_rust_file(&temp_dir, "main.rs", "pub fn indexed_fn() -> i32 { 1 }\n");

        // A file inside an excluded directory: it is not indexed, so it must not
        // influence freshness.
        let excluded_dir = temp_dir.path().join("excluded");
        fs::create_dir(&excluded_dir).unwrap();
        let excluded_file = excluded_dir.join("generated.rs");
        fs::write(&excluded_file, "pub fn excluded_fn() -> i32 { 2 }\n").unwrap();

        // Configure the scanner to exclude that directory (mirrors how config
        // exclude_patterns / gitignore drop build artifacts).
        let cache_dir = TempDir::new().unwrap();
        let mut config = create_test_config_with_cache(&cache_dir);
        config
            .file_scanning
            .exclude_patterns
            .push("**/excluded/**".to_string());

        let cache_path = LoreGrep::cache_path_for(cache_dir.path(), temp_dir.path()).unwrap();
        {
            let mut app = CliApp::new(config.clone(), false, false).await.unwrap();
            let args = ExecToolArgs {
                tool: "search_functions".to_string(),
                params: r#"{"pattern":"indexed_fn"}"#.to_string(),
                path: temp_dir.path().to_path_buf(),
            };
            assert!(app.exec_tool(args).await.is_ok());
        }
        assert!(cache_path.exists(), "first run should write the cache");

        let app2 = CliApp::new(config.clone(), false, false).await.unwrap();

        // Touch the excluded file so it is strictly newer than the cache. Since
        // it is excluded from indexing, the cache must still be considered fresh.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&excluded_file, "pub fn excluded_fn() -> i32 { 3 }\n").unwrap();
        assert!(
            app2.loregrep.is_cache_fresh(&cache_path, temp_dir.path()),
            "a change under an excluded dir must NOT make the cache stale"
        );

        // Control: editing the indexed file MUST invalidate the cache.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            temp_dir.path().join("main.rs"),
            "pub fn indexed_fn() -> i32 { 42 }\n",
        )
        .unwrap();
        assert!(
            !app2.loregrep.is_cache_fresh(&cache_path, temp_dir.path()),
            "editing an indexed file MUST make the cache stale"
        );
    }

    #[tokio::test]
    async fn test_analyze_directory_produces_output() {
        // Regression test for the `analyze <directory>` path, which used to call
        // a nonexistent `analyze_directory` tool. It now routes to
        // `get_repository_tree`; verify the whole path succeeds and that the
        // returned tree contains the scanned file with its symbols.
        let temp_dir = TempDir::new().unwrap();
        let rust_content = r#"
pub fn dir_function() -> i32 {
    7
}

pub struct DirStruct {
    pub field: String,
}
"#;
        create_test_rust_file(&temp_dir, "sample.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        // Do NOT scan manually here: the `analyze <directory>` path must scan
        // the directory itself before reading the repository tree. Without that,
        // a real run would print an empty tree.
        let analyze_args = AnalyzeArgs {
            file: temp_dir.path().to_path_buf(),
            format: "text".to_string(),
            functions: true,
            structs: true,
            imports: false,
        };
        assert!(app.analyze(analyze_args).await.is_ok());

        // The analyze path itself populated the index.
        assert!(
            app.loregrep.is_scanned(),
            "analyze <directory> should have scanned the directory"
        );

        // Exercise the underlying tool call and assert non-empty output.
        let tool_result = app
            .loregrep
            .execute_tool(
                "get_repository_tree",
                serde_json::json!({
                    "include_file_details": true,
                    "max_depth": 0
                }),
            )
            .await
            .unwrap();
        assert!(tool_result.success);

        let tree = tool_result
            .data
            .get("repository_tree")
            .expect("repository_tree present");
        let mut files = Vec::new();
        CliApp::collect_tree_files(tree, &mut files);
        assert!(
            !files.is_empty(),
            "directory analysis should surface at least one file"
        );

        // The scanned file should expose its function and struct.
        let has_symbols = files.iter().any(|f| {
            let skeleton = f.get("skeleton");
            let funcs = skeleton
                .and_then(|s| s.get("functions"))
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let structs = skeleton
                .and_then(|s| s.get("structs"))
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            funcs || structs
        });
        assert!(
            has_symbols,
            "scanned file should carry functions/structs in its skeleton"
        );
    }

    #[test]
    async fn test_analyze_nonexistent_file() {
        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: std::path::PathBuf::from("nonexistent.rs"),
            format: "text".to_string(),
            functions: false,
            structs: false,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok()); // Should handle gracefully
    }

    #[test]
    async fn test_analyze_json_format() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = "pub fn simple() {}";
        let file_path = create_test_rust_file(&temp_dir, "simple.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: file_path,
            format: "json".to_string(),
            functions: false,
            structs: false,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_analyze_file_json_includes_content() {
        // Regression: the single-file analyze branch passed `include_source`,
        // but the analyze_file tool reads `include_content`, so source was never
        // included. Verify the corrected key surfaces `content`, and that the
        // old (wrong) key does not.
        let temp_dir = TempDir::new().unwrap();
        let rust_content = "pub fn simple() -> i32 { 1 }";
        let file_path = create_test_rust_file(&temp_dir, "simple.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();
        // Scan first: analyze_file resolves against the analysis root.
        app.loregrep
            .scan(temp_dir.path().to_str().unwrap())
            .await
            .unwrap();

        // Corrected key used by `analyze <file>` -> content present.
        let with_content = app
            .loregrep
            .execute_tool(
                "analyze_file",
                serde_json::json!({
                    "file_path": file_path.to_string_lossy(),
                    "include_content": true
                }),
            )
            .await
            .unwrap();
        assert!(with_content.success);
        assert_eq!(
            with_content.data.get("content").and_then(|v| v.as_str()),
            Some(rust_content),
            "include_content: true should surface the file content"
        );

        // Old/wrong key is ignored by the tool -> no content field.
        let wrong_key = app
            .loregrep
            .execute_tool(
                "analyze_file",
                serde_json::json!({
                    "file_path": file_path.to_string_lossy(),
                    "include_source": true
                }),
            )
            .await
            .unwrap();
        assert!(wrong_key.success);
        assert!(
            wrong_key.data.get("content").is_none(),
            "the wrong key must not surface content"
        );
    }

    #[test]
    async fn test_analyze_tree_format() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = "pub fn simple() {}";
        let file_path = create_test_rust_file(&temp_dir, "simple.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: file_path,
            format: "tree".to_string(),
            functions: false,
            structs: false,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok());
    }
}
