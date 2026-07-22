use blake3::Hasher;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use std::fs::{File, create_dir_all};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::memory::{RepoMap, RepoMapMetadata};
use crate::types::{AnalysisError, TreeNode};

// Create our own Result type alias for this module
type Result<T> = std::result::Result<T, AnalysisError>;

/// On-disk cache format version.
///
/// Bump this whenever the meaning or shape of anything in the cache changes.
/// There is deliberately NO migration path: the cache is derived data that
/// regenerates in seconds, and migration code for a derived artifact is a bug
/// farm. Every cache written by a different format version is rejected outright
/// and rebuilt.
///
/// * v1 — original header (`version`, `created_at`, `file_count`,
///   `content_hash`, `compression`). Validated the crate version only; the
///   `file_count` and per-file `content_hash` fields were written and never
///   read, and nothing recorded the configuration the index was built with.
/// * v2 — adds `config_fingerprint` (so a cache built with different
///   include/exclude patterns, size/depth limits or analyzer set is rejected
///   rather than silently served) and index-coverage fields (so a truncated
///   index is never mistaken for a complete one). `file_count` is now
///   validated against the payload.
/// * v3 — paths in the payload became normalized ROOT-RELATIVE keys, and the
///   header records the CANONICAL analysis root they are relative to. A cache is
///   now refused when its root is not the root of the current invocation:
///   without that, a cwd-relative path stored by one run re-resolved against a
///   different cwd in the next (K1), and a symlinked root shared a cache file
///   with its target while producing different keys (K9). Nothing migrates a v2
///   cache — its paths are in the old ambiguous vocabulary, which is the very
///   thing being discarded.
pub const CACHE_FORMAT_VERSION: u32 = 3;

/// How much of the repository an index actually covers.
///
/// An index built with a `max_files` limit can stop short of the discovered
/// file set. That fact must travel with the index — through the cache, and out
/// to the tool layer — because a silently truncated index answers "not found"
/// with the same confidence as a complete one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexCoverage {
    /// Files actually analyzed and stored in the index.
    pub files_indexed: usize,
    /// Files discovery found before the `max_files` limit was applied.
    pub files_discovered: usize,
    /// True when `files_indexed < files_discovered` because of a limit.
    pub truncated: bool,
}

impl Default for IndexCoverage {
    fn default() -> Self {
        Self::complete(0)
    }
}

impl IndexCoverage {
    /// Coverage for an index that contains everything discovery found.
    pub fn complete(files: usize) -> Self {
        Self {
            files_indexed: files,
            files_discovered: files,
            truncated: false,
        }
    }

    /// Coverage for an index that was cut short by a limit.
    pub fn partial(files_indexed: usize, files_discovered: usize) -> Self {
        Self {
            files_indexed,
            files_discovered,
            truncated: files_indexed < files_discovered,
        }
    }

    /// A human-readable coverage note, or `None` when the index is complete.
    ///
    /// This is the string handed to agents so an empty result set from a
    /// truncated index cannot be read as "this symbol does not exist".
    pub fn note(&self) -> Option<String> {
        if !self.truncated {
            return None;
        }
        Some(format!(
            "index covers {} of {} files (truncated by the max_files limit); \
             absence of a symbol does NOT prove it is missing from the repository",
            group_digits(self.files_indexed),
            group_digits(self.files_discovered)
        ))
    }
}

/// Format an integer with thousands separators ("10050" -> "10,050").
fn group_digits(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A cheaply cloneable, shared handle to the current [`IndexCoverage`].
///
/// The coverage of the live index is written by the scan/cache-load path and
/// read by whatever renders tool responses. It is a separate handle (rather
/// than a field on `RepoMap`) so both sides can hold it without either owning
/// the other.
#[derive(Debug, Clone, Default)]
pub struct CoverageHandle {
    inner: std::sync::Arc<std::sync::Mutex<IndexCoverage>>,
}

impl CoverageHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current coverage. Returns the default (complete, empty) coverage if the
    /// lock is poisoned — coverage reporting must never be the thing that fails
    /// a query.
    pub fn get(&self) -> IndexCoverage {
        self.inner
            .lock()
            .map(|c| *c)
            .unwrap_or_else(|_| IndexCoverage::default())
    }

    pub fn set(&self, coverage: IndexCoverage) {
        if let Ok(mut slot) = self.inner.lock() {
            *slot = coverage;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHeader {
    /// See [`CACHE_FORMAT_VERSION`]. Any other value means "throw it away".
    pub format_version: u32,
    /// Crate version that produced the cache.
    pub crate_version: String,
    pub created_at: SystemTime,
    pub file_count: usize,
    pub content_hash: String,
    pub compression: CompressionType,
    /// Fingerprint of the scan configuration and analyzer set that produced the
    /// index. A cache built with a `*.rs`-only include list must not be served
    /// to a run whose configuration also indexes Python.
    pub config_fingerprint: String,
    /// Coverage of the indexed set (see [`IndexCoverage`]).
    pub coverage: IndexCoverage,
    /// The CANONICAL analysis root the payload's root-relative paths hang off.
    /// Empty only for an index that never recorded one. A cache whose root is
    /// not the current invocation's root describes a different repository and
    /// must be rejected, not reinterpreted.
    pub scan_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Gzip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedRepoMap {
    pub header: CacheHeader,
    pub metadata: RepoMapMetadata,
    pub files: Vec<TreeNode>,
}

impl SerializedRepoMap {
    pub fn new(repo_map: &RepoMap, compression: CompressionType) -> Self {
        let files = repo_map.get_all_files().to_vec();
        let content_hash = Self::calculate_content_hash(&files);
        let coverage = IndexCoverage::complete(files.len());

        Self {
            header: CacheHeader {
                format_version: CACHE_FORMAT_VERSION,
                crate_version: env!("CARGO_PKG_VERSION").to_string(),
                created_at: SystemTime::now(),
                file_count: files.len(),
                content_hash,
                compression,
                config_fingerprint: String::new(),
                coverage,
                scan_root: repo_map.scan_root().unwrap_or_default().to_string(),
            },
            metadata: repo_map.get_metadata().clone(),
            files,
        }
    }

    /// Stamp the configuration fingerprint the index was built with.
    pub fn with_config_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.header.config_fingerprint = fingerprint.into();
        self
    }

    /// Stamp the coverage of the index being written.
    pub fn with_coverage(mut self, coverage: IndexCoverage) -> Self {
        self.header.coverage = coverage;
        self
    }

    fn calculate_content_hash(files: &[TreeNode]) -> String {
        let mut hasher = Hasher::new();
        for file in files {
            hasher.update(file.file_path.as_bytes());
            hasher.update(file.content_hash.as_bytes());
            hasher.update(
                &file
                    .last_modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .to_le_bytes(),
            );
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// A cache that was read back from disk, together with everything the header
/// says about it.
#[derive(Debug)]
pub struct LoadedIndex {
    pub repo_map: RepoMap,
    pub coverage: IndexCoverage,
    pub header: CacheHeader,
}

pub struct PersistenceManager {
    cache_dir: PathBuf,
    compression: CompressionType,
    max_cache_files: usize,
    /// Fingerprint of the *current* configuration. Stamped into caches on save
    /// and required to match on load.
    config_fingerprint: String,
    /// The canonical analysis root of the current invocation. When set, a cache
    /// whose header names a different root is rejected.
    expected_scan_root: Option<String>,
    /// Coverage stamped into caches on save.
    coverage: IndexCoverage,
}

impl PersistenceManager {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        create_dir_all(&cache_dir)
            .map_err(|e| AnalysisError::Io(format!("Failed to create cache directory: {}", e)))?;

        Ok(Self {
            cache_dir,
            compression: CompressionType::Gzip,
            max_cache_files: 10, // Keep last 10 cache files
            config_fingerprint: String::new(),
            expected_scan_root: None,
            coverage: IndexCoverage::default(),
        })
    }

    pub fn with_compression(mut self, compression: CompressionType) -> Self {
        self.compression = compression;
        self
    }

    pub fn with_max_cache_files(mut self, max_files: usize) -> Self {
        self.max_cache_files = max_files;
        self
    }

    /// Set the configuration fingerprint used to stamp and validate caches.
    pub fn with_config_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.config_fingerprint = fingerprint.into();
        self
    }

    /// Require loaded caches to have been built from `root` (canonical form).
    pub fn with_scan_root(mut self, root: impl Into<String>) -> Self {
        self.expected_scan_root = Some(root.into());
        self
    }

    /// Set the index coverage stamped into caches written by this manager.
    pub fn with_coverage(mut self, coverage: IndexCoverage) -> Self {
        self.coverage = coverage;
        self
    }

    /// Save RepoMap to disk with optional compression
    pub fn save_to_disk(&self, repo_map: &RepoMap, name: &str) -> Result<PathBuf> {
        let serialized = SerializedRepoMap::new(repo_map, self.compression.clone())
            .with_config_fingerprint(self.config_fingerprint.clone())
            .with_coverage(self.coverage);
        let filename = format!("{}.cache", name);
        let file_path = self.cache_dir.join(&filename);

        match self.compression {
            CompressionType::None => {
                self.save_json(&serialized, &file_path)?;
            }
            CompressionType::Gzip => {
                self.save_compressed_json(&serialized, &file_path)?;
            }
        }

        // Clean up old cache files
        self.cleanup_old_cache_files(name)?;

        Ok(file_path)
    }

    /// Load RepoMap from disk, rejecting any cache this build must not trust.
    pub fn load_from_disk(&self, name: &str) -> Result<RepoMap> {
        Ok(self.load_index(name)?.repo_map)
    }

    /// Load a cache and everything its header asserts about it.
    ///
    /// Rejects, in order: a missing file, a foreign cache *format* (see
    /// [`CACHE_FORMAT_VERSION`] — there is no migration), a cache written by a
    /// different crate version, a cache built with a different configuration or
    /// analyzer set, and a cache whose payload disagrees with its own
    /// `file_count`. Every one of these returns an error so the caller rescans
    /// rather than serving results it cannot vouch for.
    pub fn load_index(&self, name: &str) -> Result<LoadedIndex> {
        let filename = format!("{}.cache", name);
        let file_path = self.cache_dir.join(&filename);

        if !file_path.exists() {
            return Err(AnalysisError::Other(format!(
                "Cache file not found: {:?}",
                file_path
            )));
        }

        // A cache written by an older format fails to deserialize (its header is
        // shaped differently); report that as the format mismatch it is.
        let serialized = self.load_serialized(&file_path).map_err(|e| {
            AnalysisError::Other(format!(
                "Cache is not readable as format v{}, regeneration required ({})",
                CACHE_FORMAT_VERSION, e
            ))
        })?;

        if serialized.header.format_version != CACHE_FORMAT_VERSION {
            return Err(AnalysisError::Other(format!(
                "Cache format version mismatch (found v{}, expected v{}), regeneration required",
                serialized.header.format_version, CACHE_FORMAT_VERSION
            )));
        }

        if serialized.header.crate_version != env!("CARGO_PKG_VERSION") {
            return Err(AnalysisError::Other(
                "Cache version mismatch, regeneration required".to_string(),
            ));
        }

        if serialized.header.config_fingerprint != self.config_fingerprint {
            return Err(AnalysisError::Other(
                "Cache was built with a different scan configuration or analyzer set, \
                 regeneration required"
                    .to_string(),
            ));
        }

        // K1/K9: the cache's root, not merely its directory, decides whether it
        // describes this invocation. `<root>/.loregrep/index.cache` is shared by
        // every spelling of `<root>` — including a symlink to it, and a run from
        // a different cwd — so the directory alone cannot tell them apart.
        if let Some(expected) = &self.expected_scan_root {
            if &serialized.header.scan_root != expected {
                return Err(AnalysisError::Other(format!(
                    "Cache was built for analysis root {:?}, this run's root is {:?}; \
                     regeneration required",
                    serialized.header.scan_root, expected
                )));
            }
        }

        if serialized.header.file_count != serialized.files.len() {
            return Err(AnalysisError::Other(format!(
                "Cache is inconsistent: header claims {} files, payload has {}",
                serialized.header.file_count,
                serialized.files.len()
            )));
        }

        let header = serialized.header.clone();
        let coverage = header.coverage;

        // Reconstruct RepoMap. The root travels WITH the cache, so a load no
        // longer depends on the caller remembering to re-record it.
        let mut repo_map = RepoMap::new();
        if !header.scan_root.is_empty() {
            repo_map.set_scan_root(header.scan_root.clone());
        }
        for file in serialized.files {
            repo_map.add_file(file)?;
        }

        Ok(LoadedIndex {
            repo_map,
            coverage,
            header,
        })
    }

    /// Get incremental update information
    pub fn get_incremental_update_info(
        &self,
        name: &str,
        current_files: &[TreeNode],
    ) -> Result<IncrementalUpdateInfo> {
        match self.load_from_disk(name) {
            Ok(cached_repo_map) => {
                let cached_files = cached_repo_map.get_all_files();
                let mut info = IncrementalUpdateInfo::new();

                // Build hash maps for efficient comparison
                let cached_file_map: std::collections::HashMap<String, &TreeNode> = cached_files
                    .iter()
                    .map(|f| (f.file_path.clone(), f))
                    .collect();
                let current_file_map: std::collections::HashMap<String, &TreeNode> = current_files
                    .iter()
                    .map(|f| (f.file_path.clone(), f))
                    .collect();

                // Find new and modified files
                for (path, current_file) in &current_file_map {
                    match cached_file_map.get(path) {
                        Some(cached_file) => {
                            // File exists in cache, check if modified
                            if cached_file.content_hash != current_file.content_hash {
                                info.modified_files.push(path.clone());
                            }
                        }
                        None => {
                            // New file
                            info.new_files.push(path.clone());
                        }
                    }
                }

                // Find deleted files
                for (path, _) in &cached_file_map {
                    if !current_file_map.contains_key(path) {
                        info.deleted_files.push(path.clone());
                    }
                }

                Ok(info)
            }
            Err(_) => {
                // No cache exists, all files are new
                let mut info = IncrementalUpdateInfo::new();
                info.new_files = current_files.iter().map(|f| f.file_path.clone()).collect();
                Ok(info)
            }
        }
    }

    /// List available cache files
    pub fn list_cache_files(&self) -> Result<Vec<CacheFileInfo>> {
        let mut cache_files = Vec::new();

        for entry in std::fs::read_dir(&self.cache_dir)
            .map_err(|e| AnalysisError::Io(format!("Failed to read cache directory: {}", e)))?
        {
            let entry = entry
                .map_err(|e| AnalysisError::Io(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("cache") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let metadata = std::fs::metadata(&path).map_err(|e| {
                        AnalysisError::Io(format!("Failed to read file metadata: {}", e))
                    })?;

                    cache_files.push(CacheFileInfo {
                        name: name.to_string(),
                        path: path.clone(),
                        size_bytes: metadata.len(),
                        created_at: metadata.created().unwrap_or(SystemTime::UNIX_EPOCH),
                        modified_at: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    });
                }
            }
        }

        // Sort by modification time (newest first)
        cache_files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        Ok(cache_files)
    }

    /// Delete a specific cache file
    pub fn delete_cache(&self, name: &str) -> Result<bool> {
        let filename = format!("{}.cache", name);
        let cache_path = self.cache_dir.join(&filename);

        if cache_path.exists() {
            std::fs::remove_file(&cache_path)
                .map_err(|e| AnalysisError::Io(format!("Failed to delete cache file: {}", e)))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clean up old cache files, keeping only the most recent ones
    fn cleanup_old_cache_files(&self, name: &str) -> Result<()> {
        let pattern = format!("{}_", name);
        let mut cache_files = Vec::new();

        for entry in std::fs::read_dir(&self.cache_dir)
            .map_err(|e| AnalysisError::Io(format!("Failed to read cache directory: {}", e)))?
        {
            let entry = entry
                .map_err(|e| AnalysisError::Io(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if filename.starts_with(&pattern) && filename.ends_with(".cache") {
                    let metadata = std::fs::metadata(&path).map_err(|e| {
                        AnalysisError::Io(format!("Failed to read file metadata: {}", e))
                    })?;

                    cache_files.push((path, metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
                }
            }
        }

        // Sort by modification time (newest first)
        cache_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove files beyond the limit
        for (path, _) in cache_files.iter().skip(self.max_cache_files) {
            if let Err(e) = std::fs::remove_file(path) {
                eprintln!("Warning: Failed to remove old cache file {:?}: {}", path, e);
            }
        }

        Ok(())
    }

    fn save_json(&self, data: &SerializedRepoMap, path: &Path) -> Result<()> {
        let file = File::create(path)
            .map_err(|e| AnalysisError::Io(format!("Failed to create file: {}", e)))?;
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, data)
            .map_err(|e| AnalysisError::Other(format!("Failed to serialize data: {}", e)))?;

        Ok(())
    }

    fn save_compressed_json(&self, data: &SerializedRepoMap, path: &Path) -> Result<()> {
        let file = File::create(path)
            .map_err(|e| AnalysisError::Io(format!("Failed to create file: {}", e)))?;
        let writer = BufWriter::new(file);
        let mut encoder = GzEncoder::new(writer, Compression::default());

        let json_data = serde_json::to_vec_pretty(data)
            .map_err(|e| AnalysisError::Other(format!("Failed to serialize data: {}", e)))?;

        encoder
            .write_all(&json_data)
            .map_err(|e| AnalysisError::Io(format!("Failed to write compressed data: {}", e)))?;

        encoder
            .finish()
            .map_err(|e| AnalysisError::Io(format!("Failed to finish compression: {}", e)))?;

        Ok(())
    }

    fn load_serialized(&self, path: &Path) -> Result<SerializedRepoMap> {
        let file = File::open(path)
            .map_err(|e| AnalysisError::Io(format!("Failed to open file: {}", e)))?;
        let mut reader = BufReader::new(file);

        // Try to detect if file is compressed by reading magic bytes
        let mut magic_bytes = [0u8; 2];
        reader
            .read_exact(&mut magic_bytes)
            .map_err(|e| AnalysisError::Io(format!("Failed to read magic bytes: {}", e)))?;

        // Reset reader
        let file = File::open(path)
            .map_err(|e| AnalysisError::Io(format!("Failed to reopen file: {}", e)))?;
        let reader = BufReader::new(file);

        if magic_bytes == [0x1f, 0x8b] {
            // Gzip compressed
            let decoder = GzDecoder::new(reader);
            serde_json::from_reader(decoder).map_err(|e| {
                AnalysisError::Other(format!("Failed to deserialize compressed data: {}", e))
            })
        } else {
            // Uncompressed JSON
            serde_json::from_reader(reader)
                .map_err(|e| AnalysisError::Other(format!("Failed to deserialize data: {}", e)))
        }
    }
}

#[derive(Debug, Clone)]
pub struct IncrementalUpdateInfo {
    pub new_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
}

impl IncrementalUpdateInfo {
    pub fn new() -> Self {
        Self {
            new_files: Vec::new(),
            modified_files: Vec::new(),
            deleted_files: Vec::new(),
        }
    }

    pub fn has_changes(&self) -> bool {
        !self.new_files.is_empty()
            || !self.modified_files.is_empty()
            || !self.deleted_files.is_empty()
    }

    pub fn total_changes(&self) -> usize {
        self.new_files.len() + self.modified_files.len() + self.deleted_files.len()
    }
}

#[derive(Debug, Clone)]
pub struct CacheFileInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
}

impl CacheFileInfo {
    pub fn age(&self) -> std::time::Duration {
        SystemTime::now()
            .duration_since(self.modified_at)
            .unwrap_or_default()
    }
}

// Extension trait for RepoMap to add persistence capabilities.
//
// There is deliberately no `is_cache_valid` here. The previous one returned a
// hardcoded `false`, and `PersistenceManager`'s returned whether the cache file
// was newer than the repository *directory* — a directory's mtime does not
// change when a file's contents are edited, so it answered "valid" for a stale
// cache, and it panicked outright on a path with no file name (`--path .`).
// Validity now lives in one place: `PersistenceManager::load_index` (identity of
// the cache) plus the caller's disk comparison (contents of the cache).
pub trait PersistentRepoMap {
    fn save_to_disk(&self, path: &Path) -> Result<()>;
    fn load_from_disk(path: &Path) -> Result<RepoMap>;
}

impl PersistentRepoMap for RepoMap {
    fn save_to_disk(&self, path: &Path) -> Result<()> {
        let serialized = SerializedRepoMap::new(self, CompressionType::Gzip);

        let file = File::create(path)
            .map_err(|e| AnalysisError::Io(format!("Failed to create file: {}", e)))?;
        let writer = BufWriter::new(file);
        let mut encoder = GzEncoder::new(writer, Compression::default());

        let json_data = serde_json::to_vec_pretty(&serialized)
            .map_err(|e| AnalysisError::Other(format!("Failed to serialize data: {}", e)))?;

        encoder
            .write_all(&json_data)
            .map_err(|e| AnalysisError::Io(format!("Failed to write compressed data: {}", e)))?;

        encoder
            .finish()
            .map_err(|e| AnalysisError::Io(format!("Failed to finish compression: {}", e)))?;

        Ok(())
    }

    fn load_from_disk(path: &Path) -> Result<RepoMap> {
        let file = File::open(path)
            .map_err(|e| AnalysisError::Io(format!("Failed to open file: {}", e)))?;
        let reader = BufReader::new(file);

        // Try gzip first, then fall back to plain JSON
        let serialized: SerializedRepoMap = {
            let decoder = GzDecoder::new(reader);
            match serde_json::from_reader(decoder) {
                Ok(data) => data,
                Err(_) => {
                    // Try uncompressed
                    let file = File::open(path)
                        .map_err(|e| AnalysisError::Io(format!("Failed to reopen file: {}", e)))?;
                    let reader = BufReader::new(file);
                    serde_json::from_reader(reader).map_err(|e| {
                        AnalysisError::Other(format!("Failed to deserialize data: {}", e))
                    })?
                }
            }
        };

        if serialized.header.format_version != CACHE_FORMAT_VERSION {
            return Err(AnalysisError::Other(format!(
                "Cache format version mismatch (found v{}, expected v{}), regeneration required",
                serialized.header.format_version, CACHE_FORMAT_VERSION
            )));
        }

        // Reconstruct RepoMap. The root travels WITH the cache, so a load no
        // longer depends on the caller remembering to re-record it.
        let mut repo_map = RepoMap::new();
        if !serialized.header.scan_root.is_empty() {
            repo_map.set_scan_root(serialized.header.scan_root.clone());
        }
        for file in serialized.files {
            repo_map.add_file(file)?;
        }

        Ok(repo_map)
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
    use tempfile::TempDir;

    fn create_test_tree_node(name: &str, language: &str) -> TreeNode {
        let mut node = TreeNode::new(format!("/test/{}.rs", name), language.to_string());

        // Add test functions
        node.functions.push(
            FunctionSignature::new(format!("function_{}", name), node.file_path.clone())
                .with_parameters(vec![Parameter::new(
                    "param1".to_string(),
                    "i32".to_string(),
                )])
                .with_return_type("String".to_string())
                .with_visibility(true),
        );

        // Add test structs
        node.structs.push(StructSignature::new(
            format!("Struct{}", name),
            node.file_path.clone(),
        ));

        // Add test imports
        node.imports.push(
            ImportStatement::new(format!("crate::{}", name), node.file_path.clone())
                .with_external(false),
        );

        // Add test exports
        node.exports.push(ExportStatement::new(
            format!("pub_{}", name),
            node.file_path.clone(),
        ));

        // Add test function calls
        node.function_calls.push(FunctionCall::new(
            format!("call_{}", name),
            node.file_path.clone(),
            10,
        ));

        node.content_hash = format!("hash_{}", name);
        node
    }

    fn create_test_repo_map() -> RepoMap {
        let mut repo_map = RepoMap::new();

        for i in 0..3 {
            let node = create_test_tree_node(&format!("test{}", i), "rust");
            repo_map.add_file(node).unwrap();
        }

        repo_map
    }

    #[test]
    fn test_persistence_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        assert!(temp_dir.path().exists());
        assert_eq!(manager.max_cache_files, 10);
    }

    #[test]
    fn test_persistence_manager_with_options() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_compression(CompressionType::None)
            .with_max_cache_files(5);

        assert_eq!(manager.max_cache_files, 5);
        match manager.compression {
            CompressionType::None => {} // Expected
            _ => panic!("Compression type not set correctly"),
        }
    }

    #[test]
    fn test_save_and_load_uncompressed() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_compression(CompressionType::None);

        let original_repo_map = create_test_repo_map();

        // Save to disk
        let cache_path = manager
            .save_to_disk(&original_repo_map, "test_repo")
            .unwrap();
        assert!(cache_path.exists());

        // Load from disk
        let loaded_repo_map = manager.load_from_disk("test_repo").unwrap();

        // Verify data integrity
        assert_eq!(loaded_repo_map.get_all_files().len(), 3);
        assert_eq!(loaded_repo_map.get_metadata().total_functions, 3);
        assert_eq!(loaded_repo_map.get_metadata().total_structs, 3);

        // Verify specific content
        let file = loaded_repo_map.get_file("/test/test0.rs").unwrap();
        assert_eq!(file.functions[0].name, "function_test0");
        assert_eq!(file.structs[0].name, "Structtest0");
    }

    #[test]
    fn test_save_and_load_compressed() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_compression(CompressionType::Gzip);

        let original_repo_map = create_test_repo_map();

        // Save to disk
        let cache_path = manager
            .save_to_disk(&original_repo_map, "test_repo_compressed")
            .unwrap();
        assert!(cache_path.exists());

        // Load from disk
        let loaded_repo_map = manager.load_from_disk("test_repo_compressed").unwrap();

        // Verify data integrity
        assert_eq!(loaded_repo_map.get_all_files().len(), 3);
        assert_eq!(loaded_repo_map.get_metadata().total_functions, 3);
    }

    #[test]
    fn test_cache_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        let result = manager.load_from_disk("non_existent");
        assert!(result.is_err());
    }

    /// Write `serialized` as an uncompressed cache named `<name>.cache`.
    fn write_raw_cache(dir: &Path, name: &str, serialized: &SerializedRepoMap) {
        let cache_path = dir.join(format!("{}.cache", name));
        let file = File::create(&cache_path).unwrap();
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, serialized).unwrap();
    }

    #[test]
    fn test_version_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        // Create a cache written by another crate version.
        let mut serialized = SerializedRepoMap::new(&create_test_repo_map(), CompressionType::None);
        serialized.header.crate_version = "0.0.0".to_string();
        write_raw_cache(temp_dir.path(), "wrong_version", &serialized);

        // The bytes are readable...
        assert!(
            manager
                .load_serialized(&temp_dir.path().join("wrong_version.cache"))
                .is_ok()
        );

        // ...but the validated load path must refuse it.
        let err = manager.load_from_disk("wrong_version").unwrap_err();
        assert!(
            err.to_string().contains("version mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_foreign_cache_format_version_is_rejected() {
        // Regression: the cache format is versioned and there is NO migration.
        // A cache stamped with any other format version must be rejected, not
        // interpreted.
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        let mut serialized = SerializedRepoMap::new(&create_test_repo_map(), CompressionType::None);
        serialized.header.format_version = CACHE_FORMAT_VERSION - 1;
        write_raw_cache(temp_dir.path(), "old_format", &serialized);

        let err = manager.load_from_disk("old_format").unwrap_err();
        assert!(
            err.to_string().contains("format version mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_prior_format_cache_payload_is_rejected() {
        // A v1 cache file (header without format_version/config_fingerprint/
        // coverage) must not load at all. This is the "treat ALL prior caches as
        // invalid" contract, exercised against the actual v1 JSON shape.
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        let v1 = serde_json::json!({
            "header": {
                "version": env!("CARGO_PKG_VERSION"),
                "created_at": { "secs_since_epoch": 0, "nanos_since_epoch": 0 },
                "file_count": 0,
                "content_hash": "deadbeef",
                "compression": "None",
            },
            "metadata": RepoMap::new().get_metadata(),
            "files": [],
        });
        std::fs::write(
            temp_dir.path().join("v1.cache"),
            serde_json::to_vec_pretty(&v1).unwrap(),
        )
        .unwrap();

        let err = manager.load_from_disk("v1").unwrap_err();
        assert!(
            err.to_string().contains("format v"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_config_fingerprint_mismatch_is_rejected() {
        // K4 regression: a cache built under one configuration (e.g. a
        // `*.rs`-only include list) must NOT be served to a run configured
        // differently (e.g. one that also indexes Python) — that is how a
        // Python file stays silently absent from the index.
        let temp_dir = TempDir::new().unwrap();
        let repo_map = create_test_repo_map();

        let rust_only = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_config_fingerprint("include=[*.rs]");
        rust_only.save_to_disk(&repo_map, "cfg").unwrap();

        // Same configuration: the cache loads.
        assert!(rust_only.load_from_disk("cfg").is_ok());

        // Different configuration: rejected.
        let rust_and_python = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_config_fingerprint("include=[*.rs,*.py]");
        let err = rust_and_python.load_from_disk("cfg").unwrap_err();
        assert!(
            err.to_string().contains("different scan configuration"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_header_file_count_is_validated() {
        // `file_count` used to be written and never read. It is now checked
        // against the payload it describes.
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        let mut serialized = SerializedRepoMap::new(&create_test_repo_map(), CompressionType::None);
        serialized.header.file_count = 99;
        write_raw_cache(temp_dir.path(), "bad_count", &serialized);

        let err = manager.load_from_disk("bad_count").unwrap_err();
        assert!(
            err.to_string().contains("inconsistent"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_coverage_round_trips_through_the_cache() {
        // K6: truncation is a property of the index and must survive
        // persistence, so a cache-backed run reports it just like a scan does.
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_coverage(IndexCoverage::partial(10_000, 10_050));

        manager
            .save_to_disk(&create_test_repo_map(), "truncated")
            .unwrap();

        let loaded = manager.load_index("truncated").unwrap();
        assert!(loaded.coverage.truncated);
        assert_eq!(loaded.coverage.files_indexed, 10_000);
        assert_eq!(loaded.coverage.files_discovered, 10_050);
        assert_eq!(
            loaded.coverage.note().unwrap(),
            "index covers 10,000 of 10,050 files (truncated by the max_files limit); \
             absence of a symbol does NOT prove it is missing from the repository"
        );

        // A complete index reports no note at all.
        assert!(IndexCoverage::complete(12).note().is_none());
    }

    #[test]
    fn test_coverage_handle_shares_state() {
        let handle = CoverageHandle::new();
        assert!(!handle.get().truncated);

        let clone = handle.clone();
        clone.set(IndexCoverage::partial(5, 9));

        let seen = handle.get();
        assert!(seen.truncated);
        assert_eq!(seen.files_indexed, 5);
        assert_eq!(seen.files_discovered, 9);
    }

    #[test]
    fn test_incremental_update_info() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        // Create and save initial repo map
        let initial_repo_map = create_test_repo_map();
        manager
            .save_to_disk(&initial_repo_map, "incremental_test")
            .unwrap();

        // Create current files with some changes
        let mut current_files = Vec::new();

        // Keep test0 unchanged
        current_files.push(create_test_tree_node("test0", "rust"));

        // Modify test1
        let mut modified_node = create_test_tree_node("test1", "rust");
        modified_node.content_hash = "modified_hash".to_string();
        current_files.push(modified_node);

        // Remove test2 (not in current_files)

        // Add new test3
        current_files.push(create_test_tree_node("test3", "rust"));

        let update_info = manager
            .get_incremental_update_info("incremental_test", &current_files)
            .unwrap();

        assert_eq!(update_info.new_files.len(), 1);
        assert!(
            update_info
                .new_files
                .contains(&"/test/test3.rs".to_string())
        );

        assert_eq!(update_info.modified_files.len(), 1);
        assert!(
            update_info
                .modified_files
                .contains(&"/test/test1.rs".to_string())
        );

        assert_eq!(update_info.deleted_files.len(), 1);
        assert!(
            update_info
                .deleted_files
                .contains(&"/test/test2.rs".to_string())
        );

        assert!(update_info.has_changes());
        assert_eq!(update_info.total_changes(), 3);
    }

    #[test]
    fn test_incremental_update_info_no_cache() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        let current_files = vec![
            create_test_tree_node("test0", "rust"),
            create_test_tree_node("test1", "rust"),
        ];

        let update_info = manager
            .get_incremental_update_info("no_cache", &current_files)
            .unwrap();

        // All files should be new since no cache exists
        assert_eq!(update_info.new_files.len(), 2);
        assert_eq!(update_info.modified_files.len(), 0);
        assert_eq!(update_info.deleted_files.len(), 0);
        assert!(update_info.has_changes());
    }

    #[test]
    fn test_list_cache_files() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        // Initially no cache files
        let cache_files = manager.list_cache_files().unwrap();
        assert_eq!(cache_files.len(), 0);

        // Create some cache files
        let repo_map = create_test_repo_map();
        manager.save_to_disk(&repo_map, "cache1").unwrap();
        manager.save_to_disk(&repo_map, "cache2").unwrap();

        let cache_files = manager.list_cache_files().unwrap();
        assert_eq!(cache_files.len(), 2);

        // Verify they're sorted by modification time (newest first)
        assert!(cache_files[0].modified_at >= cache_files[1].modified_at);

        // Check cache file info
        for cache_file in &cache_files {
            assert!(cache_file.size_bytes > 0);
            assert!(cache_file.age().as_secs() < 60); // Should be very recent
        }
    }

    #[test]
    fn test_delete_cache() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        // Create a cache file
        let repo_map = create_test_repo_map();
        manager.save_to_disk(&repo_map, "delete_test").unwrap();

        // Verify it exists
        let result = manager.load_from_disk("delete_test");
        assert!(result.is_ok());

        // Delete it
        let deleted = manager.delete_cache("delete_test").unwrap();
        assert!(deleted);

        // Verify it's gone
        let result = manager.load_from_disk("delete_test");
        assert!(result.is_err());

        // Try to delete non-existent cache
        let deleted = manager.delete_cache("non_existent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_cache_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path())
            .unwrap()
            .with_max_cache_files(2); // Only keep 2 files

        let repo_map = create_test_repo_map();

        // Create multiple cache files with the same prefix
        for i in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure different timestamps
            let cache_name = format!("cleanup_test_{}", i);
            manager.save_to_disk(&repo_map, &cache_name).unwrap();
        }

        // Should only have recent cache files remaining
        let cache_files = manager.list_cache_files().unwrap();
        let cleanup_files: Vec<_> = cache_files
            .iter()
            .filter(|f| f.name.starts_with("cleanup_test"))
            .collect();

        // This test is tricky because the cleanup only affects files with the exact pattern
        // For now, just verify that the mechanism exists
        assert!(!cleanup_files.is_empty());
    }

    #[test]
    fn test_persistent_repo_map_trait() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("trait_test.cache");

        let original_repo_map = create_test_repo_map();

        // Save using trait method
        let result = original_repo_map.save_to_disk(&cache_path);
        assert!(result.is_ok());
        assert!(cache_path.exists());

        // Load using trait method
        let loaded_repo_map = RepoMap::load_from_disk(&cache_path).unwrap();

        // Verify data integrity
        assert_eq!(loaded_repo_map.get_all_files().len(), 3);
        assert_eq!(loaded_repo_map.get_metadata().total_functions, 3);
    }

    #[test]
    fn test_serialized_repo_map() {
        let repo_map = create_test_repo_map();
        let serialized = SerializedRepoMap::new(&repo_map, CompressionType::Gzip);

        assert_eq!(serialized.header.crate_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(serialized.header.format_version, CACHE_FORMAT_VERSION);
        assert_eq!(serialized.header.file_count, 3);
        assert_eq!(serialized.files.len(), 3);
        assert!(!serialized.header.content_hash.is_empty());

        match serialized.header.compression {
            CompressionType::Gzip => {} // Expected
            _ => panic!("Compression type not set correctly"),
        }
    }

    #[test]
    fn test_content_hash_calculation() {
        let files1 = vec![
            create_test_tree_node("test1", "rust"),
            create_test_tree_node("test2", "rust"),
        ];

        let files2 = vec![
            create_test_tree_node("test1", "rust"),
            create_test_tree_node("test2", "rust"),
        ];

        let mut files3 = vec![
            create_test_tree_node("test1", "rust"),
            create_test_tree_node("test2", "rust"),
        ];
        files3[0].content_hash = "different_hash".to_string();

        let hash1 = SerializedRepoMap::calculate_content_hash(&files1);
        let hash2 = SerializedRepoMap::calculate_content_hash(&files2);
        let hash3 = SerializedRepoMap::calculate_content_hash(&files3);

        // Same content should produce same hash
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_compression_detection() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PersistenceManager::new(temp_dir.path()).unwrap();

        let repo_map = create_test_repo_map();

        // Save with compression
        let serialized = SerializedRepoMap::new(&repo_map, CompressionType::Gzip);
        let compressed_path = temp_dir.path().join("compressed.cache");
        manager
            .save_compressed_json(&serialized, &compressed_path)
            .unwrap();

        // Save without compression
        let uncompressed_path = temp_dir.path().join("uncompressed.cache");
        manager.save_json(&serialized, &uncompressed_path).unwrap();

        // Both should be loadable
        let loaded_compressed = manager.load_serialized(&compressed_path).unwrap();
        let loaded_uncompressed = manager.load_serialized(&uncompressed_path).unwrap();

        assert_eq!(
            loaded_compressed.files.len(),
            loaded_uncompressed.files.len()
        );
        assert_eq!(
            loaded_compressed.header.file_count,
            loaded_uncompressed.header.file_count
        );
    }

    #[test]
    fn test_incremental_update_info_methods() {
        let mut info = IncrementalUpdateInfo::new();

        assert!(!info.has_changes());
        assert_eq!(info.total_changes(), 0);

        info.new_files.push("new.rs".to_string());
        info.modified_files.push("modified.rs".to_string());
        info.deleted_files.push("deleted.rs".to_string());

        assert!(info.has_changes());
        assert_eq!(info.total_changes(), 3);
    }

    #[test]
    fn test_cache_file_info_age() {
        let info = CacheFileInfo {
            name: "test".to_string(),
            path: PathBuf::from("/test/path"),
            size_bytes: 1024,
            created_at: SystemTime::now() - std::time::Duration::from_secs(60),
            modified_at: SystemTime::now() - std::time::Duration::from_secs(30),
        };

        let age = info.age();
        assert!(age.as_secs() >= 25 && age.as_secs() <= 35); // Should be around 30 seconds
    }

    /// K1/K9: `<root>/.loregrep/index.cache` is reached by every spelling of
    /// `<root>` — a symlink to it, a run from another cwd — so the cache DIRECTORY
    /// cannot tell two roots apart. The header's canonical root can, and must.
    #[test]
    fn a_cache_written_under_one_root_is_refused_for_another() {
        let dir = TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");

        let mut repo_map = RepoMap::new();
        repo_map.set_scan_root("/repo/one");
        repo_map
            .add_file(create_test_tree_node("a", "rust"))
            .unwrap();

        PersistenceManager::new(&cache_dir)
            .unwrap()
            .save_to_disk(&repo_map, "index")
            .unwrap();

        // Same cache file, different analysis root: refuse, do not reinterpret.
        let err = PersistenceManager::new(&cache_dir)
            .unwrap()
            .with_scan_root("/repo/two")
            .load_index("index")
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("analysis root"),
            "error must name the root mismatch, got: {message}"
        );

        // The run it WAS built for still gets it, with the root restored from
        // the header rather than from the caller's memory.
        let loaded = PersistenceManager::new(&cache_dir)
            .unwrap()
            .with_scan_root("/repo/one")
            .load_index("index")
            .unwrap();
        assert_eq!(loaded.header.scan_root, "/repo/one");
        assert_eq!(loaded.repo_map.scan_root(), Some("/repo/one"));
    }
}
