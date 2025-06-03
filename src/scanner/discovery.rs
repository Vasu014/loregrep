use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{Walk, WalkBuilder};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

use crate::internal::config::FileScanningConfig;

#[derive(Clone)]
pub struct FileFilters {
    include_globs: GlobSet,
    exclude_globs: GlobSet,
    max_file_size: u64,
}

#[derive(Clone)]
pub struct LanguageDetector {
    rust_extensions: GlobSet,
    python_extensions: GlobSet,
    typescript_extensions: GlobSet,
    javascript_extensions: GlobSet,
    go_extensions: GlobSet,
}

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub follow_symlinks: bool,
    pub max_depth: Option<u32>,
    pub show_progress: bool,
    pub parallel: bool,
}

pub struct ScanResult {
    pub files: Vec<DiscoveredFile>,
    pub total_files_found: usize,
    pub total_files_filtered: usize,
    pub scan_duration: std::time::Duration,
    pub languages_found: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub language: String,
    pub size: u64,
    pub relative_path: PathBuf,
}

#[derive(Clone)]
pub struct RepositoryScanner {
    filters: FileFilters,
    language_detector: LanguageDetector,
    config: ScanConfig,
}

impl FileFilters {
    pub fn new(config: &FileScanningConfig) -> Result<Self> {
        let mut include_builder = GlobSetBuilder::new();
        for pattern in &config.include_patterns {
            let glob = Glob::new(pattern)
                .with_context(|| format!("Invalid include pattern: {}", pattern))?;
            include_builder.add(glob);
        }

        let mut exclude_builder = GlobSetBuilder::new();
        for pattern in &config.exclude_patterns {
            let glob = Glob::new(pattern)
                .with_context(|| format!("Invalid exclude pattern: {}", pattern))?;
            exclude_builder.add(glob);
        }

        Ok(Self {
            include_globs: include_builder.build()?,
            exclude_globs: exclude_builder.build()?,
            max_file_size: config.max_file_size,
        })
    }

    pub fn should_include(&self, path: &Path, size: u64) -> bool {
        // Check file size first (quick check)
        if size > self.max_file_size {
            return false;
        }

        let path_str = path.to_string_lossy().to_string();

        // If explicitly excluded, reject
        if self.exclude_globs.is_match(&path_str) {
            return false;
        }

        // If include patterns are specified, file must match at least one
        if self.include_globs.len() > 0 {
            self.include_globs.is_match(&path_str)
        } else {
            // No include patterns means include everything (except excluded)
            true
        }
    }
}

impl LanguageDetector {
    pub fn new() -> Result<Self> {
        let rust_globs = Self::build_globset(&["*.rs"])?;
        let python_globs = Self::build_globset(&["*.py", "*.pyi"])?;
        let typescript_globs = Self::build_globset(&["*.ts", "*.tsx"])?;
        let javascript_globs = Self::build_globset(&["*.js", "*.jsx", "*.mjs", "*.cjs"])?;
        let go_globs = Self::build_globset(&["*.go"])?;

        Ok(Self {
            rust_extensions: rust_globs,
            python_extensions: python_globs,
            typescript_extensions: typescript_globs,
            javascript_extensions: javascript_globs,
            go_extensions: go_globs,
        })
    }

    fn build_globset(patterns: &[&str]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            builder.add(Glob::new(pattern)?);
        }
        Ok(builder.build()?)
    }

    pub fn detect_language(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy().to_string();

        if self.rust_extensions.is_match(&path_str) {
            "rust".to_string()
        } else if self.python_extensions.is_match(&path_str) {
            "python".to_string()
        } else if self.typescript_extensions.is_match(&path_str) {
            "typescript".to_string()
        } else if self.javascript_extensions.is_match(&path_str) {
            "javascript".to_string()
        } else if self.go_extensions.is_match(&path_str) {
            "go".to_string()
        } else {
            "unknown".to_string()
        }
    }
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            max_depth: Some(20),
            show_progress: true,
            parallel: true,
        }
    }
}

impl RepositoryScanner {
    pub fn new(
        scanning_config: &FileScanningConfig,
        scan_config: Option<ScanConfig>,
    ) -> Result<Self> {
        let filters = FileFilters::new(scanning_config)?;
        let language_detector = LanguageDetector::new()?;
        let config = scan_config.unwrap_or_default();

        Ok(Self {
            filters,
            language_detector,
            config,
        })
    }

    pub fn scan<P: AsRef<Path>>(&self, root_path: P) -> Result<ScanResult> {
        let start_time = Instant::now();
        let root_path = root_path.as_ref();

        info!("Starting repository scan at: {:?}", root_path);

        // Create progress bar
        let progress = if self.config.show_progress {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} [{elapsed_precise}] {msg} ({pos} files)")
                    .unwrap(),
            );
            pb.set_message("Scanning files...");
            Some(pb)
        } else {
            None
        };

        // Build the walker
        let walker = self.build_walker(root_path)?;

        // Track statistics
        let total_found = Arc::new(AtomicUsize::new(0));
        let total_filtered = Arc::new(AtomicUsize::new(0));

        // Collect files
        let mut discovered_files = Vec::new();
        let mut languages_found = std::collections::HashMap::new();

        for result in walker {
            match result {
                Ok(entry) => {
                    total_found.fetch_add(1, Ordering::Relaxed);

                    if let Some(pb) = &progress {
                        pb.set_position(total_found.load(Ordering::Relaxed) as u64);
                    }

                    // Skip directories
                    if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                        continue;
                    }

                    let path = entry.path();
                    
                    // Get file size
                    let metadata = match entry.metadata() {
                        Ok(meta) => meta,
                        Err(e) => {
                            warn!("Failed to get metadata for {:?}: {}", path, e);
                            continue;
                        }
                    };

                    let file_size = metadata.len();

                    // Apply filters
                    if !self.filters.should_include(path, file_size) {
                        total_filtered.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }

                    // Detect language
                    let language = self.language_detector.detect_language(path);
                    
                    // Skip unknown languages for now
                    if language == "unknown" {
                        continue;
                    }

                    // Calculate relative path
                    let relative_path = path.strip_prefix(root_path)
                        .unwrap_or(path)
                        .to_path_buf();

                    let discovered_file = DiscoveredFile {
                        path: path.to_path_buf(),
                        language: language.clone(),
                        size: file_size,
                        relative_path,
                    };

                    discovered_files.push(discovered_file);
                    *languages_found.entry(language).or_insert(0) += 1;
                }
                Err(e) => {
                    warn!("Error walking directory: {}", e);
                }
            }
        }

        if let Some(pb) = progress {
            pb.finish_and_clear();
            println!(
                "Scan complete! Found {} files ({} filtered out)",
                discovered_files.len(),
                total_filtered.load(Ordering::Relaxed)
            );
        }

        let scan_duration = start_time.elapsed();

        info!(
            "Repository scan completed in {:?}. Found {} files, filtered out {}",
            scan_duration,
            discovered_files.len(),
            total_filtered.load(Ordering::Relaxed)
        );

        Ok(ScanResult {
            files: discovered_files,
            total_files_found: total_found.load(Ordering::Relaxed),
            total_files_filtered: total_filtered.load(Ordering::Relaxed),
            scan_duration,
            languages_found,
        })
    }

    fn build_walker(&self, root_path: &Path) -> Result<Walk> {
        let mut builder = WalkBuilder::new(root_path);
        
        builder
            .follow_links(self.config.follow_symlinks)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .hidden(false); // Include hidden files by default

        if let Some(max_depth) = self.config.max_depth {
            builder.max_depth(Some(max_depth as usize));
        }

        // Add thread count for parallel processing
        if self.config.parallel {
            builder.threads(num_cpus::get());
        } else {
            builder.threads(1);
        }

        Ok(builder.build())
    }

    /// Quick scan that just counts files without detailed analysis
    pub fn quick_scan<P: AsRef<Path>>(&self, root_path: P) -> Result<(usize, std::collections::HashMap<String, usize>)> {
        let root_path = root_path.as_ref();
        let walker = self.build_walker(root_path)?;

        let mut count = 0;
        let mut languages = std::collections::HashMap::new();

        for result in walker {
            if let Ok(entry) = result {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path = entry.path();
                    
                    if let Ok(metadata) = entry.metadata() {
                        if self.filters.should_include(path, metadata.len()) {
                            let language = self.language_detector.detect_language(path);
                            if language != "unknown" {
                                count += 1;
                                *languages.entry(language).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok((count, languages))
    }

    /// Check if a path should be analyzed based on current filters
    pub fn should_analyze(&self, path: &Path) -> Result<bool> {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("Failed to get metadata for {:?}", path))?;
        
        let file_size = metadata.len();
        Ok(self.filters.should_include(path, file_size))
    }

    /// Get language for a specific file
    pub fn detect_file_language(&self, path: &Path) -> String {
        self.language_detector.detect_language(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> FileScanningConfig {
        FileScanningConfig {
            include_patterns: vec!["*.rs".to_string(), "*.py".to_string()],
            exclude_patterns: vec!["target/*".to_string(), "*.test.rs".to_string()],
            max_file_size: 1024 * 1024, // 1MB
            follow_symlinks: false,
            max_depth: Some(10),
        }
    }

    #[test]
    fn test_file_filters_creation() {
        let config = create_test_config();
        let filters = FileFilters::new(&config).unwrap();

        // Test include patterns
        assert!(filters.should_include(Path::new("main.rs"), 1000));
        assert!(filters.should_include(Path::new("script.py"), 1000));
        assert!(!filters.should_include(Path::new("main.js"), 1000));

        // Test exclude patterns
        assert!(!filters.should_include(Path::new("target/debug/main.rs"), 1000));
        assert!(!filters.should_include(Path::new("main.test.rs"), 1000));

        // Test file size limit
        assert!(!filters.should_include(Path::new("huge.rs"), 2 * 1024 * 1024));
    }

    #[test]
    fn test_language_detector() {
        let detector = LanguageDetector::new().unwrap();

        assert_eq!(detector.detect_language(Path::new("main.rs")), "rust");
        assert_eq!(detector.detect_language(Path::new("script.py")), "python");
        assert_eq!(detector.detect_language(Path::new("app.ts")), "typescript");
        assert_eq!(detector.detect_language(Path::new("app.js")), "javascript");
        assert_eq!(detector.detect_language(Path::new("main.go")), "go");
        assert_eq!(detector.detect_language(Path::new("unknown.txt")), "unknown");
    }

    #[test]
    fn test_repository_scanner() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        // Create test files
        fs::write(root.join("main.rs"), "fn main() {}")?;
        fs::write(root.join("lib.rs"), "pub fn test() {}")?;
        fs::write(root.join("script.py"), "print('hello')")?;
        fs::write(root.join("readme.txt"), "This is a readme")?;

        // Create subdirectory
        fs::create_dir(root.join("src"))?;
        fs::write(root.join("src/parser.rs"), "pub mod parser;")?;

        // Create excluded file
        fs::create_dir(root.join("target"))?;
        fs::write(root.join("target/main.rs"), "// generated")?;

        let config = create_test_config();
        let scan_config = ScanConfig {
            show_progress: false,
            ..Default::default()
        };

        let scanner = RepositoryScanner::new(&config, Some(scan_config))?;
        let result = scanner.scan(root)?;

        // Should find main.rs, lib.rs, script.py, src/parser.rs
        // Should exclude readme.txt (not in include patterns) and target/main.rs (in exclude patterns)
        assert_eq!(result.files.len(), 4);
        assert_eq!(result.languages_found.get("rust"), Some(&3));
        assert_eq!(result.languages_found.get("python"), Some(&1));

        Ok(())
    }
} 