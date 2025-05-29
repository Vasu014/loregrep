use tree_sitter::{Parser, Language,  Query, QueryCursor, Node};
use std::fs;
use std::error::Error;
use std::io;
use std::path::{Path};
use std::env;
use streaming_iterator::StreamingIteratorMut;
use serde::{Serialize, Deserialize};
use serde_json;


// The core objects for the tree-sitter parser
// The function signature object, contains all the information about the function
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionSignature {
    name: String,
    parameters: Vec<String>,
    return_type: String,
    is_async: bool,
    is_pub: bool,
    is_extern: bool,
    is_const: bool,
    is_static: bool,
}

// The struct signature object, contains all the information about the struct
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StructSignature {
    name: String,
    fields: Vec<String>, // field_name: field_type
    is_pub: bool,
    is_tuple_struct: bool,
}

// The class signature object, contains all the information about the class
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClassSignature {
    name: String,
    methods: Vec<String>,
    is_pub: bool,
}


// The core output of this tree-sitter parser, contains all the information about the file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TreeNode {
    file_path: String,
    language: String,
    imports: Vec<String>,
    exports: Vec<String>,
    functions: Vec<FunctionSignature>,
    structs: Vec<StructSignature>
}

impl TreeNode {
    /// Convert to JSON string for easy display/storage
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Get summary stats for terminal display
    pub fn summary(&self) -> String {
        format!(
            "File: {} | Language: {} | Functions: {} | Structs: {} | Imports: {} | Exports: {}",
            self.file_path,
            self.language,
            self.functions.len(),
            self.structs.len(),
            self.imports.len(),
            self.exports.len()
        )
    }
    
    /// Get formatted function list for terminal display
    pub fn format_functions(&self) -> Vec<String> {
        self.functions.iter().map(|f| {
            let visibility = if f.is_pub { "pub " } else { "" };
            let async_keyword = if f.is_async { "async " } else { "" };
            let params = f.parameters.join(", ");
            format!("{}{}fn {}({}) -> {}", visibility, async_keyword, f.name, params, f.return_type)
        }).collect()
    }
    
    /// Get formatted struct list for terminal display
    pub fn format_structs(&self) -> Vec<String> {
        self.structs.iter().map(|s| {
            let visibility = if s.is_pub { "pub " } else { "" };
            let fields = s.fields.join(", ");
            format!("{}struct {} {{ {} }}", visibility, s.name, fields)
        }).collect()
    }
}


// The rust analyzer object, contains all the information about the rust file
struct RustAnalyzer {
    language: Language,

}

// The trait for the language analyzer, contains all the methods for extracting the information about the file
trait LanguageAnalyzer {
 fn extract_functions(&self, source: &String) -> Result<Vec<FunctionSignature>, io::Error>;
 fn extract_imports(&self, source: &String) -> Result<Vec<String>, io::Error>;
 fn extract_exports(&self, source: &String) -> Result<Vec<String>, io::Error>;
 fn extract_structs(&self, source: &String) -> Result<Vec<StructSignature>, io::Error>;
 fn extract_classes(&self, source: &String) -> Result<Vec<ClassSignature>, io::Error>;
}

impl LanguageAnalyzer for RustAnalyzer {
    fn extract_functions(&self, source: &String) -> Result<Vec<FunctionSignature>, io::Error> {
        let mut parser = Parser::new();
        parser.set_language(&self.language)?;
        
        let tree = parser.parse(source, None)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to parse"))?;
        
        // Query for function definitions
        let query_str = r#"
            (function_item
              "async"? @async_keyword
              (visibility_modifier)? @visibility
              name: (identifier) @name
              parameters: (parameters) @params
              return_type: (type_annotation) @return_type?
            )
        "#;
        
        let query = Query::new(&self.language, query_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Query error: {:?}", e)))?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut functions = Vec::new();
        for query_match in matches {
            let mut name = String::new();
            let mut is_pub = false;
            let mut is_async = false;
            let mut return_type = "()".to_string();
            let mut parameters = Vec::new();
            
            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name {
                    "name" => name = text.to_string(),
                    "visibility" => is_pub = text.contains("pub"),
                    "async_keyword" => is_async = text == "async",
                    "return_type" => return_type = text.trim_start_matches("->").trim().to_string(),
                    "params" => {
                        parameters = self.parse_parameters(&capture.node, source)?;
                    },
                    _ => {}
                }
            }
            
            if !name.is_empty() {
                functions.push(FunctionSignature {
                    name,
                    parameters,
                    return_type,
                    is_async,
                    is_pub,
                    is_extern: false,
                    is_const: false,
                    is_static: false,
                });
            }
        }
        
        Ok(functions)
    }
    
    fn extract_imports(&self, source: &String) -> Result<Vec<String>, io::Error> {
        let mut parser = Parser::new();
        parser.set_language(&self.language)?;
        
        let tree = parser.parse(source, None)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to parse"))?;
        
        // Query for use declarations
        let query_str = r#"
            (use_declaration
              argument: (_) @import_path
            )
        "#;
        
        let query = Query::new(&self.language, query_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Query error: {:?}", e)))?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut imports = Vec::new();
        for query_match in matches {
            for capture in query_match.captures {
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                imports.push(text.to_string());
            }
        }
        
        Ok(imports)
    }
    
    fn extract_exports(&self, source: &String) -> Result<Vec<String>, io::Error> {
        // For now, extract all pub items
        let mut parser = Parser::new();
        parser.set_language(&self.language)?;
        
        let tree = parser.parse(source, None)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to parse"))?;
        
        // Query for public items
        let query_str = r#"
            [
              (function_item (visibility_modifier) @vis name: (identifier) @name)
              (struct_item (visibility_modifier) @vis name: (type_identifier) @name)
              (enum_item (visibility_modifier) @vis name: (type_identifier) @name)
            ]
        "#;
        
        let query = Query::new(&self.language, query_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Query error: {:?}", e)))?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut exports = Vec::new();
        for query_match in matches {
            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                if capture_name == "name" {
                    let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                    exports.push(text.to_string());
                }
            }
        }
        
        Ok(exports)
    }
    
    fn extract_structs(&self, source: &String) -> Result<Vec<StructSignature>, io::Error> {
        let mut parser = Parser::new();
        parser.set_language(&self.language)?;
        
        let tree = parser.parse(source, None)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to parse"))?;
        
        // Query for struct definitions
        let query_str = r#"
            (struct_item
              (visibility_modifier)? @visibility
              name: (type_identifier) @name
              body: (field_declaration_list) @fields
            )
        "#;
        
        let query = Query::new(&self.language, query_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Query error: {:?}", e)))?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        
        let mut structs = Vec::new();
        for query_match in matches {
            let mut name = String::new();
            let mut is_pub = false;
            let mut fields = Vec::new();
            
            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name {
                    "name" => name = text.to_string(),
                    "visibility" => is_pub = text.contains("pub"),
                    "fields" => {
                        fields = self.parse_struct_fields(&capture.node, source)?;
                    },
                    _ => {}
                }
            }
            
            if !name.is_empty() {
                structs.push(StructSignature {
                    name,
                    fields,
                    is_pub,
                    is_tuple_struct: false, // TODO: Detect tuple structs
                });
            }
        }
        
        Ok(structs)
    }
    
    fn extract_classes(&self, source: &String) -> Result<Vec<ClassSignature>, io::Error> {
        // Rust doesn't have classes, return empty vec
        Ok(Vec::new())
    }
}

impl RustAnalyzer {
    fn parse_parameters(&self, params_node: &Node, source: &String) -> Result<Vec<String>, io::Error> {
        let mut parameters = Vec::new();
        
        // Query for individual parameters
        let query_str = r#"
            (parameter
              pattern: (identifier) @param_name
              type: (_) @param_type
            )
        "#;
        
        let query = Query::new(&self.language, query_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Parameter query error: {:?}", e)))?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, *params_node, source.as_bytes());
        
        for query_match in matches {
            let mut param_name = String::new();
            let mut param_type = String::new();
            
            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name {
                    "param_name" => param_name = text.to_string(),
                    "param_type" => param_type = text.to_string(),
                    _ => {}
                }
            }
            
            if !param_name.is_empty() && !param_type.is_empty() {
                parameters.push(format!("{}: {}", param_name, param_type));
            }
        }
        
        Ok(parameters)
    }
    
    fn parse_struct_fields(&self, fields_node: &Node, source: &String) -> Result<Vec<String>, io::Error> {
        let mut fields = Vec::new();
        
        // Query for struct fields
        let query_str = r#"
            (field_declaration
              (visibility_modifier)? @visibility
              name: (field_identifier) @field_name
              type: (_) @field_type
            )
        "#;
        
        let query = Query::new(&self.language, query_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Field query error: {:?}", e)))?;
        
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, *fields_node, source.as_bytes());
        
        for query_match in matches {
            let mut field_name = String::new();
            let mut field_type = String::new();
            let mut is_pub = false;
            
            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
                
                match capture_name {
                    "field_name" => field_name = text.to_string(),
                    "field_type" => field_type = text.to_string(),
                    "visibility" => is_pub = text.contains("pub"),
                    _ => {}
                }
            }
            
            if !field_name.is_empty() && !field_type.is_empty() {
                let visibility_prefix = if is_pub { "pub " } else { "" };
                fields.push(format!("{}{}: {}", visibility_prefix, field_name, field_type));
            }
        }
        
        Ok(fields)
    }
}


// The repo map object, contains all the information about the repository
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepoMap {
    files: Vec<TreeNode>,
    total_functions: usize,
    total_structs: usize,
    languages: Vec<String>,
}

impl RepoMap {
    pub fn new() -> Self {
        RepoMap {
            files: Vec::new(),
            total_functions: 0,
            total_structs: 0,
            languages: Vec::new(),
        }
    }
    
    pub fn add_file(&mut self, tree_node: TreeNode) {
        self.total_functions += tree_node.functions.len();
        self.total_structs += tree_node.structs.len();
        
        if !self.languages.contains(&tree_node.language) {
            self.languages.push(tree_node.language.clone());
        }
        
        self.files.push(tree_node);
    }
    
    /// Convert to JSON string for easy display/storage
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Get summary for terminal display
    pub fn summary(&self) -> String {
        format!(
            "Repository Map: {} files | {} functions | {} structs | Languages: [{}]",
            self.files.len(),
            self.total_functions,
            self.total_structs,
            self.languages.join(", ")
        )
    }
    
    /// Get files grouped by language
    pub fn files_by_language(&self) -> std::collections::HashMap<String, Vec<&TreeNode>> {
        let mut grouped = std::collections::HashMap::new();
        for file in &self.files {
            grouped.entry(file.language.clone()).or_insert_with(Vec::new).push(file);
        }
        grouped
    }
}

fn get_language(extension: &str) -> Result<Language, io::Error>{
    match extension{
        "rs" => Ok(tree_sitter_rust::LANGUAGE.into()),
        "py" => Ok(tree_sitter_python::LANGUAGE.into()),
        _ => Err(io::Error::new(io::ErrorKind::Other, "Unsupported file extension"))
    }
}

fn build_file_tree(file_path: &str) -> Result<TreeNode, io::Error>{
    let current_path = env::current_dir().expect("Failed to get current path");
    let full_path = current_path.join(file_path);
    println!("Full path: {:?}", full_path);
    let file_content = fs::read_to_string(&full_path)?;

    let file_extension = full_path.extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file extension"))?;
    
    let language = get_language(file_extension)?;

    // Create RustAnalyzer and extract all data
    let analyzer = RustAnalyzer { language };
    
    let functions = analyzer.extract_functions(&file_content)?;
    let imports = analyzer.extract_imports(&file_content)?;
    let exports = analyzer.extract_exports(&file_content)?;
    let structs = analyzer.extract_structs(&file_content)?;

    // Optional: Keep the SCM file processing for debugging
    if let Ok(scm_file) = fs::read_to_string(current_path.join("src/rust-tags.scm")) {
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let tree = parser.parse(&file_content, None).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Failed to parse file")
        })?;
        
        if let Ok(query) = Query::new(&language, &scm_file) {
            let mut cursor = QueryCursor::new();
            let mut captures = cursor.captures(&query, tree.root_node(), file_content.as_bytes());
            
            while let Some((query_match, index)) = captures.next_mut() {
                let capture = &query_match.captures[*index];
                let name = query.capture_names()[capture.index as usize];
                let node_text = capture.node.utf8_text(file_content.as_bytes()).unwrap_or("");
                println!("SCM Capture: {} => {}", name, node_text);
            }
        }
    }

    // Create properly populated TreeNode
    Ok(TreeNode {
        file_path: file_path.to_string(),
        language: file_extension.to_string(),
        imports,
        exports,
        functions,
        structs,
    })
}

fn main() {
    // Build file tree for a specific file
    match build_file_tree("src/tree-sitter.rs") {
        Ok(tree_node) => {
            // Print summary
            println!("📊 {}", tree_node.summary());
            println!();
            
            // Print functions
            if !tree_node.functions.is_empty() {
                println!("🔧 Functions:");
                for func in tree_node.format_functions() {
                    println!("  • {}", func);
                }
                println!();
            }
            
            // Print structs
            if !tree_node.structs.is_empty() {
                println!("🏗️  Structs:");
                for struc in tree_node.format_structs() {
                    println!("  • {}", struc);
                }
                println!();
            }
            
            // Print imports
            if !tree_node.imports.is_empty() {
                println!("📦 Imports:");
                for import in &tree_node.imports {
                    println!("  • {}", import);
                }
                println!();
            }
            
            // Print exports
            if !tree_node.exports.is_empty() {
                println!("📤 Exports:");
                for export in &tree_node.exports {
                    println!("  • {}", export);
                }
                println!();
            }
            
            // Demonstrate JSON serialization
            println!("📄 JSON Output:");
            match tree_node.to_json() {
                Ok(json) => println!("{}", json),
                Err(e) => println!("Error serializing to JSON: {}", e),
            }
            
            // Create a repo map example
            let mut repo_map = RepoMap::new();
            repo_map.add_file(tree_node);
            
            println!("\n🗺️  {}", repo_map.summary());
            
        },
        Err(e) => println!("❌ Error building file tree: {}", e),
    }
}
