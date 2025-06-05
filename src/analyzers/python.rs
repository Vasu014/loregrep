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
pub struct PythonAnalyzer {
    language: Language,
}

impl PythonAnalyzer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            language: tree_sitter_python::language(),
        })
    }

    /// Parse parameter node into structured parameter
    fn parse_parameter(&self, param_node: &Node, source: &str) -> Result<Parameter> {
        let query_str = r#"
            (parameter
              name: (identifier) @param_name
              type: (_)? @param_type
              default_value: (_)? @default_value
            )
            (typed_parameter
              name: (identifier) @param_name
              type: (_) @param_type
            )
            (default_parameter
              name: (identifier) @param_name
              default_value: (_) @default_value
            )
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, *param_node, source.as_bytes());
        
        let mut param_name = String::new();
        let mut param_type = String::new();
        let mut default_value = String::new();
        
        for query_match in matches {
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = self.safe_utf8_text(&capture.node, source);
                
                match capture_name.as_str() {
                    "param_name" => param_name = text.to_string(),
                    "param_type" => param_type = text.to_string(),
                    "default_value" => default_value = text.to_string(),
                    _ => {}
                }
            }
        }
        
        // Fallback: try to get text directly if query didn't work
        if param_name.is_empty() {
            let node_text = self.safe_utf8_text(param_node, source);
            // Handle different Python parameter patterns
            if node_text.contains(':') {
                // Type annotated parameter: name: type = default
                let parts: Vec<&str> = node_text.split(':').collect();
                if let Some(name_part) = parts.first() {
                    param_name = name_part.trim().to_string();
                }
                if parts.len() > 1 {
                    let type_and_default = parts[1];
                    if let Some(equals_pos) = type_and_default.find('=') {
                        param_type = type_and_default[..equals_pos].trim().to_string();
                        default_value = type_and_default[equals_pos + 1..].trim().to_string();
                    } else {
                        param_type = type_and_default.trim().to_string();
                    }
                }
            } else if node_text.contains('=') {
                // Default parameter: name = default
                let parts: Vec<&str> = node_text.split('=').collect();
                if let Some(name_part) = parts.first() {
                    param_name = name_part.trim().to_string();
                }
                if parts.len() > 1 {
                    default_value = parts[1].trim().to_string();
                }
            } else {
                // Simple parameter: name
                param_name = node_text.trim().to_string();
            }
        }
        
        let mut param = Parameter::new(param_name, param_type);
        if !default_value.is_empty() {
            param = param.with_default(default_value);
        }
        
        Ok(param)
    }

    /// Extract decorators from a function
    fn extract_decorators(&self, function_node: &Node, source: &str) -> Vec<String> {
        let mut decorators = Vec::new();
        
        // Look for decorated_definition parent
        if let Some(parent) = function_node.parent() {
            if parent.kind() == "decorated_definition" {
                let mut child_cursor = parent.walk();
                if child_cursor.goto_first_child() {
                    loop {
                        let child = child_cursor.node();
                        if child.kind() == "decorator" {
                            let decorator_text = self.safe_utf8_text(&child, source);
                            if !decorator_text.is_empty() {
                                decorators.push(decorator_text.to_string());
                            }
                        }
                        if !child_cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
            }
        }
        
        decorators
    }

    /// Determine if a function is a method and what type
    fn analyze_method_type(&self, function_node: &Node, function_sig: &FunctionSignature) -> (bool, bool, bool) {
        let mut is_method = false;
        let mut is_static = false;
        let mut is_class_method = false;

        // Check if function is inside a class
        let mut current = function_node.parent();
        while let Some(node) = current {
            if node.kind() == "class_definition" {
                is_method = true;
                break;
            }
            current = node.parent();
        }

        if is_method {
            // Check for @staticmethod or @classmethod decorators
            let decorators = self.extract_decorators(function_node, "");
            for decorator in &decorators {
                if decorator.contains("staticmethod") {
                    is_static = true;
                    break;
                } else if decorator.contains("classmethod") {
                    is_class_method = true;
                    break;
                }
            }

            // If not static and first parameter is 'self', it's an instance method
            if !is_static && !is_class_method {
                if let Some(first_param) = function_sig.parameters.first() {
                    if first_param.name == "self" {
                        // Instance method - not static
                    } else if first_param.name == "cls" {
                        is_class_method = true;
                    }
                }
            }
        }

        (is_method, is_static, is_class_method)
    }

    /// Calculate content hash for caching
    fn calculate_content_hash(&self, content: &str) -> String {
        blake3::hash(content.as_bytes()).to_hex().to_string()
    }
    
    /// Safely extract UTF-8 text from a tree-sitter node
    fn safe_utf8_text(&self, node: &Node, source: &str) -> String {
        let source_bytes = source.as_bytes();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        
        // Bounds check to prevent panic
        if start_byte >= source_bytes.len() || end_byte > source_bytes.len() || start_byte > end_byte {
            return String::new();
        }
        
        // Additional safety: try the tree-sitter method first, with fallback
        match std::panic::catch_unwind(|| node.utf8_text(source_bytes)) {
            Ok(Ok(text)) => text.to_string(),
            Ok(Err(_)) | Err(_) => {
                // Fallback: manual byte slice extraction
                match std::str::from_utf8(&source_bytes[start_byte..end_byte]) {
                    Ok(text) => text.to_string(),
                    Err(_) => String::new(),
                }
            }
        }
    }
}

#[async_trait]
impl LanguageAnalyzer for PythonAnalyzer {
    fn language(&self) -> &'static str {
        "python"
    }
    
    fn file_extensions(&self) -> &[&'static str] {
        &["py", "pyx", "pyi"]
    }
    
    fn supports_async(&self) -> bool {
        true
    }
    
    async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis> {
        let start_time = Instant::now();
        let mut tree_node = TreeNode::new(file_path.to_string(), "python".to_string());
        
        // Calculate content hash
        tree_node.content_hash = self.calculate_content_hash(content);
        tree_node.last_modified = std::time::SystemTime::now();
        
        // Early validation - check for empty or invalid content
        if content.trim().is_empty() {
            tree_node.add_error("File is empty".to_string());
            let duration = start_time.elapsed().as_millis() as u64;
            return Ok(FileAnalysis::new(tree_node, duration));
        }
        
        // Parse with tree-sitter with comprehensive error handling
        let tree_result = std::panic::catch_unwind(|| {
            let mut parser = Parser::new();
            match parser.set_language(self.language) {
                Ok(_) => parser.parse(content, None),
                Err(_) => None,
            }
        });
        
        let tree = match tree_result {
            Ok(Some(tree)) => tree,
            Ok(None) => {
                // Failed to parse - use fallback
                tree_node.add_error("Tree-sitter parsing failed, using fallback".to_string());
                let fallback_analysis = self.extract_with_fallback(content, file_path);
                let mut fallback_tree = TreeNode::new(file_path.to_string(), "python".to_string());
                fallback_tree.functions = fallback_analysis.functions;
                fallback_tree.structs = fallback_analysis.structs;
                fallback_tree.imports = fallback_analysis.imports;
                fallback_tree.exports = fallback_analysis.exports;
                fallback_tree.parse_errors = fallback_analysis.errors;
                fallback_tree.content_hash = self.calculate_content_hash(content);
                fallback_tree.last_modified = std::time::SystemTime::now();
                return Ok(FileAnalysis::new(fallback_tree, start_time.elapsed().as_millis() as u64));
            },
            Err(_) => {
                // Panic occurred during parsing - use fallback
                tree_node.add_error("Tree-sitter parsing panicked, using fallback".to_string());
                let fallback_analysis = self.extract_with_fallback(content, file_path);
                let mut fallback_tree = TreeNode::new(file_path.to_string(), "python".to_string());
                fallback_tree.functions = fallback_analysis.functions;
                fallback_tree.structs = fallback_analysis.structs;
                fallback_tree.imports = fallback_analysis.imports;
                fallback_tree.exports = fallback_analysis.exports;
                fallback_tree.parse_errors = fallback_analysis.errors;
                fallback_tree.content_hash = self.calculate_content_hash(content);
                fallback_tree.last_modified = std::time::SystemTime::now();
                return Ok(FileAnalysis::new(fallback_tree, start_time.elapsed().as_millis() as u64));
            }
        };
        
        // Extract all components with panic protection
        match std::panic::catch_unwind(|| self.extract_functions(&tree, content, file_path)) {
            Ok(Ok(functions)) => tree_node.functions = functions,
            Ok(Err(e)) => tree_node.add_error(format!("Function extraction failed: {}", e)),
            Err(_) => tree_node.add_error("Function extraction panicked".to_string()),
        }
        
        match std::panic::catch_unwind(|| self.extract_structs(&tree, content, file_path)) {
            Ok(Ok(structs)) => tree_node.structs = structs,
            Ok(Err(e)) => tree_node.add_error(format!("Class extraction failed: {}", e)),
            Err(_) => tree_node.add_error("Class extraction panicked".to_string()),
        }
        
        match std::panic::catch_unwind(|| self.extract_imports(&tree, content, file_path)) {
            Ok(Ok(imports)) => tree_node.imports = imports,
            Ok(Err(e)) => tree_node.add_error(format!("Import extraction failed: {}", e)),
            Err(_) => tree_node.add_error("Import extraction panicked".to_string()),
        }
        
        match std::panic::catch_unwind(|| self.extract_exports(&tree, content, file_path)) {
            Ok(Ok(exports)) => tree_node.exports = exports,
            Ok(Err(e)) => tree_node.add_error(format!("Export extraction failed: {}", e)),
            Err(_) => tree_node.add_error("Export extraction panicked".to_string()),
        }
        
        match std::panic::catch_unwind(|| self.extract_function_calls(&tree, content, file_path)) {
            Ok(Ok(function_calls)) => tree_node.function_calls = function_calls,
            Ok(Err(e)) => tree_node.add_error(format!("Function call extraction failed: {}", e)),
            Err(_) => tree_node.add_error("Function call extraction panicked".to_string()),
        }
        
        let duration = start_time.elapsed().as_millis() as u64;
        Ok(FileAnalysis::new(tree_node, duration))
    }
    
    fn extract_functions(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<FunctionSignature>> {
        // Query for function definitions - simplified based on actual tree structure
        let query_str = r#"
            (function_definition
              name: (identifier) @name
              parameters: (parameters) @params
              return_type: (_)? @return_type
            ) @function
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        
        let mut functions = Vec::new();
        
        for query_match in matches {
            let mut function_sig = FunctionSignature::new(String::new(), file_path.to_string());
            let mut function_node: Option<Node> = None;
            let mut is_async = false;
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = self.safe_utf8_text(&capture.node, source);
                
                match capture_name.as_str() {
                    "name" => function_sig.name = text.to_string(),
                    "function" => {
                        function_node = Some(capture.node);
                        let start_point = capture.node.start_position();
                        let end_point = capture.node.end_position();
                        function_sig.start_line = start_point.row as u32 + 1;
                        function_sig.end_line = end_point.row as u32 + 1;
                        
                        // Check if this is an async function by looking at the function text
                        let function_text = self.safe_utf8_text(&capture.node, source);
                        is_async = function_text.starts_with("async ");
                    },
                    "params" => {
                        // Parse parameters
                        let mut param_cursor = capture.node.walk();
                        if param_cursor.goto_first_child() {
                            loop {
                                let param_child = param_cursor.node();
                                if param_child.kind() == "identifier" ||
                                   param_child.kind() == "typed_parameter" ||
                                   param_child.kind() == "default_parameter" ||
                                   param_child.kind() == "list_splat_pattern" ||
                                   param_child.kind() == "dictionary_splat_pattern" {
                                    
                                    match self.parse_parameter(&param_child, source) {
                                        Ok(param) => function_sig.parameters.push(param),
                                        Err(_) => {
                                            // Fallback: just get the text and parse it manually
                                            let param_text = self.safe_utf8_text(&param_child, source);
                                            if !param_text.is_empty() && param_text != "," {
                                                // Parse Python parameter format: name: type = default
                                                if param_text.contains(':') {
                                                    let parts: Vec<&str> = param_text.split(':').collect();
                                                    if let Some(name_part) = parts.first() {
                                                        let param_name = name_part.trim().to_string();
                                                        let param_type = if parts.len() > 1 {
                                                            let type_and_default = parts[1];
                                                            if let Some(equals_pos) = type_and_default.find('=') {
                                                                type_and_default[..equals_pos].trim().to_string()
                                                            } else {
                                                                type_and_default.trim().to_string()
                                                            }
                                                        } else {
                                                            String::new()
                                                        };
                                                        let param = Parameter::new(param_name, param_type);
                                                        function_sig.parameters.push(param);
                                                    }
                                                } else {
                                                    // Simple parameter name
                                                    let param = Parameter::new(param_text.to_string(), String::new());
                                                    function_sig.parameters.push(param);
                                                }
                                            }
                                        }
                                    }
                                }
                                if !param_cursor.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    },
                    "return_type" => {
                        function_sig.return_type = Some(text.to_string());
                    },
                    _ => {}
                }
            }
            
            // Set async flag
            function_sig.is_async = is_async;
            
            // Analyze method type and visibility
            if let Some(node) = function_node {
                let (is_method, is_static_method, is_class_method) = self.analyze_method_type(&node, &function_sig);
                
                if is_method {
                    function_sig.is_static = is_static_method;
                    // In Python, methods are "public" unless they start with underscore
                    function_sig.is_public = !function_sig.name.starts_with('_');
                } else {
                    // Module-level function
                    function_sig.is_public = !function_sig.name.starts_with('_');
                }
                
                // Note: Python doesn't have const or extern functions like Rust
                function_sig.is_const = false;
                function_sig.is_extern = false;
            }
            
            if !function_sig.name.is_empty() {
                functions.push(function_sig);
            }
        }
        
        Ok(functions)
    }
    
    fn extract_structs(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<StructSignature>> {
        // In Python, we extract class definitions instead of structs
        let query_str = r#"
            (class_definition
              name: (identifier) @name
              superclasses: (argument_list)? @inheritance
              body: (block) @body
            ) @class
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut classes = Vec::new();
        
        for query_match in matches {
            let mut class_sig = StructSignature::new(String::new(), file_path.to_string());
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = self.safe_utf8_text(&capture.node, source);
                
                match capture_name.as_str() {
                    "name" => class_sig.name = text.to_string(),
                    "inheritance" => {
                        // Extract base classes - simplified for now
                        class_sig.generics.push(format!("inherits: {}", text));
                    },
                    "body" => {
                        // Extract class attributes/methods as "fields"
                        let mut child_cursor = capture.node.walk();
                        if child_cursor.goto_first_child() {
                            loop {
                                let child = child_cursor.node();
                                // Look for assignment statements (class attributes)
                                if child.kind() == "expression_statement" {
                                    let child_text = self.safe_utf8_text(&child, source);
                                    if !child_text.is_empty() {
                                        if child_text.contains('=') && !child_text.trim().starts_with("def ") {
                                            // This looks like a class attribute
                                            let parts: Vec<&str> = child_text.split('=').collect();
                                            if let Some(attr_name) = parts.first() {
                                                let attr_name = attr_name.trim().to_string();
                                                let is_public = !attr_name.starts_with('_');
                                                let field = StructField::new(attr_name, "Any".to_string())
                                                    .with_visibility(is_public);
                                                class_sig.fields.push(field);
                                            }
                                        }
                                    }
                                }
                                if !child_cursor.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    },
                    "class" => {
                        let start_point = capture.node.start_position();
                        let end_point = capture.node.end_position();
                        class_sig.start_line = start_point.row as u32 + 1;
                        class_sig.end_line = end_point.row as u32 + 1;
                    },
                    _ => {}
                }
            }
            
            // Python classes are "public" unless they start with underscore
            class_sig.is_public = !class_sig.name.starts_with('_');
            
            if !class_sig.name.is_empty() {
                classes.push(class_sig);
            }
        }
        
        Ok(classes)
    }
    
    fn extract_imports(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<ImportStatement>> {
        let query_str = r#"
            (import_statement
              name: (dotted_name) @import_path
            ) @import
            (import_from_statement
              module_name: (dotted_name)? @module
              name: (_) @import_items
            ) @from_import
            (future_import_statement
              name: (_) @future_import
            ) @future
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut imports = Vec::new();
        
        for query_match in matches {
            let mut import_stmt = ImportStatement::new(String::new(), file_path.to_string());
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = self.safe_utf8_text(&capture.node, source);
                
                match capture_name.as_str() {
                    "import_path" => {
                        import_stmt.module_path = text.to_string();
                        // Python imports are external unless they start with . (relative)
                        import_stmt.is_external = !text.starts_with('.');
                    },
                    "module" => {
                        import_stmt.module_path = text.to_string();
                        import_stmt.is_external = !text.starts_with('.');
                    },
                    "import_items" => {
                        // Check for wildcard imports
                        import_stmt.is_glob = text.contains('*');
                        // For from imports, combine module and items
                        if !import_stmt.module_path.is_empty() {
                            import_stmt.module_path = format!("{}.{}", import_stmt.module_path, text);
                        } else {
                            import_stmt.module_path = text.to_string();
                        }
                    },
                    "future_import" => {
                        import_stmt.module_path = format!("__future__.{}", text);
                        import_stmt.is_external = true;
                    },
                    "import" | "from_import" | "future" => {
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
    
    fn extract_exports(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<ExportStatement>> {
        // Python doesn't have explicit exports like JavaScript/TypeScript
        // We consider public (non-underscore prefixed) module-level items as exports
        let query_str = r#"
            (function_definition name: (identifier) @func_name) @func
            (class_definition name: (identifier) @class_name) @class
            (assignment 
              left: (identifier) @var_name
            ) @variable
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut exports = Vec::new();
        
        for query_match in matches {
            let mut export_stmt = ExportStatement::new(String::new(), file_path.to_string());
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = self.safe_utf8_text(&capture.node, source);
                
                match capture_name.as_str() {
                    "func_name" | "class_name" | "var_name" => {
                        // Only consider public items (not starting with _) as exports
                        if !text.starts_with('_') {
                            export_stmt.exported_item = text.to_string();
                            export_stmt.is_public = true;
                        }
                    },
                    "func" | "class" | "variable" => {
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
    
    fn extract_function_calls(&self, tree: &Tree, source: &str, file_path: &str) -> Result<Vec<FunctionCall>> {
        let query_str = r#"
            (call
              function: (identifier) @function_name
            ) @call
            (call
              function: (attribute
                object: (_) @receiver
                attribute: (identifier) @method_name
              )
            ) @method_call
        "#;
        
        let query = Query::new(self.language, query_str)
            .map_err(|e| AnalysisError::QueryError { message: format!("{:?}", e) })?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut function_calls = Vec::new();
        
        for query_match in matches {
            let mut function_call = FunctionCall::new(String::new(), file_path.to_string(), 0);
            
            for capture in query_match.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let text = self.safe_utf8_text(&capture.node, source);
                
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
        let mut analysis = PartialAnalysis::new(file_path.to_string(), "python".to_string()).with_fallback();
        
        // Simple regex-based fallback parsing for Python
        
        // Try to extract function definitions with regex
        if let Ok(fn_regex) = Regex::new(r"(?m)^(\s*)(async\s+)?def\s+(\w+)\s*\(") {
            for caps in fn_regex.captures_iter(content) {
                if let Some(name_match) = caps.get(3) {
                    let mut func = FunctionSignature::new(name_match.as_str().to_string(), file_path.to_string());
                    func.is_async = caps.get(2).is_some();
                    func.is_public = !name_match.as_str().starts_with('_');
                    analysis.functions.push(func);
                }
            }
        } else {
            analysis.add_error("Failed to create function regex".to_string());
        }
        
        // Try to extract class definitions with regex
        if let Ok(class_regex) = Regex::new(r"(?m)^(\s*)class\s+(\w+)(\s*\([^)]*\))?:") {
            for caps in class_regex.captures_iter(content) {
                if let Some(name_match) = caps.get(2) {
                    let mut class_sig = StructSignature::new(name_match.as_str().to_string(), file_path.to_string());
                    class_sig.is_public = !name_match.as_str().starts_with('_');
                    analysis.structs.push(class_sig);
                }
            }
        } else {
            analysis.add_error("Failed to create class regex".to_string());
        }
        
        // Try to extract import statements
        if let Ok(import_regex) = Regex::new(r"(?m)^(from\s+[\w.]+\s+)?import\s+([\w.*,\s]+)") {
            for caps in import_regex.captures_iter(content) {
                if let Some(import_match) = caps.get(0) {
                    let import_stmt = ImportStatement::new(import_match.as_str().to_string(), file_path.to_string());
                    analysis.imports.push(import_stmt);
                }
            }
        } else {
            analysis.add_error("Failed to create import regex".to_string());
        }
        
        analysis
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_python_analyzer_basic() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        assert_eq!(analyzer.language(), "python");
        assert_eq!(analyzer.file_extensions(), &["py", "pyx", "pyi"]);
        assert!(analyzer.supports_async());
    }

    #[tokio::test]
    async fn test_debug_tree_sitter() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
def hello():
    print("Hello")
        "#;
        
        // Test direct tree-sitter parsing
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(analyzer.language).unwrap();
        let tree = parser.parse(code, None).unwrap();
        
        println!("Root node kind: {}", tree.root_node().kind());
        println!("Root node S-expression: {}", tree.root_node().to_sexp());
        
        // Test complex query like in extract_functions
        let complex_query = r#"
            (function_definition
              name: (identifier) @name
              parameters: (parameters) @params
            ) @function
        "#;
        println!("Testing complex query:");
        if let Ok(query) = tree_sitter::Query::new(analyzer.language, complex_query) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
            
            println!("Number of matches: {}", matches.count());
            
            // Re-run to actually process them
            let mut cursor = tree_sitter::QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
            
            for query_match in matches {
                for capture in query_match.captures {
                    let capture_name = &query.capture_names()[capture.index as usize];
                    let text = analyzer.safe_utf8_text(&capture.node, code);
                    println!("Complex Capture: {} = {}", capture_name, text);
                }
            }
        } else {
            println!("Complex query failed to parse");
        }
        
        // Try simpler query
        let simple_query = r#"(function_definition name: (identifier) @name) @function"#;
        println!("Testing simple query:");
        if let Ok(query) = tree_sitter::Query::new(analyzer.language, simple_query) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
            
            for query_match in matches {
                for capture in query_match.captures {
                    let capture_name = &query.capture_names()[capture.index as usize];
                    let text = analyzer.safe_utf8_text(&capture.node, code);
                    println!("Simple Capture: {} = {}", capture_name, text);
                }
            }
        } else {
            println!("Simple query failed to parse");
        }
    }

    #[tokio::test]
    async fn test_extract_simple_function() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
def hello_world():
    print("Hello, world!")
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "hello_world");
        assert!(functions[0].is_public); // Doesn't start with _
        assert!(!functions[0].is_async);
        assert!(!functions[0].is_static);
    }

    #[tokio::test]
    async fn test_extract_async_function() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
async def fetch_data(url: str) -> str:
    """Fetch data from URL"""
    return "data"
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "fetch_data");
        assert!(functions[0].is_public);
        assert!(functions[0].is_async);
        assert_eq!(functions[0].return_type, Some("str".to_string()));
        assert_eq!(functions[0].parameters.len(), 1);
        assert_eq!(functions[0].parameters[0].name, "url");
        assert_eq!(functions[0].parameters[0].param_type, "str");
    }

    #[tokio::test]
    async fn test_extract_private_function() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
def _private_function():
    pass

def __dunder_method__(self):
    pass
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 2);
        
        let private_fn = functions.iter().find(|f| f.name == "_private_function").unwrap();
        assert!(!private_fn.is_public);
        
        let dunder_fn = functions.iter().find(|f| f.name == "__dunder_method__").unwrap();
        assert!(!dunder_fn.is_public);
    }

    #[tokio::test]
    async fn test_extract_class() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
class User:
    def __init__(self, name: str):
        self.name = name
        self.email = ""
    
    def get_name(self) -> str:
        return self.name
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let classes = &analysis.tree_node.structs;
        let functions = &analysis.tree_node.functions;
        
        // Should find the class
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "User");
        assert!(classes[0].is_public);
        
        // Should find the methods
        assert_eq!(functions.len(), 2);
        let init_method = functions.iter().find(|f| f.name == "__init__").unwrap();
        assert!(!init_method.is_public); // Dunder methods are not public
        
        let get_name_method = functions.iter().find(|f| f.name == "get_name").unwrap();
        assert!(get_name_method.is_public);
    }

    #[tokio::test]
    async fn test_extract_imports() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
import os
import sys
from typing import List, Dict
from .relative_module import something
from ..parent_module import other
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let imports = &analysis.tree_node.imports;
        
        assert!(imports.len() >= 3);
        
        // Check for standard library imports
        let os_import = imports.iter().find(|i| i.module_path.contains("os"));
        assert!(os_import.is_some());
        
        // Check for relative imports
        let relative_import = imports.iter().find(|i| i.module_path.contains("relative_module"));
        if let Some(rel_import) = relative_import {
            assert!(!rel_import.is_external); // Relative imports are not external
        }
    }

    #[tokio::test]
    async fn test_extract_function_calls() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
def main():
    print("Hello")
    obj = SomeClass()
    result = obj.method_call()
    standalone_function()
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let function_calls = &analysis.tree_node.function_calls;
        
        assert!(!function_calls.is_empty());
        
        // Check for method calls
        let method_calls: Vec<&FunctionCall> = function_calls.iter()
            .filter(|fc| fc.is_method_call)
            .collect();
        
        // Should find at least one method call
        assert!(!method_calls.is_empty());
    }

    #[tokio::test]
    async fn test_function_with_parameters() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let code = r#"
def complex_function(a: int, b: str = "default", *args, **kwargs) -> bool:
    return True
        "#;
        
        let analysis = analyzer.analyze_file(code, "test.py").await.expect("Analysis failed");
        let functions = &analysis.tree_node.functions;
        
        assert_eq!(functions.len(), 1);
        let func = &functions[0];
        assert_eq!(func.name, "complex_function");
        assert_eq!(func.return_type, Some("bool".to_string()));
        
        // Should capture multiple parameters
        assert!(!func.parameters.is_empty());
    }

    #[tokio::test]
    async fn test_fallback_parsing() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        // Intentionally malformed Python code to trigger fallback
        let malformed_code = r#"
def valid_function():
    # This should be parsed normally
    pass

class InvalidSyntax
    # Missing colon - this might cause parsing issues
    pass

async def another_function(param: str) -> str:
    return param
        "#;
        
        let fallback_analysis = analyzer.extract_with_fallback(malformed_code, "test.py");
        
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
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let content1 = "def test(): pass";
        let content2 = "def test(): pass";
        let content3 = "def different(): pass";
        
        let hash1 = analyzer.calculate_content_hash(content1);
        let hash2 = analyzer.calculate_content_hash(content2);
        let hash3 = analyzer.calculate_content_hash(content3);
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert!(!hash1.is_empty());
    }

    #[tokio::test]
    async fn test_comprehensive_python_analysis() {
        let analyzer = PythonAnalyzer::new().expect("Failed to create PythonAnalyzer");
        
        let comprehensive_code = r#"
import asyncio
from typing import List, Optional

class DataProcessor:
    def __init__(self, name: str):
        self.name = name
        self._private_attr = None
    
    async def process_data(self, data: List[str]) -> Optional[str]:
        """Process a list of data strings."""
        result = await self._internal_process(data)
        return result
    
    def _internal_process(self, data: List[str]) -> str:
        return " ".join(data)

async def main():
    processor = DataProcessor("test")
    result = await processor.process_data(["hello", "world"])
    print(result)

if __name__ == "__main__":
    asyncio.run(main())
        "#;
        
        let analysis = analyzer.analyze_file(comprehensive_code, "processor.py").await.expect("Analysis failed");
        let tree_node = &analysis.tree_node;
        
        // Verify all components were extracted
        assert!(!tree_node.functions.is_empty());
        assert!(!tree_node.structs.is_empty());
        assert!(!tree_node.imports.is_empty());
        
        // Verify timing information
        assert!(analysis.analysis_duration_ms > 0);
        
        
        assert!(analysis.success);
        
        // Verify content hash was calculated
        assert!(!tree_node.content_hash.is_empty());
        
        // Check specific extractions
        let processor_class = tree_node.structs.iter().find(|s| s.name == "DataProcessor").unwrap();
        assert!(processor_class.is_public);
        
        let process_data_method = tree_node.functions.iter().find(|f| f.name == "process_data").unwrap();
        assert!(process_data_method.is_async);
        assert!(process_data_method.is_public);
        
        let main_function = tree_node.functions.iter().find(|f| f.name == "main").unwrap();
        assert!(main_function.is_async);
        assert!(main_function.is_public);
        
        let internal_method = tree_node.functions.iter().find(|f| f.name == "_internal_process").unwrap();
        assert!(!internal_method.is_public); // Private method
    }
}