# Repository Scanner & File Discovery

## Overview
High-performance repository scanning system with intelligent file discovery, incremental updates, and parallel processing capabilities for large codebases.

## Core Architecture

### Scanner Components
```rust
pub struct RepositoryScanner {
    file_discovery: FileDiscovery,
    content_hasher: ContentHasher,
    change_detector: ChangeDetector,
    parallel_processor: ParallelProcessor,
    filter_engine: FilterEngine,
    progress_tracker: ProgressTracker,
    database: Arc<DatabaseManager>,
    config: ScannerConfig,
}

pub struct ScannerConfig {
    pub max_file_size_bytes: usize,
    pub max_files_per_scan: usize,
    pub parallel_workers: usize,
    pub chunk_size: usize,
    pub timeout_seconds: u64,
    pub include_hidden_files: bool,
    pub follow_symlinks: bool,
    pub respect_gitignore: bool,
    pub incremental_mode: bool,
}
```

## File Discovery System

### Discovery Engine
```rust
pub struct FileDiscovery {
    walker: WalkDir,
    filters: FilterChain,
    ignore_engine: Option<IgnoreEngine>,
    language_detector: LanguageDetector,
}

impl FileDiscovery {
    pub fn scan_directory(&self, path: &Path, config: &ScanConfig) -> Result<Vec<FileEntry>> {
        let mut files = Vec::new();
        let mut walker = WalkDir::new(path)
            .follow_links(config.follow_symlinks)
            .max_depth(config.max_depth.unwrap_or(usize::MAX));
            
        if !config.include_hidden {
            walker = walker.filter_entry(|entry| {
                !entry.file_name().to_str().unwrap_or("").starts_with('.')
            });
        }
        
        for entry in walker {
            let entry = entry?;
            let file_entry = self.process_file_entry(entry, config)?;
            
            if let Some(file_entry) = file_entry {
                files.push(file_entry);
            }
        }
        
        Ok(files)
    }
    
    fn process_file_entry(&self, entry: DirEntry, config: &ScanConfig) -> Result<Option<FileEntry>> {
        let path = entry.path();
        
        // Skip directories
        if path.is_dir() {
            return Ok(None);
        }
        
        // Apply size filter
        if let Ok(metadata) = entry.metadata() {
            if metadata.len() > config.max_file_size as u64 {
                return Ok(None);
            }
        }
        
        // Apply include/exclude patterns
        if !self.filters.should_include(path)? {
            return Ok(None);
        }
        
        // Detect language
        let language = self.language_detector.detect_language(path)?;
        if language.is_none() && !config.include_unknown_files {
            return Ok(None);
        }
        
        Ok(Some(FileEntry {
            path: path.to_path_buf(),
            language,
            size: entry.metadata()?.len(),
            last_modified: entry.metadata()?.modified()?,
        }))
    }
}

pub struct FileEntry {
    pub path: PathBuf,
    pub language: Option<String>,
    pub size: u64,
    pub last_modified: SystemTime,
}
```

### Language Detection
```rust
pub struct LanguageDetector {
    extension_map: HashMap<String, String>,
    content_patterns: HashMap<String, Vec<Regex>>,
    shebang_patterns: HashMap<Regex, String>,
}

impl LanguageDetector {
    pub fn detect_language(&self, file_path: &Path) -> Result<Option<String>> {
        // 1. Try extension-based detection first (fastest)
        if let Some(extension) = file_path.extension().and_then(|e| e.to_str()) {
            if let Some(language) = self.extension_map.get(extension) {
                return Ok(Some(language.clone()));
            }
        }
        
        // 2. Check for special filenames
        if let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) {
            if let Some(language) = self.detect_by_filename(filename) {
                return Ok(Some(language));
            }
        }
        
        // 3. Content-based detection for ambiguous cases
        if file_path.metadata()?.len() < 1024 * 1024 { // Only for files < 1MB
            if let Some(language) = self.detect_by_content(file_path)? {
                return Ok(Some(language));
            }
        }
        
        Ok(None)
    }
    
    fn detect_by_filename(&self, filename: &str) -> Option<String> {
        match filename {
            "Cargo.toml" | "Cargo.lock" => Some("toml".to_string()),
            "package.json" | "tsconfig.json" => Some("json".to_string()),
            "Dockerfile" => Some("dockerfile".to_string()),
            "Makefile" | "makefile" => Some("makefile".to_string()),
            "CMakeLists.txt" => Some("cmake".to_string()),
            _ => None,
        }
    }
    
    fn detect_by_content(&self, file_path: &Path) -> Result<Option<String>> {
        let mut file = File::open(file_path)?;
        let mut buffer = [0; 512]; // Read first 512 bytes
        let bytes_read = file.read(&mut buffer)?;
        let content = String::from_utf8_lossy(&buffer[..bytes_read]);
        
        // Check shebang
        if content.starts_with("#!") {
            for (pattern, language) in &self.shebang_patterns {
                if pattern.is_match(&content) {
                    return Ok(Some(language.clone()));
                }
            }
        }
        
        // Check content patterns
        for (language, patterns) in &self.content_patterns {
            for pattern in patterns {
                if pattern.is_match(&content) {
                    return Ok(Some(language.clone()));
                }
            }
        }
        
        Ok(None)
    }
}
```

## Filter Engine

### Pattern Matching
```rust
pub struct FilterEngine {
    include_patterns: Vec<Glob>,
    exclude_patterns: Vec<Glob>,
    language_filters: HashSet<String>,
    size_limits: SizeLimits,
    gitignore_enabled: bool,
    ignore_matcher: Option<gitignore::GitignoreMatcher>,
}

pub struct SizeLimits {
    pub min_size_bytes: u64,
    pub max_size_bytes: u64,
}

impl FilterEngine {
    pub fn should_include(&self, file_path: &Path) -> Result<bool> {
        // Check gitignore first (if enabled)
        if self.gitignore_enabled {
            if let Some(ref matcher) = self.ignore_matcher {
                if matcher.is_ignored(file_path)? {
                    return Ok(false);
                }
            }
        }
        
        // Check exclude patterns
        for pattern in &self.exclude_patterns {
            if pattern.matches_path(file_path) {
                return Ok(false);
            }
        }
        
        // Check include patterns (if any specified)
        if !self.include_patterns.is_empty() {
            let mut included = false;
            for pattern in &self.include_patterns {
                if pattern.matches_path(file_path) {
                    included = true;
                    break;
                }
            }
            if !included {
                return Ok(false);
            }
        }
        
        // Check file size
        if let Ok(metadata) = file_path.metadata() {
            let size = metadata.len();
            if size < self.size_limits.min_size_bytes || size > self.size_limits.max_size_bytes {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    pub fn update_gitignore(&mut self, repo_root: &Path) -> Result<()> {
        if self.gitignore_enabled {
            self.ignore_matcher = Some(gitignore::GitignoreMatcher::new(repo_root)?);
        }
        Ok(())
    }
}

pub struct Glob {
    pattern: String,
    matcher: globset::GlobMatcher,
}

impl Glob {
    pub fn new(pattern: &str) -> Result<Self> {
        let glob = globset::Glob::new(pattern)?;
        Ok(Self {
            pattern: pattern.to_string(),
            matcher: glob.compile_matcher(),
        })
    }
    
    pub fn matches_path(&self, path: &Path) -> bool {
        self.matcher.is_match(path)
    }
}
```

## Content Hashing & Change Detection

### Hashing System
```rust
pub struct ContentHasher {
    hasher_type: HashType,
    cache: HashMap<PathBuf, (SystemTime, String)>,
    cache_size_limit: usize,
}

pub enum HashType {
    Sha256,
    Blake3,
    XxHash,
}

impl ContentHasher {
    pub fn calculate_hash(&mut self, file_path: &Path) -> Result<String> {
        let metadata = file_path.metadata()?;
        let modified_time = metadata.modified()?;
        
        // Check cache first
        if let Some((cached_time, cached_hash)) = self.cache.get(file_path) {
            if *cached_time == modified_time {
                return Ok(cached_hash.clone());
            }
        }
        
        // Calculate new hash
        let hash = self.hash_file_content(file_path)?;
        
        // Update cache
        if self.cache.len() >= self.cache_size_limit {
            self.evict_oldest_cache_entry();
        }
        self.cache.insert(file_path.to_path_buf(), (modified_time, hash.clone()));
        
        Ok(hash)
    }
    
    fn hash_file_content(&self, file_path: &Path) -> Result<String> {
        let mut file = File::open(file_path)?;
        
        match self.hasher_type {
            HashType::Sha256 => {
                let mut hasher = Sha256::new();
                io::copy(&mut file, &mut hasher)?;
                Ok(format!("{:x}", hasher.finalize()))
            }
            HashType::Blake3 => {
                let mut hasher = blake3::Hasher::new();
                io::copy(&mut file, &mut hasher)?;
                Ok(hasher.finalize().to_hex().to_string())
            }
            HashType::XxHash => {
                let mut hasher = XxHash64::default();
                let mut buffer = [0; 8192];
                loop {
                    let bytes_read = file.read(&mut buffer)?;
                    if bytes_read == 0 {
                        break;
                    }
                    hasher.write(&buffer[..bytes_read]);
                }
                Ok(format!("{:x}", hasher.finish()))
            }
        }
    }
}
```

### Change Detection
```rust
pub struct ChangeDetector {
    database: Arc<DatabaseManager>,
    hasher: ContentHasher,
}

impl ChangeDetector {
    pub fn detect_changes(&mut self, repo_id: i64, discovered_files: &[FileEntry]) -> Result<ChangeSet> {
        let mut changes = ChangeSet::new();
        
        // Get existing files from database
        let existing_files = self.database.get_repository_files(repo_id)?;
        let existing_paths: HashSet<PathBuf> = existing_files
            .iter()
            .map(|f| f.path.clone())
            .collect();
            
        let discovered_paths: HashSet<PathBuf> = discovered_files
            .iter()
            .map(|f| f.path.clone())
            .collect();
        
        // Find deleted files
        for existing_file in &existing_files {
            if !discovered_paths.contains(&existing_file.path) {
                changes.deleted.push(existing_file.clone());
            }
        }
        
        // Find new and modified files
        for discovered_file in discovered_files {
            if let Some(existing_file) = existing_files
                .iter()
                .find(|f| f.path == discovered_file.path) {
                
                // Check if file was modified
                if discovered_file.last_modified != existing_file.last_modified {
                    let new_hash = self.hasher.calculate_hash(&discovered_file.path)?;
                    if new_hash != existing_file.content_hash {
                        changes.modified.push(FileChange {
                            old_file: existing_file.clone(),
                            new_file: discovered_file.clone(),
                            new_hash,
                        });
                    }
                }
            } else {
                // New file
                let hash = self.hasher.calculate_hash(&discovered_file.path)?;
                changes.added.push(FileAddition {
                    file: discovered_file.clone(),
                    hash,
                });
            }
        }
        
        Ok(changes)
    }
}

pub struct ChangeSet {
    pub added: Vec<FileAddition>,
    pub modified: Vec<FileChange>,
    pub deleted: Vec<ExistingFile>,
}

pub struct FileAddition {
    pub file: FileEntry,
    pub hash: String,
}

pub struct FileChange {
    pub old_file: ExistingFile,
    pub new_file: FileEntry,
    pub new_hash: String,
}
```

## Parallel Processing

### Worker Pool
```rust
pub struct ParallelProcessor {
    worker_pool: ThreadPool,
    chunk_size: usize,
    progress_sender: mpsc::Sender<ProgressUpdate>,
}

impl ParallelProcessor {
    pub fn process_files<F, R>(&self, files: Vec<FileEntry>, processor: F) -> Result<Vec<R>>
    where
        F: Fn(FileEntry) -> Result<R> + Send + Sync + 'static,
        R: Send + 'static,
    {
        let (result_sender, result_receiver) = mpsc::channel();
        let processor = Arc::new(processor);
        let total_files = files.len();
        
        // Split files into chunks for parallel processing
        let chunks: Vec<Vec<FileEntry>> = files
            .chunks(self.chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect();
            
        let completed_chunks = Arc::new(AtomicUsize::new(0));
        
        for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
            let result_sender = result_sender.clone();
            let processor = processor.clone();
            let progress_sender = self.progress_sender.clone();
            let completed_chunks = completed_chunks.clone();
            
            self.worker_pool.execute(move || {
                let mut chunk_results = Vec::new();
                
                for (file_idx, file) in chunk.into_iter().enumerate() {
                    match processor(file) {
                        Ok(result) => chunk_results.push(Ok(result)),
                        Err(e) => chunk_results.push(Err(e)),
                    }
                    
                    // Send progress update
                    let _ = progress_sender.send(ProgressUpdate {
                        chunk_idx,
                        file_idx,
                        total_files,
                    });
                }
                
                // Send chunk results
                let _ = result_sender.send(ChunkResult {
                    chunk_idx,
                    results: chunk_results,
                });
                
                completed_chunks.fetch_add(1, Ordering::Relaxed);
            });
        }
        
        // Collect results
        drop(result_sender); // Close sender to signal completion
        let mut all_results = Vec::new();
        
        while let Ok(chunk_result) = result_receiver.recv() {
            for result in chunk_result.results {
                all_results.push(result?);
            }
        }
        
        Ok(all_results)
    }
}

pub struct ProgressUpdate {
    pub chunk_idx: usize,
    pub file_idx: usize,
    pub total_files: usize,
}

pub struct ChunkResult<R> {
    pub chunk_idx: usize,
    pub results: Vec<Result<R>>,
}
```

## Progress Tracking

### Progress Reporter
```rust
pub struct ProgressTracker {
    start_time: Instant,
    total_files: usize,
    processed_files: AtomicUsize,
    failed_files: AtomicUsize,
    current_file: Mutex<Option<String>>,
    update_interval: Duration,
    last_update: Mutex<Instant>,
}

impl ProgressTracker {
    pub fn new(total_files: usize) -> Self {
        Self {
            start_time: Instant::now(),
            total_files,
            processed_files: AtomicUsize::new(0),
            failed_files: AtomicUsize::new(0),
            current_file: Mutex::new(None),
            update_interval: Duration::from_millis(500),
            last_update: Mutex::new(Instant::now()),
        }
    }
    
    pub fn update_progress(&self, file_path: &str, success: bool) {
        self.processed_files.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.failed_files.fetch_add(1, Ordering::Relaxed);
        }
        
        {
            let mut current = self.current_file.lock().unwrap();
            *current = Some(file_path.to_string());
        }
        
        // Rate-limited progress output
        let mut last_update = self.last_update.lock().unwrap();
        if last_update.elapsed() >= self.update_interval {
            self.print_progress();
            *last_update = Instant::now();
        }
    }
    
    fn print_progress(&self) {
        let processed = self.processed_files.load(Ordering::Relaxed);
        let failed = self.failed_files.load(Ordering::Relaxed);
        let percentage = (processed as f64 / self.total_files as f64 * 100.0).round() as u32;
        let elapsed = self.start_time.elapsed();
        
        let current_file = self.current_file.lock().unwrap();
        let current_display = current_file
            .as_ref()
            .map(|p| format!(" | {}", p))
            .unwrap_or_default();
            
        println!(
            "\r🔍 Progress: {}/{} ({}%) | Failed: {} | Elapsed: {:.1}s{}",
            processed,
            self.total_files,
            percentage,
            failed,
            elapsed.as_secs_f64(),
            current_display
        );
    }
    
    pub fn finish(&self) -> ScanSummary {
        let processed = self.processed_files.load(Ordering::Relaxed);
        let failed = self.failed_files.load(Ordering::Relaxed);
        let duration = self.start_time.elapsed();
        
        ScanSummary {
            total_files: self.total_files,
            processed_files: processed,
            failed_files: failed,
            duration,
        }
    }
}

pub struct ScanSummary {
    pub total_files: usize,
    pub processed_files: usize,
    pub failed_files: usize,
    pub duration: Duration,
}
```

## Incremental Updates

### Update Strategy
```rust
pub struct IncrementalUpdater {
    database: Arc<DatabaseManager>,
    change_detector: ChangeDetector,
    analyzer: Arc<CodeAnalyzer>,
}

impl IncrementalUpdater {
    pub fn update_repository(&mut self, repo_id: i64, scan_config: &ScanConfig) -> Result<UpdateResult> {
        let start_time = Instant::now();
        
        // Get last scan time from database
        let last_scan = self.database.get_last_scan_time(repo_id)?;
        
        // Discover files that may have changed
        let discovered_files = if let Some(last_scan_time) = last_scan {
            self.discover_changed_files_since(repo_id, last_scan_time, scan_config)?
        } else {
            // Full scan if no previous scan
            return self.full_repository_scan(repo_id, scan_config);
        };
        
        // Detect actual changes
        let changes = self.change_detector.detect_changes(repo_id, &discovered_files)?;
        
        let mut update_stats = UpdateStats::default();
        
        // Process deletions
        for deleted_file in changes.deleted {
            self.database.delete_file_data(deleted_file.id)?;
            update_stats.deleted_files += 1;
        }
        
        // Process additions
        for addition in changes.added {
            let analysis = self.analyzer.analyze_file(&addition.file.path)?;
            let file_id = self.database.insert_file_with_analysis(
                repo_id,
                &addition.file,
                &addition.hash,
                &analysis
            )?;
            update_stats.added_files += 1;
            update_stats.added_functions += analysis.functions.len();
            update_stats.added_structs += analysis.structs.len();
        }
        
        // Process modifications
        for modification in changes.modified {
            let analysis = self.analyzer.analyze_file(&modification.new_file.path)?;
            self.database.update_file_with_analysis(
                modification.old_file.id,
                &modification.new_file,
                &modification.new_hash,
                &analysis
            )?;
            update_stats.modified_files += 1;
        }
        
        // Update repository scan timestamp
        self.database.update_repository_scan_time(repo_id, Utc::now())?;
        
        Ok(UpdateResult {
            stats: update_stats,
            duration: start_time.elapsed(),
            incremental: true,
        })
    }
    
    fn discover_changed_files_since(
        &self,
        repo_id: i64,
        since: DateTime<Utc>,
        config: &ScanConfig
    ) -> Result<Vec<FileEntry>> {
        let repo_path = self.database.get_repository_path(repo_id)?;
        let mut discovered_files = Vec::new();
        
        // Use filesystem's last modified time for efficient discovery
        for entry in WalkDir::new(&repo_path)
            .follow_links(config.follow_symlinks)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let metadata = entry.metadata()?;
            let modified_time = metadata.modified()?;
            
            // Only include files modified after last scan
            if DateTime::<Utc>::from(modified_time) > since {
                if let Some(file_entry) = self.process_discovered_file(entry, config)? {
                    discovered_files.push(file_entry);
                }
            }
        }
        
        Ok(discovered_files)
    }
}

pub struct UpdateStats {
    pub added_files: usize,
    pub modified_files: usize,
    pub deleted_files: usize,
    pub added_functions: usize,
    pub added_structs: usize,
}

pub struct UpdateResult {
    pub stats: UpdateStats,
    pub duration: Duration,
    pub incremental: bool,
}
```

## Configuration & Performance

### Scanner Configuration
```toml
[scanner]
max_file_size_mb = 10
max_files_per_scan = 10000
parallel_workers = 4
chunk_size = 50
timeout_seconds = 300
include_hidden_files = false
follow_symlinks = false
respect_gitignore = true
incremental_mode = true

[filters]
include_patterns = [
    "**/*.rs", "**/*.py", "**/*.ts", "**/*.js", "**/*.go",
    "**/*.java", "**/*.cpp", "**/*.c", "**/*.h"
]
exclude_patterns = [
    "**/node_modules/**", "**/target/**", "**/.git/**",
    "**/build/**", "**/dist/**", "**/__pycache__/**",
    "**/*.min.js", "**/*.bundle.js"
]

[performance]
hash_algorithm = "blake3"  # "sha256", "blake3", "xxhash"
content_hash_cache_size = 10000
file_discovery_buffer_size = 8192
progress_update_interval_ms = 500
```

## Error Handling & Recovery

### Robust Error Handling
```rust
#[derive(Debug, thiserror::Error)]
pub enum ScannerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },
    
    #[error("File too large: {path} ({size} bytes)")]
    FileTooLarge { path: String, size: u64 },
    
    #[error("Timeout during scan")]
    Timeout,
    
    #[error("Scan aborted by user")]
    Aborted,
    
    #[error("Invalid pattern: {pattern}")]
    InvalidPattern { pattern: String },
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl RepositoryScanner {
    pub fn scan_with_recovery(&mut self, config: &ScanConfig) -> Result<ScanResult> {
        let mut retry_count = 0;
        const MAX_RETRIES: usize = 3;
        
        loop {
            match self.scan_repository(config) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= MAX_RETRIES {
                        return Err(e);
                    }
                    
                    // Exponential backoff
                    std::thread::sleep(Duration::from_millis(100 * (1 << retry_count)));
                    
                    // Try to recover from specific errors
                    match &e {
                        ScannerError::PermissionDenied { .. } => {
                            // Skip problematic file and continue
                            continue;
                        }
                        ScannerError::FileTooLarge { .. } => {
                            // Increase size limit or skip
                            continue;
                        }
                        _ => {
                            // For other errors, propagate after retries
                            if retry_count >= MAX_RETRIES {
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
    }
}
```

## Testing

### Performance Benchmarks
- Large repository scanning (Linux kernel, Chromium)
- Incremental update performance
- Memory usage profiling
- Concurrent scanning stress tests

### Unit Tests
- File discovery accuracy
- Pattern matching correctness
- Change detection precision
- Hash calculation consistency

### Integration Tests
- End-to-end scanning workflows
- Database integration
- Error recovery scenarios
- Progress tracking accuracy 