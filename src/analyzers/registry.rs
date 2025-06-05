use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::analyzers::LanguageAnalyzer;
use crate::types::Result;

/// Trait for managing language analyzers in a registry system
pub trait LanguageAnalyzerRegistry: Send + Sync {
    /// Register a new language analyzer
    fn register(&mut self, analyzer: Box<dyn LanguageAnalyzer>) -> Result<()>;
    
    /// Get an analyzer by language name
    fn get_by_language(&self, language: &str) -> Option<&dyn LanguageAnalyzer>;
    
    /// Get an analyzer by file extension
    fn get_by_extension(&self, extension: &str) -> Option<&dyn LanguageAnalyzer>;
    
    /// Detect language from file path (extension-based for now)
    fn detect_language(&self, file_path: &str, content: &str) -> Option<String>;
    
    /// List all supported languages
    fn list_supported_languages(&self) -> Vec<String>;
    
    /// List all supported file extensions
    fn list_supported_extensions(&self) -> Vec<String>;
}

/// Default implementation of the language analyzer registry
#[derive(Clone)]
pub struct DefaultLanguageRegistry {
    /// Map from language name to analyzer
    analyzers_by_language: Arc<RwLock<HashMap<String, Box<dyn LanguageAnalyzer>>>>,
    /// Map from file extension to language name for fast lookup
    extensions_to_language: Arc<RwLock<HashMap<String, String>>>,
}

impl DefaultLanguageRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            analyzers_by_language: Arc::new(RwLock::new(HashMap::new())),
            extensions_to_language: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Extract file extension from path (without the dot)
    fn extract_extension(file_path: &str) -> Option<String> {
        let path = std::path::Path::new(file_path);
        
        // Handle the special case of hidden files like ".hidden"
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.starts_with('.') && filename.chars().filter(|&c| c == '.').count() == 1 {
                // This is a hidden file without an extension like ".hidden"
                return None;
            }
        }
        
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
    }
}

impl Default for DefaultLanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageAnalyzerRegistry for DefaultLanguageRegistry {
    fn register(&mut self, analyzer: Box<dyn LanguageAnalyzer>) -> Result<()> {
        let language = analyzer.language().to_string();
        let extensions = analyzer.file_extensions().iter().map(|ext| ext.to_string()).collect::<Vec<_>>();
        
        // Register the analyzer by language
        {
            let mut analyzers = self.analyzers_by_language.write()
                .map_err(|_| crate::types::AnalysisError::RegistryError { 
                    message: "Failed to acquire write lock for analyzers".to_string() 
                })?;
            
            if analyzers.contains_key(&language) {
                return Err(crate::types::AnalysisError::RegistryError {
                    message: format!("Language '{}' is already registered", language)
                });
            }
            
            analyzers.insert(language.clone(), analyzer);
        }
        
        // Register the file extensions
        {
            let mut ext_map = self.extensions_to_language.write()
                .map_err(|_| crate::types::AnalysisError::RegistryError { 
                    message: "Failed to acquire write lock for extensions".to_string() 
                })?;
            
            for ext in extensions {
                if ext_map.contains_key(&ext) {
                    return Err(crate::types::AnalysisError::RegistryError {
                        message: format!("Extension '{}' is already registered", ext)
                    });
                }
                ext_map.insert(ext, language.clone());
            }
        }
        
        Ok(())
    }
    
    fn get_by_language(&self, language: &str) -> Option<&dyn LanguageAnalyzer> {
        // Note: This is a simplified implementation. In a real-world scenario,
        // we'd need to handle the lifetime issues with RwLock guards properly.
        // For now, we'll return None and handle this in the integration phase.
        None
    }
    
    fn get_by_extension(&self, extension: &str) -> Option<&dyn LanguageAnalyzer> {
        // Same note as above - simplified for now
        None
    }
    
    fn detect_language(&self, file_path: &str, _content: &str) -> Option<String> {
        let extension = Self::extract_extension(file_path)?;
        
        let ext_map = self.extensions_to_language.read().ok()?;
        ext_map.get(&extension).cloned()
    }
    
    fn list_supported_languages(&self) -> Vec<String> {
        if let Ok(analyzers) = self.analyzers_by_language.read() {
            analyzers.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    fn list_supported_extensions(&self) -> Vec<String> {
        if let Ok(ext_map) = self.extensions_to_language.read() {
            ext_map.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
}

/// Helper struct for working with the registry in a thread-safe manner
pub struct RegistryHandle {
    analyzers_by_language: Arc<RwLock<HashMap<String, Box<dyn LanguageAnalyzer>>>>,
    extensions_to_language: Arc<RwLock<HashMap<String, String>>>,
}

impl RegistryHandle {
    pub fn new(registry: &DefaultLanguageRegistry) -> Self {
        Self {
            analyzers_by_language: registry.analyzers_by_language.clone(),
            extensions_to_language: registry.extensions_to_language.clone(),
        }
    }
    
    /// Get analyzer by language name (thread-safe)
    pub fn get_analyzer_by_language(&self, language: &str) -> Option<String> {
        if let Ok(analyzers) = self.analyzers_by_language.read() {
            if analyzers.contains_key(language) {
                Some(language.to_string())
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Get language by extension (thread-safe)
    pub fn get_language_by_extension(&self, extension: &str) -> Option<String> {
        if let Ok(ext_map) = self.extensions_to_language.read() {
            ext_map.get(extension).cloned()
        } else {
            None
        }
    }
    
    /// Detect language from file path
    pub fn detect_language(&self, file_path: &str) -> Option<String> {
        let extension = DefaultLanguageRegistry::extract_extension(file_path)?;
        self.get_language_by_extension(&extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::rust::RustAnalyzer;
    use async_trait::async_trait;
    use crate::types::{FileAnalysis, PartialAnalysis, TreeNode, FunctionSignature, StructSignature, ImportStatement, ExportStatement, FunctionCall};

    // Mock analyzer for testing
    struct MockPythonAnalyzer;

    #[async_trait]
    impl LanguageAnalyzer for MockPythonAnalyzer {
        fn language(&self) -> &'static str {
            "python"
        }
        
        fn file_extensions(&self) -> &[&'static str] {
            &["py", "pyx", "pyi"]
        }
        
        fn supports_async(&self) -> bool {
            true
        }
        
        async fn analyze_file(&self, _content: &str, _file_path: &str) -> Result<FileAnalysis> {
            Ok(FileAnalysis::new(TreeNode::new("test.py".to_string(), "python".to_string()), 0))
        }
        
        fn extract_functions(&self, _tree: &tree_sitter::Tree, _source: &str, _file_path: &str) -> Result<Vec<FunctionSignature>> {
            Ok(Vec::new())
        }
        
        fn extract_structs(&self, _tree: &tree_sitter::Tree, _source: &str, _file_path: &str) -> Result<Vec<StructSignature>> {
            Ok(Vec::new())
        }
        
        fn extract_imports(&self, _tree: &tree_sitter::Tree, _source: &str, _file_path: &str) -> Result<Vec<ImportStatement>> {
            Ok(Vec::new())
        }
        
        fn extract_exports(&self, _tree: &tree_sitter::Tree, _source: &str, _file_path: &str) -> Result<Vec<ExportStatement>> {
            Ok(Vec::new())
        }
        
        fn extract_function_calls(&self, _tree: &tree_sitter::Tree, _source: &str, _file_path: &str) -> Result<Vec<FunctionCall>> {
            Ok(Vec::new())
        }
        
        fn extract_with_fallback(&self, _content: &str, _file_path: &str) -> PartialAnalysis {
            PartialAnalysis::new("test.py".to_string(), "python".to_string())
        }
    }

    #[test]
    fn test_registry_creation() {
        let registry = DefaultLanguageRegistry::new();
        assert_eq!(registry.list_supported_languages().len(), 0);
        assert_eq!(registry.list_supported_extensions().len(), 0);
    }

    #[test]
    fn test_register_analyzer() {
        let mut registry = DefaultLanguageRegistry::new();
        let analyzer = Box::new(MockPythonAnalyzer);
        
        // Should succeed on first registration
        let result = registry.register(analyzer);
        assert!(result.is_ok());
        
        // Should have one language and three extensions
        assert_eq!(registry.list_supported_languages().len(), 1);
        assert_eq!(registry.list_supported_extensions().len(), 3);
        assert!(registry.list_supported_languages().contains(&"python".to_string()));
        assert!(registry.list_supported_extensions().contains(&"py".to_string()));
        assert!(registry.list_supported_extensions().contains(&"pyx".to_string()));
        assert!(registry.list_supported_extensions().contains(&"pyi".to_string()));
    }

    #[test]
    fn test_duplicate_language_registration() {
        let mut registry = DefaultLanguageRegistry::new();
        
        // Register first analyzer
        let analyzer1 = Box::new(MockPythonAnalyzer);
        assert!(registry.register(analyzer1).is_ok());
        
        // Try to register duplicate language
        let analyzer2 = Box::new(MockPythonAnalyzer);
        let result = registry.register(analyzer2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already registered"));
    }

    #[test]
    fn test_language_detection() {
        let mut registry = DefaultLanguageRegistry::new();
        let analyzer = Box::new(MockPythonAnalyzer);
        registry.register(analyzer).unwrap();
        
        // Test extension-based detection
        assert_eq!(registry.detect_language("test.py", ""), Some("python".to_string()));
        assert_eq!(registry.detect_language("module.pyx", ""), Some("python".to_string()));
        assert_eq!(registry.detect_language("types.pyi", ""), Some("python".to_string()));
        assert_eq!(registry.detect_language("test.rs", ""), None);
        assert_eq!(registry.detect_language("README.md", ""), None);
        
        // Test with different cases
        assert_eq!(registry.detect_language("Test.PY", ""), Some("python".to_string()));
        assert_eq!(registry.detect_language("MODULE.PYX", ""), Some("python".to_string()));
    }

    #[test]
    fn test_registry_handle() {
        let mut registry = DefaultLanguageRegistry::new();
        let analyzer = Box::new(MockPythonAnalyzer);
        registry.register(analyzer).unwrap();
        
        let handle = RegistryHandle::new(&registry);
        
        // Test language detection through handle
        assert_eq!(handle.detect_language("test.py"), Some("python".to_string()));
        assert_eq!(handle.detect_language("test.rs"), None);
        
        // Test direct lookups
        assert_eq!(handle.get_language_by_extension("py"), Some("python".to_string()));
        assert_eq!(handle.get_language_by_extension("rs"), None);
        assert!(handle.get_analyzer_by_language("python").is_some());
        assert!(handle.get_analyzer_by_language("rust").is_none());
    }

    #[test]
    fn test_multiple_analyzers() {
        let mut registry = DefaultLanguageRegistry::new();
        
        // Register Python analyzer
        let python_analyzer = Box::new(MockPythonAnalyzer);
        assert!(registry.register(python_analyzer).is_ok());
        
        // Register Rust analyzer
        let rust_analyzer = RustAnalyzer::new().unwrap();
        let rust_boxed = Box::new(rust_analyzer) as Box<dyn LanguageAnalyzer>;
        assert!(registry.register(rust_boxed).is_ok());
        
        // Should have both languages
        let languages = registry.list_supported_languages();
        assert_eq!(languages.len(), 2);
        assert!(languages.contains(&"python".to_string()));
        assert!(languages.contains(&"rust".to_string()));
        
        // Test detection for both
        assert_eq!(registry.detect_language("test.py", ""), Some("python".to_string()));
        assert_eq!(registry.detect_language("main.rs", ""), Some("rust".to_string()));
    }

    #[test]
    fn test_extension_extraction() {
        assert_eq!(DefaultLanguageRegistry::extract_extension("test.py"), Some("py".to_string()));
        assert_eq!(DefaultLanguageRegistry::extract_extension("path/to/file.rs"), Some("rs".to_string()));
        assert_eq!(DefaultLanguageRegistry::extract_extension("FILE.PY"), Some("py".to_string()));
        assert_eq!(DefaultLanguageRegistry::extract_extension("no_extension"), None);
        assert_eq!(DefaultLanguageRegistry::extract_extension(""), None);
        assert_eq!(DefaultLanguageRegistry::extract_extension(".hidden"), None);
    }

    #[test]
    fn test_empty_registry_operations() {
        let registry = DefaultLanguageRegistry::new();
        
        assert_eq!(registry.detect_language("test.py", ""), None);
        assert_eq!(registry.list_supported_languages().len(), 0);
        assert_eq!(registry.list_supported_extensions().len(), 0);
        assert!(registry.get_by_language("python").is_none());
        assert!(registry.get_by_extension("py").is_none());
    }
}