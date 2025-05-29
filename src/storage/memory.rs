// Placeholder RepoMap - will be enhanced in Phase 2: Task 2.1
use crate::types::{
    TreeNode, FunctionSignature, StructSignature, ImportStatement, 
    ExportStatement, AnalysisError
};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;
use regex::Regex;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use serde::{Serialize, Deserialize};

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
#[derive(Debug, Clone)]
pub struct RepoMap {
    // Core data
    files: Vec<TreeNode>,
    
    // Fast indexes
    file_index: HashMap<String, usize>,                    // file_path -> index
    function_index: HashMap<String, Vec<usize>>,           // function_name -> file indices
    struct_index: HashMap<String, Vec<usize>>,             // struct_name -> file indices
    import_index: HashMap<String, Vec<usize>>,             // import_path -> file indices
    export_index: HashMap<String, Vec<usize>>,             // export_name -> file indices
    language_index: HashMap<String, Vec<usize>>,           // language -> file indices
    
    // Call graph
    call_graph: HashMap<String, Vec<CallSite>>,            // function_name -> call sites
    
    // Metadata
    metadata: RepoMapMetadata,
    
    // Memory management
    max_files: Option<usize>,
    
    // Query caching
    query_cache: HashMap<String, (Vec<usize>, SystemTime)>, // query -> (results, timestamp)
    cache_ttl_seconds: u64,
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
                return Err(AnalysisError::Other(format!("Maximum file limit ({}) reached", max)));
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
        
        Ok(())
    }

    /// Remove a file from the repository map
    pub fn remove_file(&mut self, file_path: &str) -> Result<bool> {
        if let Some(&index) = self.file_index.get(file_path) {
            self.remove_file_by_index(index);
            self.update_metadata();
            self.query_cache.clear();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get a file by path
    pub fn get_file(&self, file_path: &str) -> Option<&TreeNode> {
        self.file_index.get(file_path)
            .and_then(|&index| self.files.get(index))
    }

    /// Get all files
    pub fn get_all_files(&self) -> &[TreeNode] {
        &self.files
    }

    /// Get files by language
    pub fn get_files_by_language(&self, language: &str) -> Vec<&TreeNode> {
        self.language_index.get(language)
            .map(|indices| {
                indices.iter()
                    .filter_map(|&i| self.files.get(i))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find functions by pattern (supports regex and fuzzy matching) - Original method
    pub fn find_functions(&self, pattern: &str) -> QueryResult<&FunctionSignature> {
        let start_time = std::time::Instant::now();
        
        // Check cache first
        let cache_key = format!("func:{}", pattern);
        if let Some((cached_indices, timestamp)) = self.query_cache.get(&cache_key) {
            if timestamp.elapsed().unwrap_or_default().as_secs() < self.cache_ttl_seconds {
                let functions: Vec<&FunctionSignature> = cached_indices.iter()
                    .filter_map(|&file_idx| self.files.get(file_idx))
                    .flat_map(|file| &file.functions)
                    .filter(|func| self.matches_pattern(&func.name, pattern))
                    .collect();
                
                let len = functions.len();
                return QueryResult::new(
                    functions,
                    len,
                    start_time.elapsed().as_millis() as u64
                );
            }
        }

        let mut results = Vec::new();
        
        // Try exact match first
        if let Some(file_indices) = self.function_index.get(pattern) {
            for &file_idx in file_indices {
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
    pub fn find_functions_with_options(&self, pattern: &str, limit: usize, fuzzy: bool) -> Vec<&FunctionSignature> {
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
        
        // Try exact match first
        if let Some(file_indices) = self.struct_index.get(pattern) {
            for &file_idx in file_indices {
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
        QueryResult::new(results, len, duration)
    }

    /// Find structs with limit and fuzzy matching support - CLI-compatible method
    pub fn find_structs_with_options(&self, pattern: &str, limit: usize, fuzzy: bool) -> Vec<&StructSignature> {
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
            file.imports.iter()
                .map(|import| import.module_path.clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Find all callers of a specific function
    pub fn find_function_callers(&self, function_name: &str) -> Vec<CallSite> {
        self.call_graph.get(function_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Get repository metadata
    pub fn get_metadata(&self) -> &RepoMapMetadata {
        &self.metadata
    }

    /// Get changed files since a specific time
    pub fn get_changed_files(&self, since: SystemTime) -> Vec<&TreeNode> {
        self.files.iter()
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

        // Remove from files vector and update remaining indexes
        self.files.remove(index);
        self.reindex_after_removal(index);
    }

    fn update_indexes_for_file(&mut self, index: usize, tree_node: &TreeNode) -> Result<()> {
        let file_path = tree_node.file_path.clone();
        
        // Update file index
        self.file_index.insert(file_path, index);

        // Update function index
        for func in &tree_node.functions {
            self.function_index.entry(func.name.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update struct index
        for struct_def in &tree_node.structs {
            self.struct_index.entry(struct_def.name.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update import index
        for import in &tree_node.imports {
            self.import_index.entry(import.module_path.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update export index
        for export in &tree_node.exports {
            self.export_index.entry(export.exported_item.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        // Update language index
        self.language_index.entry(tree_node.language.clone())
            .or_insert_with(Vec::new)
            .push(index);

        // Update call graph
        for call in &tree_node.function_calls {
            let call_site = CallSite {
                file_path: tree_node.file_path.clone(),
                line_number: call.line_number,
                column: call.column,
                function_name: call.function_name.clone(),
                caller_function: None, // TODO: Extract caller context
            };
            
            self.call_graph.entry(call.function_name.clone())
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
        let files_to_update: Vec<(String, usize)> = self.file_index.iter()
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
        if pattern.contains(['*', '^', '$', '[', ']', '(', ')', '{', '}', '|', '+', '?', '\\']) {
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
    use crate::types::{FunctionSignature, StructSignature, ImportStatement, ExportStatement, FunctionCall, Parameter};
    use std::time::SystemTime;

    fn create_test_tree_node(name: &str, language: &str) -> TreeNode {
        let mut node = TreeNode::new(format!("/test/{}.rs", name), language.to_string());
        
        // Add some test functions
        node.functions.push(
            FunctionSignature::new(format!("function_{}", name))
                .with_parameters(vec![
                    Parameter::new("param1".to_string(), "i32".to_string()),
                    Parameter::new("param2".to_string(), "String".to_string()),
                ])
                .with_return_type("Result<(), Error>".to_string())
                .with_visibility(true)
                .with_async(true)
        );
        
        // Add some test structs
        node.structs.push(StructSignature::new(format!("Struct{}", name.to_uppercase())));
        
        // Add some test imports
        node.imports.push(
            ImportStatement::new(format!("crate::{}", name))
                .with_external(false)
        );
        
        // Add some test exports
        node.exports.push(
            ExportStatement::new(format!("pub_{}", name))
        );
        
        // Add some test function calls
        node.function_calls.push(FunctionCall::new(
            format!("call_{}", name),
            node.file_path.clone(),
            42
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
        let repo_map = RepoMap::new()
            .with_max_files(5)
            .with_cache_ttl(60);
        
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
        node.functions.push(FunctionSignature::new("new_function".to_string()));
        
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
        node.functions.push(FunctionSignature::new("calculate_hash".to_string()));
        node.functions.push(FunctionSignature::new("parse_content".to_string()));
        node.structs.push(StructSignature::new("Parser".to_string()));
        node.structs.push(StructSignature::new("Calculator".to_string()));
        repo_map.add_file(node).unwrap();
        
        // Fuzzy search for "calc"
        let results = repo_map.fuzzy_search("calc", Some(10));
        assert!(!results.is_empty());
        
        // Should find both calculate_hash function and Calculator struct
        let calc_results: Vec<_> = results.iter()
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
            node.functions.push(FunctionSignature::new("common_function".to_string()));
            
            // Add unique function
            node.functions.push(FunctionSignature::new(format!("unique_func_{}", i)));
            
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