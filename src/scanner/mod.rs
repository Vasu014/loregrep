// Placeholder for scanner module
// Will be implemented in Phase 5: Task 5.1

pub mod discovery;

pub use discovery::{
    DiscoveredFile, FileFilters, LanguageDetector, RepositoryScanner, ScanConfig, ScanResult,
};
