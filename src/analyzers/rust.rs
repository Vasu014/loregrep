use async_trait::async_trait;
use tree_sitter::{Parser, Language, Query, QueryCursor, Node, Tree};
use std::time::Instant;
use blake3;
use regex::Regex;
use crate::analyzers::LanguageAnalyzer;
use crate::types::{
    Result, AnalysisError, FileAnalysis, PartialAnalysis, TreeNode,
    FunctionSignature, StructSignature, ImportStatement, ExportStatement, 
    FunctionCall, Parameter, StructField
};

#[derive(Clone)]
pub struct RustAnalyzer {
    language: Language,
}

impl RustAnalyzer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            language: tree_sitter_rust::language(),
        })
    }

    /// Parse parameter node into structured parameter
    fn parse_parameter(&self, param_node: &Node, source: &str) -> Result<Parameter> {
        // Query for parameter components
        let query_str = r#"
            (parameter
              pattern: (identifier) @param_name
              type: (_) @param_type
            )
            (parameter
              pattern: (mut_pattern (identifier) @mut_param_name)
              type: (_) @param_type
            )
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, *param_node, source.as_bytes());
        
        let mut param_name = String::new();
        let mut param_type = String::new();
        let mut is_mutable = false;
        
        for query_match in matches {
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "param_name" => param_name = text.to_string(),
                    "mut_param_name" => {
                        param_name = text.to_string();
                        is_mutable = true;
                    },
                    "param_type" => param_type = text.to_string(),
                    _ => {}
                }
            }
        }
        
        // Fallback: try to get text directly if query didn't work
        if param_name.is_empty() {
            let node_text = param_node.utf8_text(source.as_bytes()).unwrap_or("");
            if let Some((name_part, type_part)) = node_text.split_once(':') {
                param_name = name_part.trim().to_string();
                param_type = type_part.trim().to_string();
                is_mutable = param_name.starts_with("mut ");
                if is_mutable {
                    param_name = param_name.trim_start_matches("mut ").to_string();
                }
            }
        }
        
        Ok(Parameter::new(param_name, param_type).with_mutability(is_mutable))
    }

    /// Parse struct field node into structured field
    fn parse_struct_field(&self, field_node: &Node, source: &str) -> Result<StructField> {
        let query_str = r#"
            (field_declaration
              (visibility_modifier)? @visibility
              name: (field_identifier) @field_name
              type: (_) @field_type
            )
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, *field_node, source.as_bytes());
        
        let mut field_name = String::new();
        let mut field_type = String::new();
        let mut is_public = false;
        
        for query_match in matches {
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "field_name" => field_name = text.to_string(),
                    "field_type" => field_type = text.to_string(),
                    "visibility" => is_public = text.contains("pub"),
                    _ => {}
                }
            }
        }
        
        Ok(StructField::new(field_name, field_type).with_visibility(is_public))
    }

    /// Extract generics from a node
    fn extract_generics(&self, node: &Node, source: &str) -> Vec<String> {
        let query_str = r#"
            (type_parameters
              (type_parameter (type_identifier) @generic)
            )
        "#;
        
        if let Ok(query) = Query::new(self.language, query_str) {
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, *node, source.as_bytes());
            
            let mut generics = Vec::new();
            for query_match in matches {
                for capture in query_match.captures {
                    let capture_name = &query.capture_names()[capture.index as usize];
                    if capture_name == "generic" {
                        let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                        generics.push(text.to_string());
                    }
                }
            }
            generics
        } else {
            Vec::new()
        }
    }

    /// Calculate content hash for caching
    fn calculate_content_hash(&self, content: &str) -> String {
        blake3::hash(content.as_bytes()).to_hex().to_string()
    }
}

#[async_trait]
impl LanguageAnalyzer for RustAnalyzer {
    fn language(&self) -> &'static str {
        "rust"
    }
    
    fn file_extensions(&self) -> &[&'static str] {
        &["rs"]
    }
    
    fn supports_async(&self) -> bool {
        true
    }
    
    async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis> {
        let start_time = Instant::now();
        let mut tree_node = TreeNode::new(file_path.to_string(), "rust".to_string());
        
        // Calculate content hash
        tree_node.content_hash = self.calculate_content_hash(content);
        tree_node.last_modified = std::time::SystemTime::now();
        
        // Parse with tree-sitter
        let mut parser = Parser::new();
        parser.set_language(self.language)
            .map_err(|e| AnalysisError::ParseError { message: format!("Failed to set language: {:?}", e) })?;
        
        let tree = parser.parse(content, None)
            .ok_or_else(|| AnalysisError::ParseError { message: "Failed to parse file".to_string() })?;
        
        // Extract all components
        match self.extract_functions(&tree, content) {
            Ok(functions) => tree_node.functions = functions,
            Err(e) => tree_node.add_error(format!("Function extraction failed: {}", e)),
        }
        
        match self.extract_structs(&tree, content) {
            Ok(structs) => tree_node.structs = structs,
            Err(e) => tree_node.add_error(format!("Struct extraction failed: {}", e)),
        }
        
        match self.extract_imports(&tree, content) {
            Ok(imports) => tree_node.imports = imports,
            Err(e) => tree_node.add_error(format!("Import extraction failed: {}", e)),
        }
        
        match self.extract_exports(&tree, content) {
            Ok(exports) => tree_node.exports = exports,
            Err(e) => tree_node.add_error(format!("Export extraction failed: {}", e)),
        }
        
        match self.extract_function_calls(&tree, content) {
            Ok(function_calls) => tree_node.function_calls = function_calls,
            Err(e) => tree_node.add_error(format!("Function call extraction failed: {}", e)),
        }
        
        let duration = start_time.elapsed().as_millis() as u64;
        Ok(FileAnalysis::new(tree_node, duration))
    }
    
    fn extract_functions(&self, tree: &Tree, source: &str) -> Result<Vec<FunctionSignature>> {
        // Use the simple query that we know works
        let query_str = r#"(function_item name: (identifier) @name) @function"#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut functions = Vec::new();
        
        for query_match in matches {
            let mut function_sig = FunctionSignature::new(String::new());
            let mut function_node: Option<Node> = None;
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "name" => function_sig.name = text.to_string(),
                    "function" => {
                        function_node = Some(capture.node);
                        let start_point = capture.node.start_position();
                        let end_point = capture.node.end_position();
                        function_sig.start_line = start_point.row as u32 + 1;
                        function_sig.end_line = end_point.row as u32 + 1;
                        
                        // Extract function properties from the full function text
                        let function_text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                        
                        // Check for visibility
                        function_sig.is_public = function_text.contains("pub ");
                        
                        // Check for async
                        function_sig.is_async = function_text.contains("async ");
                        
                        // Check for const
                        function_sig.is_const = function_text.contains("const ");
                        
                        // Check for extern
                        function_sig.is_extern = function_text.contains("extern ");
                        
                        // Extract return type (simple pattern matching)
                        if let Some(arrow_pos) = function_text.find(" -> ") {
                            let after_arrow = &function_text[arrow_pos + 4..];
                            if let Some(brace_pos) = after_arrow.find(" {") {
                                let return_type = after_arrow[..brace_pos].trim();
                                function_sig.return_type = Some(return_type.to_string());
                            } else if let Some(semicolon_pos) = after_arrow.find(";") {
                                let return_type = after_arrow[..semicolon_pos].trim();
                                function_sig.return_type = Some(return_type.to_string());
                            }
                        }
                        
                        // Extract parameters (walk the AST)
                        let mut child_cursor = capture.node.walk();
                        if child_cursor.goto_first_child() {
                            loop {
                                let child = child_cursor.node();
                                if child.kind() == "parameters" {
                                    // Parse parameters
                                    let mut param_cursor = child.walk();
                                    if param_cursor.goto_first_child() {
                                        loop {
                                            let param_child = param_cursor.node();
                                            if param_child.kind() == "parameter" {
                                                let param_text = param_child.utf8_text(source.as_bytes()).unwrap_or("");
                                                if let Some((name_part, type_part)) = param_text.split_once(':') {
                                                    let param_name = name_part.trim().to_string();
                                                    let param_type = type_part.trim().to_string();
                                                    let is_mutable = param_name.starts_with("mut ");
                                                    let clean_name = if is_mutable {
                                                        param_name.trim_start_matches("mut ").to_string()
                                                    } else {
                                                        param_name
                                                    };
                                                    
                                                    let param = Parameter::new(clean_name, param_type)
                                                        .with_mutability(is_mutable);
                                                    function_sig.parameters.push(param);
                                                }
                                            }
                                            if !param_cursor.goto_next_sibling() {
                                                break;
                                            }
                                        }
                                    }
                                    break;
                                }
                                if !child_cursor.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    },
                    _ => {}
                }
            }
            
            // Check for static functions (associated functions in impl blocks)
            if let Some(node) = function_node {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "impl_item" {
                        // Check if first parameter is self
                        let has_self = function_sig.parameters.first()
                            .map(|p| p.name == "self" || p.name == "&self" || p.name == "&mut self")
                            .unwrap_or(false);
                        function_sig.is_static = !has_self;
                    }
                }
            }
            
            if !function_sig.name.is_empty() {
                functions.push(function_sig);
            }
        }
        
        Ok(functions)
    }
    
    fn extract_structs(&self, tree: &Tree, source: &str) -> Result<Vec<StructSignature>> {
        let query_str = r#"
            (struct_item
              (visibility_modifier)? @visibility
              name: (type_identifier) @name
              (type_parameters)? @generics
              body: (field_declaration_list) @fields
            ) @struct
            
            (struct_item
              (visibility_modifier)? @tuple_visibility
              name: (type_identifier) @tuple_name
              (type_parameters)? @tuple_generics
              body: (ordered_field_declaration_list) @tuple_fields
            ) @tuple_struct
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut structs = Vec::new();
        
        for query_match in matches {
            let mut struct_sig = StructSignature::new(String::new());
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "name" | "tuple_name" => struct_sig.name = text.to_string(),
                    "visibility" | "tuple_visibility" => struct_sig.is_public = text.contains("pub"),
                    "generics" | "tuple_generics" => {
                        struct_sig.generics = self.extract_generics(&capture.node, source);
                    },
                    "fields" => {
                        // Parse named fields
                        let mut child_cursor = capture.node.walk();
                        if child_cursor.goto_first_child() {
                            loop {
                                let child = child_cursor.node();
                                if child.kind() == "field_declaration" {
                                    if let Ok(field) = self.parse_struct_field(&child, source) {
                                        struct_sig.fields.push(field);
                                    }
                                }
                                if !child_cursor.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    },
                    "tuple_fields" => {
                        struct_sig.is_tuple_struct = true;
                        // Parse tuple fields
                        let mut child_cursor = capture.node.walk();
                        let mut field_index = 0;
                        if child_cursor.goto_first_child() {
                            loop {
                                let child = child_cursor.node();
                                if child.kind() == "ordered_field_declaration" {
                                    let field_text = child.utf8_text(source.as_bytes()).unwrap_or("");
                                    let is_public = field_text.contains("pub");
                                    // Extract type (remove pub if present)
                                    let field_type = field_text.trim_start_matches("pub").trim().to_string();
                                    
                                    let field = StructField::new(field_index.to_string(), field_type)
                                        .with_visibility(is_public);
                                    struct_sig.fields.push(field);
                                    field_index += 1;
                                }
                                if !child_cursor.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    },
                    "struct" | "tuple_struct" => {
                        let start_point = capture.node.start_position();
                        let end_point = capture.node.end_position();
                        struct_sig.start_line = start_point.row as u32 + 1;
                        struct_sig.end_line = end_point.row as u32 + 1;
                    },
                    _ => {}
                }
            }
            
            if !struct_sig.name.is_empty() {
                structs.push(struct_sig);
            }
        }
        
        Ok(structs)
    }
    
    fn extract_imports(&self, tree: &Tree, source: &str) -> Result<Vec<ImportStatement>> {
        let query_str = r#"
            (use_declaration
              argument: (_) @import_path
            ) @use_stmt
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut imports = Vec::new();
        
        for query_match in matches {
            let mut import_stmt = ImportStatement::new(String::new());
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "import_path" => {
                        import_stmt.module_path = text.to_string();
                        // Check if it's external (starts with crate name or std)
                        import_stmt.is_external = !text.starts_with("crate::") && 
                                                 !text.starts_with("self::") && 
                                                 !text.starts_with("super::");
                        // Check for glob imports
                        import_stmt.is_glob = text.contains("*");
                    },
                    "use_stmt" => {
                        let start_point = capture.node.start_position();
                        import_stmt.line_number = start_point.row as u32 + 1;
                    },
                    _ => {}
                }
            }
            
            if !import_stmt.module_path.is_empty() {
                imports.push(import_stmt);
            }
        }
        
        Ok(imports)
    }
    
    fn extract_exports(&self, tree: &Tree, source: &str) -> Result<Vec<ExportStatement>> {
        // In Rust, exports are public items
        let query_str = r#"
            [
              (function_item (visibility_modifier) @vis name: (identifier) @name)
              (struct_item (visibility_modifier) @vis name: (type_identifier) @name)
              (enum_item (visibility_modifier) @vis name: (type_identifier) @name)
              (type_item (visibility_modifier) @vis name: (type_identifier) @name)
              (const_item (visibility_modifier) @vis name: (identifier) @name)
              (static_item (visibility_modifier) @vis name: (identifier) @name)
              (mod_item (visibility_modifier) @vis name: (identifier) @name)
            ] @export_item
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut exports = Vec::new();
        
        for query_match in matches {
            let mut export_stmt = ExportStatement::new(String::new());
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "name" => export_stmt.exported_item = text.to_string(),
                    "vis" => export_stmt.is_public = text.contains("pub"),
                    "export_item" => {
                        let start_point = capture.node.start_position();
                        export_stmt.line_number = start_point.row as u32 + 1;
                    },
                    _ => {}
                }
            }
            
            if !export_stmt.exported_item.is_empty() && export_stmt.is_public {
                exports.push(export_stmt);
            }
        }
        
        Ok(exports)
    }
    
    fn extract_function_calls(&self, tree: &Tree, source: &str) -> Result<Vec<FunctionCall>> {
        let query_str = r#"
            (call_expression
              function: (identifier) @function_name
            ) @call
            
            (call_expression
              function: (field_expression
                value: (_) @receiver
                field: (field_identifier) @method_name
              )
            ) @method_call
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut function_calls = Vec::new();
        
        for query_match in matches {
            let mut function_call = FunctionCall::new(String::new(), String::new(), 0);
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name.as_str() {
                    "function_name" => {
                        function_call.function_name = text.to_string();
                    },
                    "method_name" => {
                        function_call.function_name = text.to_string();
                    },
                    "receiver" => {
                        function_call = function_call.with_method_call(text.to_string());
                    },
                    "call" | "method_call" => {
                        let start_point = capture.node.start_position();
                        function_call.line_number = start_point.row as u32 + 1;
                        function_call.column = start_point.column as u32;
                    },
                    _ => {}
                }
            }
            
            if !function_call.function_name.is_empty() {
                function_calls.push(function_call);
            }
        }
        
        Ok(function_calls)
    }
    
    fn extract_with_fallback(&self, content: &str, file_path: &str) -> PartialAnalysis {
        let mut analysis = PartialAnalysis::new(file_path.to_string(), "rust".to_string()).with_fallback();
        
        // Simple regex-based fallback parsing
        
        // Try to extract function signatures with regex
        if let Ok(fn_regex) = Regex::new(r"(?m)^\s*(pub\s+)?(const\s+)?(async\s+)?fn\s+(\w+)") {
            for caps in fn_regex.captures_iter(content) {
                if let Some(name_match) = caps.get(4) {
                    let mut func = FunctionSignature::new(name_match.as_str().to_string());
                    func.is_public = caps.get(1).is_some();
                    func.is_const = caps.get(2).is_some();
                    func.is_async = caps.get(3).is_some();
                    analysis.functions.push(func);
                }
            }
        } else {
            analysis.add_error("Failed to create function regex".to_string());
        }
        
        // Try to extract struct signatures with regex
        if let Ok(struct_regex) = Regex::new(r"(?m)^\s*(pub\s+)?struct\s+(\w+)") {
            for caps in struct_regex.captures_iter(content) {
                if let Some(name_match) = caps.get(2) {
                    let mut struct_sig = StructSignature::new(name_match.as_str().to_string());
                    struct_sig.is_public = caps.get(1).is_some();
                    analysis.structs.push(struct_sig);
                }
            }
        } else {
            analysis.add_error("Failed to create struct regex".to_string());
        }
        
        analysis
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_debug_tree_sitter() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
fn hello() {
    println!("Hello");
}
        "#;
        
        // Test direct tree-sitter parsing
        let mut parser = Parser::new();
        parser.set_language(analyzer.language).unwrap();
        let tree = parser.parse(code, None).unwrap();
        
        println!("Root node kind: {}", tree.root_node().kind());
        println!("Root node S-expression: {}", tree.root_node().to_sexp());
        
        // Try simpler query
        let simple_query = r#"(function_item name: (identifier) @name)"#;
        if let Ok(query) = Query::new(analyzer.language, simple_query) {
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
            
            for query_match in matches {
                for capture in query_match.captures {
                    let capture_name = &query.capture_names()[capture.index as usize];
                    let text = capture.node.utf8_text(code.as_bytes()).unwrap_or("");
                    println!("Capture: {} = {}", capture_name, text);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_rust_analyzer_basic() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        assert_eq!(analyzer.language(), "rust");
        assert_eq!(analyzer.file_extensions(), &["rs"]);
        assert!(analyzer.supports_async());
    }

    #[tokio::test]
    async fn test_extract_simple_function() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
fn hello_world() {
    println!("Hello, world!");
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        // Debug output
        println!("Found {} functions", functions.len());
        for (i, func) in functions.iter().enumerate() {
            println!("Function {}: {}", i, func.name);
        }
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "hello_world");
        assert!(!functions[0].is_public);
        assert!(!functions[0].is_async);
        assert!(!functions[0].is_const);
        assert!(!functions[0].is_extern);
        assert!(!functions[0].is_static);
    }

    #[tokio::test]
    async fn test_extract_pub_async_function() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
pub async fn fetch_data(url: &str) -> Result<String, Error> {
    // fetch implementation
    Ok("data".to_string())
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "fetch_data");
        assert!(functions[0].is_public);
        assert!(functions[0].is_async);
        assert!(!functions[0].is_const);
        assert_eq!(functions[0].return_type, Some("Result<String, Error>".to_string()));
        assert_eq!(functions[0].parameters.len(), 1);
        assert_eq!(functions[0].parameters[0].name, "url");
        assert_eq!(functions[0].parameters[0].param_type, "&str");
    }

    #[tokio::test]
    async fn test_extract_const_function() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
pub const fn square(x: i32) -> i32 {
    x * x
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "square");
        assert!(functions[0].is_public);
        assert!(functions[0].is_const);
        assert!(!functions[0].is_async);
        assert_eq!(functions[0].return_type, Some("i32".to_string()));
    }

    #[tokio::test]
    async fn test_extract_extern_function() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
extern fn c_function(x: i32) -> i32;
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "c_function");
        assert!(functions[0].is_extern);
        assert!(!functions[0].is_public);
    }

    #[tokio::test]
    async fn test_extract_function_with_generics() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
fn generic_function<T, U>(item: T, other: U) -> T 
where 
    T: Clone,
    U: Debug,
{
    item.clone()
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "generic_function");
        assert_eq!(functions[0].generics.len(), 2);
        assert!(functions[0].generics.contains(&"T".to_string()));
        assert!(functions[0].generics.contains(&"U".to_string()));
    }

    #[tokio::test]
    async fn test_extract_impl_methods() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    
    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 2);
        
        // Check static method (new)
        let new_fn = functions.iter().find(|f| f.name == "new").unwrap();
        assert!(new_fn.is_public);
        assert!(new_fn.is_static); // No self parameter
        
        // Check instance method (distance)
        let distance_fn = functions.iter().find(|f| f.name == "distance").unwrap();
        assert!(distance_fn.is_public);
        assert!(!distance_fn.is_static); // Has self parameter
    }

    #[tokio::test]
    async fn test_extract_struct() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
pub struct User {
    pub id: u64,
    pub name: String,
    email: String,
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let structs = &analysis.tree_node.structs;
        
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "User");
        assert!(structs[0].is_public);
        assert!(!structs[0].is_tuple_struct);
        assert_eq!(structs[0].fields.len(), 3);
        
        // Check field visibility
        let id_field = structs[0].fields.iter().find(|f| f.name == "id").unwrap();
        assert!(id_field.is_public);
        
        let email_field = structs[0].fields.iter().find(|f| f.name == "email").unwrap();
        assert!(!email_field.is_public);
    }

    #[tokio::test]
    async fn test_extract_tuple_struct() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
pub struct Point(pub f64, f64);
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let structs = &analysis.tree_node.structs;
        
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "Point");
        assert!(structs[0].is_public);
        assert!(structs[0].is_tuple_struct);
        assert_eq!(structs[0].fields.len(), 2);
    }

    #[tokio::test]
    async fn test_extract_imports() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
use std::collections::HashMap;
use crate::module::LocalType;
use super::parent_module::*;
use self::child_module::ChildType;
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let imports = &analysis.tree_node.imports;
        
        assert_eq!(imports.len(), 4);
        
        // Check external import
        let std_import = imports.iter().find(|i| i.module_path.contains("std")).unwrap();
        assert!(std_import.is_external);
        assert!(!std_import.is_glob);
        
        // Check crate import
        let crate_import = imports.iter().find(|i| i.module_path.contains("crate")).unwrap();
        assert!(!crate_import.is_external);
        
        // Check glob import
        let glob_import = imports.iter().find(|i| i.module_path.contains("*")).unwrap();
        assert!(glob_import.is_glob);
    }

    #[tokio::test]
    async fn test_extract_exports() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
pub fn public_function() {}
fn private_function() {}
pub struct PublicStruct {}
struct PrivateStruct {}
pub const PUBLIC_CONST: i32 = 42;
const PRIVATE_CONST: i32 = 24;
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let exports = &analysis.tree_node.exports;
        
        // Should only capture public items
        assert_eq!(exports.len(), 3);
        
        let exported_names: Vec<&String> = exports.iter().map(|e| &e.exported_item).collect();
        assert!(exported_names.contains(&&"public_function".to_string()));
        assert!(exported_names.contains(&&"PublicStruct".to_string()));
        assert!(exported_names.contains(&&"PUBLIC_CONST".to_string()));
    }

    #[tokio::test]
    async fn test_extract_function_calls() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let code = r#"
fn main() {
    println!("Hello");
    let point = Point::new(1.0, 2.0);
    let distance = point.distance_to_origin();
}
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.rs").await.expect("Analysis failed");
        let function_calls = &analysis.tree_node.function_calls;
        
        assert!(!function_calls.is_empty());
        
        // Check for method calls
        let method_calls: Vec<&FunctionCall> = function_calls.iter()
            .filter(|fc| fc.is_method_call)
            .collect();
        
        assert!(!method_calls.is_empty());
    }

    #[tokio::test]
    async fn test_fallback_parsing() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        // Intentionally malformed Rust code to trigger fallback
        let malformed_code = r#"
pub fn valid_function() {
    // This should be parsed normally
}

struct InvalidSyntax {
    pub field: String
    // Missing comma - this might cause parsing issues
}

pub async fn another_function(param: &str) -> Result<(), Error> {
    Ok(())
}
        "#;
        
        let fallback_analysis = analyzer.extract_with_fallback(malformed_code, "test.rs");
        
        assert!(fallback_analysis.fallback_used);
        assert!(!fallback_analysis.functions.is_empty());
        
        // Should find at least the valid functions via regex
        let function_names: Vec<&String> = fallback_analysis.functions.iter()
            .map(|f| &f.name)
            .collect();
        
        assert!(function_names.contains(&&"valid_function".to_string()));
        assert!(function_names.contains(&&"another_function".to_string()));
    }

    #[test]
    fn test_content_hash() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let content1 = "fn test() {}";
        let content2 = "fn test() {}";
        let content3 = "fn different() {}";
        
        let hash1 = analyzer.calculate_content_hash(content1);
        let hash2 = analyzer.calculate_content_hash(content2);
        let hash3 = analyzer.calculate_content_hash(content3);
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert!(!hash1.is_empty());
    }

    #[tokio::test]
    async fn test_comprehensive_analysis() {
        let analyzer = RustAnalyzer::new().expect("Failed to create RustAnalyzer");
        
        let comprehensive_code = r#"
use std::collections::HashMap;
use crate::types::*;

pub struct User<T> {
    pub id: u64,
    name: String,
    data: T,
}

impl<T> User<T> {
    pub fn new(id: u64, name: String, data: T) -> Self {
        Self { id, name, data }
    }
    
    pub async fn save(&self) -> Result<(), Error> {
        database_save(self).await
    }
    
    const fn get_id(&self) -> u64 {
        self.id
    }
}

pub async fn create_user<T>(name: String, data: T) -> User<T> {
    let id = generate_id().await;
    User::new(id, name, data)
}

extern fn generate_id() -> u64;

pub const MAX_USERS: usize = 1000;
        "#;
        
        let analysis = analyzer.analyze_file(comprehensive_code, "user.rs").await.expect("Analysis failed");
        let tree_node = &analysis.tree_node;
        
        // Debug output
        println!("Functions found: {}", tree_node.functions.len());
        for func in &tree_node.functions {
            println!("  - {}", func.name);
        }
        
        // Verify all components were extracted
        assert!(!tree_node.functions.is_empty());
        assert!(!tree_node.structs.is_empty());
        assert!(!tree_node.imports.is_empty());
        assert!(!tree_node.exports.is_empty());
        
        // Verify timing information
        assert!(analysis.analysis_duration_ms > 0);
        assert!(analysis.success);
        
        // Verify content hash was calculated
        assert!(!tree_node.content_hash.is_empty());
        
        // Check specific extractions
        let user_struct = tree_node.structs.iter().find(|s| s.name == "User").unwrap();
        assert!(user_struct.is_public);
        assert_eq!(user_struct.generics.len(), 1);
        
        let new_function = tree_node.functions.iter().find(|f| f.name == "new").unwrap();
        assert!(new_function.is_static);
        
        let save_function = tree_node.functions.iter().find(|f| f.name == "save").unwrap();
        assert!(save_function.is_async);
        assert!(!save_function.is_static);
        
        let get_id_function = tree_node.functions.iter().find(|f| f.name == "get_id").unwrap();
        assert!(get_id_function.is_const);
        
        let generate_id_function = tree_node.functions.iter().find(|f| f.name == "generate_id").unwrap();
        assert!(generate_id_function.is_extern);
    }
}
