use crate::analyzers::LanguageAnalyzer;
use crate::types::{
    AnalysisError, ExportStatement, FileAnalysis, FunctionCall, FunctionSignature, ImportStatement,
    Parameter, PartialAnalysis, Result, StructField, StructSignature, TreeNode,
};
use async_trait::async_trait;
use blake3;
use regex::Regex;
use std::time::Instant;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

/// Analyzer for TypeScript and TSX sources.
///
/// It holds both grammars from `tree-sitter-typescript` and selects between
/// them based on the file extension: `.tsx` uses the TSX grammar (which
/// understands JSX), while `.ts` (and anything else) uses the plain TypeScript
/// grammar. Plain JavaScript/JSX is intentionally out of scope.
#[derive(Clone)]
pub struct TypeScriptAnalyzer {
    typescript: Language,
    tsx: Language,
}

impl TypeScriptAnalyzer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            typescript: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tsx: tree_sitter_typescript::LANGUAGE_TSX.into(),
        })
    }

    /// Pick the grammar to use for a given file path. `.tsx` files are parsed
    /// with the TSX grammar; everything else uses the TypeScript grammar.
    fn language_for_path(&self, file_path: &str) -> &Language {
        let is_tsx = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("tsx"))
            .unwrap_or(false);
        if is_tsx { &self.tsx } else { &self.typescript }
    }

    /// Calculate content hash for caching
    fn calculate_content_hash(&self, content: &str) -> String {
        blake3::hash(content.as_bytes()).to_hex().to_string()
    }

    /// Safely extract UTF-8 text from a tree-sitter node
    fn text(&self, node: &Node, source: &str) -> String {
        let source_bytes = source.as_bytes();
        let start = node.start_byte();
        let end = node.end_byte();
        if start >= source_bytes.len() || end > source_bytes.len() || start > end {
            return String::new();
        }
        match std::str::from_utf8(&source_bytes[start..end]) {
            Ok(t) => t.to_string(),
            Err(_) => String::new(),
        }
    }

    /// Does `node` have a direct child (named or anonymous) with the given kind?
    /// Used to detect keyword tokens such as `async` and `static`, which are
    /// anonymous nodes in the tree-sitter-typescript grammar.
    fn has_token(node: &Node, kind: &str) -> bool {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                if cursor.node().kind() == kind {
                    return true;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        false
    }

    /// Is `node` nested (directly or a couple of levels up) inside an
    /// `export_statement`? Used to decide public visibility of top-level items.
    fn is_exported(node: &Node) -> bool {
        let mut current = node.parent();
        let mut depth = 0;
        while let Some(n) = current {
            if n.kind() == "export_statement" {
                return true;
            }
            if depth >= 3 {
                break;
            }
            depth += 1;
            current = n.parent();
        }
        false
    }

    /// Extract generic type parameter names from a node's `type_parameters`
    /// field (e.g. `<T, U>` -> ["T", "U"]).
    fn extract_type_params(&self, node: &Node, source: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(tp) = node.child_by_field_name("type_parameters") {
            let mut cursor = tp.walk();
            if cursor.goto_first_child() {
                loop {
                    let ch = cursor.node();
                    if ch.kind() == "type_parameter" {
                        if let Some(name) = ch.child_by_field_name("name") {
                            out.push(self.text(&name, source));
                        }
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        out
    }

    /// Read the type text out of a `type_annotation` node (the first named
    /// child, i.e. the type after the `:`).
    fn type_annotation_text(&self, annotation: &Node, source: &str) -> String {
        annotation
            .named_child(0)
            .map(|t| self.text(&t, source))
            .unwrap_or_default()
    }

    /// Extract parameters (with type annotations and defaults) from a node that
    /// has a `parameters` field pointing at a `formal_parameters` node.
    fn extract_params(&self, sig_node: &Node, source: &str) -> Vec<Parameter> {
        let mut out = Vec::new();
        if let Some(params) = sig_node.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            if cursor.goto_first_child() {
                loop {
                    let p = cursor.node();
                    if p.kind() == "required_parameter" || p.kind() == "optional_parameter" {
                        let name = p
                            .child_by_field_name("pattern")
                            .map(|n| self.text(&n, source))
                            .unwrap_or_default();
                        let ptype = p
                            .child_by_field_name("type")
                            .map(|ta| self.type_annotation_text(&ta, source))
                            .unwrap_or_default();
                        let mut param = Parameter::new(name, ptype);
                        if let Some(v) = p.child_by_field_name("value") {
                            param = param.with_default(self.text(&v, source));
                        }
                        if !param.name.is_empty() {
                            out.push(param);
                        }
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        out
    }

    /// Extract the return type from a node's `return_type` (`type_annotation`)
    /// field.
    fn extract_return_type(&self, sig_node: &Node, source: &str) -> Option<String> {
        sig_node.child_by_field_name("return_type").map(|ta| {
            let t = self.type_annotation_text(&ta, source);
            if t.is_empty() {
                self.text(&ta, source)
            } else {
                t
            }
        })
    }
}

#[async_trait]
impl LanguageAnalyzer for TypeScriptAnalyzer {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["ts", "tsx"]
    }

    fn supports_async(&self) -> bool {
        true
    }

    async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis> {
        let start_time = Instant::now();
        let mut tree_node = TreeNode::new(file_path.to_string(), "typescript".to_string());
        tree_node.content_hash = self.calculate_content_hash(content);
        tree_node.last_modified = std::time::SystemTime::now();

        if content.trim().is_empty() {
            tree_node.add_error("File is empty".to_string());
            let duration = start_time.elapsed().as_millis() as u64;
            return Ok(FileAnalysis::new(tree_node, duration));
        }

        // Select the grammar based on the file extension so `.tsx` files parse
        // their JSX correctly.
        let language = self.language_for_path(file_path).clone();

        let tree_result = std::panic::catch_unwind(|| {
            let mut parser = Parser::new();
            match parser.set_language(&language) {
                Ok(_) => parser.parse(content, None),
                Err(_) => None,
            }
        });

        let build_fallback = |tree_node: &mut TreeNode, reason: &str| -> FileAnalysis {
            tree_node.add_error(reason.to_string());
            let fallback = self.extract_with_fallback(content, file_path);
            let mut fb = TreeNode::new(file_path.to_string(), "typescript".to_string());
            fb.functions = fallback.functions;
            fb.structs = fallback.structs;
            fb.imports = fallback.imports;
            fb.exports = fallback.exports;
            fb.parse_errors = fallback.errors;
            fb.content_hash = self.calculate_content_hash(content);
            fb.last_modified = std::time::SystemTime::now();
            FileAnalysis::new(fb, start_time.elapsed().as_millis() as u64)
        };

        let tree = match tree_result {
            Ok(Some(tree)) => tree,
            Ok(None) => {
                return Ok(build_fallback(
                    &mut tree_node,
                    "Tree-sitter parsing failed, using fallback",
                ));
            }
            Err(_) => {
                return Ok(build_fallback(
                    &mut tree_node,
                    "Tree-sitter parsing panicked, using fallback",
                ));
            }
        };

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

    fn extract_functions(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
    ) -> Result<Vec<FunctionSignature>> {
        // Three shapes of "function": top-level declarations, arrow functions
        // bound to a `const`/`let`/`var`, and class methods.
        let query_str = r#"
            (function_declaration name: (identifier) @name) @function
            (variable_declarator name: (identifier) @name value: (arrow_function) @arrow) @arrow_decl
            (method_definition name: (property_identifier) @name) @method
        "#;

        let query =
            Query::new(&tree.language(), query_str).map_err(|e| AnalysisError::QueryError {
                message: format!("{:?}", e),
            })?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut functions = Vec::new();

        while let Some(query_match) = matches.next() {
            let mut name = String::new();
            // `outer_node` gives the line span; `sig_node` is where parameters,
            // return type and async/static keywords live.
            let mut outer_node: Option<Node> = None;
            let mut sig_node: Option<Node> = None;
            let mut kind = "";

            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                match capture_name {
                    "name" => name = self.text(&capture.node, source),
                    "function" => {
                        outer_node = Some(capture.node);
                        sig_node = Some(capture.node);
                        kind = "function";
                    }
                    "arrow_decl" => outer_node = Some(capture.node),
                    "arrow" => {
                        sig_node = Some(capture.node);
                        kind = "arrow";
                    }
                    "method" => {
                        outer_node = Some(capture.node);
                        sig_node = Some(capture.node);
                        kind = "method";
                    }
                    _ => {}
                }
            }

            let (outer, sig) = match (outer_node, sig_node) {
                (Some(o), Some(s)) => (o, s),
                _ => continue,
            };
            if name.is_empty() {
                continue;
            }

            let mut func = FunctionSignature::new(name, file_path.to_string());
            func.start_line = outer.start_position().row as u32 + 1;
            func.end_line = outer.end_position().row as u32 + 1;
            func.parameters = self.extract_params(&sig, source);
            func.return_type = self.extract_return_type(&sig, source);
            func.generics = self.extract_type_params(&sig, source);
            func.is_async = Self::has_token(&sig, "async");

            match kind {
                "method" => {
                    func.is_static = Self::has_token(&sig, "static");
                    // Members are public by default in TypeScript; only
                    // `private`/`protected` reduce visibility.
                    let mut modifier = None;
                    let mut mc = sig.walk();
                    if mc.goto_first_child() {
                        loop {
                            if mc.node().kind() == "accessibility_modifier" {
                                modifier = Some(self.text(&mc.node(), source));
                                break;
                            }
                            if !mc.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                    func.is_public =
                        !matches!(modifier.as_deref(), Some("private") | Some("protected"));
                }
                _ => {
                    // Top-level function / arrow: public iff it is exported.
                    func.is_public = Self::is_exported(&outer);
                }
            }

            functions.push(func);
        }

        Ok(functions)
    }

    fn extract_structs(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
    ) -> Result<Vec<StructSignature>> {
        // Classes, interfaces and type aliases are all surfaced as "structs".
        let query_str = r#"
            (class_declaration name: (type_identifier) @name) @class
            (abstract_class_declaration name: (type_identifier) @name) @class
            (interface_declaration name: (type_identifier) @name) @interface
            (type_alias_declaration name: (type_identifier) @name) @type_alias
        "#;

        let query =
            Query::new(&tree.language(), query_str).map_err(|e| AnalysisError::QueryError {
                message: format!("{:?}", e),
            })?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut structs = Vec::new();

        while let Some(query_match) = matches.next() {
            let mut name = String::new();
            let mut node: Option<Node> = None;
            let mut kind = "";

            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                match capture_name {
                    "name" => name = self.text(&capture.node, source),
                    "class" => {
                        node = Some(capture.node);
                        kind = "class";
                    }
                    "interface" => {
                        node = Some(capture.node);
                        kind = "interface";
                    }
                    "type_alias" => {
                        node = Some(capture.node);
                        kind = "type_alias";
                    }
                    _ => {}
                }
            }

            let node = match node {
                Some(n) => n,
                None => continue,
            };
            if name.is_empty() {
                continue;
            }

            let mut sig = StructSignature::new(name, file_path.to_string());
            sig.start_line = node.start_position().row as u32 + 1;
            sig.end_line = node.end_position().row as u32 + 1;
            sig.generics = self.extract_type_params(&node, source);
            sig.is_public = Self::is_exported(&node);

            if let Some(body) = node.child_by_field_name("body") {
                let mut bc = body.walk();
                if bc.goto_first_child() {
                    loop {
                        let member = bc.node();
                        match member.kind() {
                            // Class field: `public x: number;`
                            "public_field_definition" => {
                                if let Some(fname) = member.child_by_field_name("name") {
                                    let field_name = self.text(&fname, source);
                                    let field_type = member
                                        .child_by_field_name("type")
                                        .map(|ta| self.type_annotation_text(&ta, source))
                                        .unwrap_or_default();
                                    let mut modifier = None;
                                    let mut fc = member.walk();
                                    if fc.goto_first_child() {
                                        loop {
                                            if fc.node().kind() == "accessibility_modifier" {
                                                modifier = Some(self.text(&fc.node(), source));
                                                break;
                                            }
                                            if !fc.goto_next_sibling() {
                                                break;
                                            }
                                        }
                                    }
                                    let is_public = !matches!(
                                        modifier.as_deref(),
                                        Some("private") | Some("protected")
                                    );
                                    sig.fields.push(
                                        StructField::new(field_name, field_type)
                                            .with_visibility(is_public),
                                    );
                                }
                            }
                            // Interface property: `x: number;`
                            "property_signature" => {
                                if let Some(fname) = member.child_by_field_name("name") {
                                    let field_name = self.text(&fname, source);
                                    let field_type = member
                                        .child_by_field_name("type")
                                        .map(|ta| self.type_annotation_text(&ta, source))
                                        .unwrap_or_default();
                                    sig.fields.push(
                                        StructField::new(field_name, field_type)
                                            .with_visibility(true),
                                    );
                                }
                            }
                            _ => {}
                        }
                        if !bc.goto_next_sibling() {
                            break;
                        }
                    }
                }
            }

            let _ = kind;
            structs.push(sig);
        }

        Ok(structs)
    }

    fn extract_imports(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
    ) -> Result<Vec<ImportStatement>> {
        let query_str = r#"
            (import_statement source: (string (string_fragment) @source)) @import
        "#;

        let query =
            Query::new(&tree.language(), query_str).map_err(|e| AnalysisError::QueryError {
                message: format!("{:?}", e),
            })?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut imports = Vec::new();

        while let Some(query_match) = matches.next() {
            let mut import_stmt = ImportStatement::new(String::new(), file_path.to_string());
            let mut import_node: Option<Node> = None;

            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                match capture_name {
                    "source" => {
                        let module = self.text(&capture.node, source);
                        // Relative module specifiers (`./x`, `../x`) are local.
                        import_stmt.is_external = !module.starts_with('.');
                        import_stmt.module_path = module;
                    }
                    "import" => import_node = Some(capture.node),
                    _ => {}
                }
            }

            if let Some(node) = import_node {
                import_stmt.line_number = node.start_position().row as u32 + 1;
                // Namespace imports (`import * as ns from ...`) are treated as glob.
                let mut items = Vec::new();
                let mut is_glob = false;
                if let Some(clause) = node.child_by_field_name("import_clause").or_else(|| {
                    // `import_clause` is not a named field on every version; fall
                    // back to scanning children.
                    let mut c = node.walk();
                    let mut found = None;
                    if c.goto_first_child() {
                        loop {
                            if c.node().kind() == "import_clause" {
                                found = Some(c.node());
                                break;
                            }
                            if !c.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                    found
                }) {
                    let mut cc = clause.walk();
                    if cc.goto_first_child() {
                        loop {
                            let ch = cc.node();
                            match ch.kind() {
                                "namespace_import" => is_glob = true,
                                "identifier" => items.push(self.text(&ch, source)),
                                "named_imports" => {
                                    let mut nc = ch.walk();
                                    if nc.goto_first_child() {
                                        loop {
                                            if nc.node().kind() == "import_specifier" {
                                                if let Some(n) =
                                                    nc.node().child_by_field_name("name")
                                                {
                                                    items.push(self.text(&n, source));
                                                }
                                            }
                                            if !nc.goto_next_sibling() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            if !cc.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                }
                import_stmt.is_glob = is_glob;
                import_stmt.imported_items = items;
            }

            if !import_stmt.module_path.is_empty() {
                imports.push(import_stmt);
            }
        }

        Ok(imports)
    }

    fn extract_exports(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
    ) -> Result<Vec<ExportStatement>> {
        let query_str = r#"
            (export_statement (export_clause (export_specifier name: (identifier) @name))) @export
            (export_statement declaration: (function_declaration name: (identifier) @name)) @export
            (export_statement declaration: (class_declaration name: (type_identifier) @name)) @export
            (export_statement declaration: (abstract_class_declaration name: (type_identifier) @name)) @export
            (export_statement declaration: (interface_declaration name: (type_identifier) @name)) @export
            (export_statement declaration: (type_alias_declaration name: (type_identifier) @name)) @export
            (export_statement declaration: (lexical_declaration (variable_declarator name: (identifier) @name))) @export
            (export_statement declaration: (variable_declaration (variable_declarator name: (identifier) @name))) @export
            (export_statement value: (identifier) @default_name) @export_default
            (export_statement "*") @reexport
        "#;

        let query =
            Query::new(&tree.language(), query_str).map_err(|e| AnalysisError::QueryError {
                message: format!("{:?}", e),
            })?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut exports = Vec::new();

        while let Some(query_match) = matches.next() {
            let mut export_stmt = ExportStatement::new(String::new(), file_path.to_string());
            let mut is_reexport = false;
            let mut line = 0u32;

            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                match capture_name {
                    "name" => export_stmt.exported_item = self.text(&capture.node, source),
                    "default_name" => {
                        export_stmt.exported_item = self.text(&capture.node, source);
                        export_stmt.alias = Some("default".to_string());
                    }
                    "export" | "export_default" => {
                        line = capture.node.start_position().row as u32 + 1;
                    }
                    "reexport" => {
                        is_reexport = true;
                        line = capture.node.start_position().row as u32 + 1;
                    }
                    _ => {}
                }
            }

            if is_reexport && export_stmt.exported_item.is_empty() {
                export_stmt.exported_item = "*".to_string();
            }
            export_stmt.line_number = line;
            export_stmt.is_public = true;

            if !export_stmt.exported_item.is_empty() {
                exports.push(export_stmt);
            }
        }

        Ok(exports)
    }

    fn extract_function_calls(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
    ) -> Result<Vec<FunctionCall>> {
        let query_str = r#"
            (call_expression
              function: (identifier) @function_name
            ) @call
            (call_expression
              function: (member_expression
                object: (_) @receiver
                property: (property_identifier) @method_name
              )
            ) @method_call
        "#;

        let query =
            Query::new(&tree.language(), query_str).map_err(|e| AnalysisError::QueryError {
                message: format!("{:?}", e),
            })?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut function_calls = Vec::new();

        while let Some(query_match) = matches.next() {
            let mut function_call = FunctionCall::new(String::new(), file_path.to_string(), 0);

            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text = self.text(&capture.node, source);
                match capture_name {
                    "function_name" => function_call.function_name = text,
                    "method_name" => function_call.function_name = text,
                    "receiver" => {
                        function_call = function_call.with_method_call(text);
                    }
                    "call" | "method_call" => {
                        let start = capture.node.start_position();
                        function_call.line_number = start.row as u32 + 1;
                        function_call.column = start.column as u32;
                    }
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
        let mut analysis =
            PartialAnalysis::new(file_path.to_string(), "typescript".to_string()).with_fallback();

        // Named function declarations: `export async function foo(`
        if let Ok(re) =
            Regex::new(r"(?m)^\s*(export\s+)?(default\s+)?(async\s+)?function\s+(\w+)\s*[<(]")
        {
            for caps in re.captures_iter(content) {
                if let Some(name) = caps.get(4) {
                    let mut func =
                        FunctionSignature::new(name.as_str().to_string(), file_path.to_string());
                    func.is_public = caps.get(1).is_some() || caps.get(2).is_some();
                    func.is_async = caps.get(3).is_some();
                    analysis.functions.push(func);
                }
            }
        } else {
            analysis.add_error("Failed to create function regex".to_string());
        }

        // Arrow functions bound to a variable: `const foo = async (`
        if let Ok(re) = Regex::new(
            r"(?m)^\s*(export\s+)?(?:const|let|var)\s+(\w+)\s*(?::[^=]+)?=\s*(async\s+)?(?:<[^>]*>\s*)?\([^)]*\)\s*(?::[^=]+)?=>",
        ) {
            for caps in re.captures_iter(content) {
                if let Some(name) = caps.get(2) {
                    let mut func =
                        FunctionSignature::new(name.as_str().to_string(), file_path.to_string());
                    func.is_public = caps.get(1).is_some();
                    func.is_async = caps.get(3).is_some();
                    analysis.functions.push(func);
                }
            }
        } else {
            analysis.add_error("Failed to create arrow function regex".to_string());
        }

        // Classes: `export abstract class Foo`
        if let Ok(re) = Regex::new(r"(?m)^\s*(export\s+)?(default\s+)?(abstract\s+)?class\s+(\w+)")
        {
            for caps in re.captures_iter(content) {
                if let Some(name) = caps.get(4) {
                    let mut class_sig =
                        StructSignature::new(name.as_str().to_string(), file_path.to_string());
                    class_sig.is_public = caps.get(1).is_some() || caps.get(2).is_some();
                    analysis.structs.push(class_sig);
                }
            }
        } else {
            analysis.add_error("Failed to create class regex".to_string());
        }

        analysis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_typescript_analyzer_basic() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        assert_eq!(analyzer.language(), "typescript");
        assert_eq!(analyzer.file_extensions(), &["ts", "tsx"]);
        assert!(analyzer.supports_async());
    }

    #[tokio::test]
    async fn test_plain_function() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "function greet(name: string): string { return name; }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "greet");
        assert!(!f[0].is_async);
        assert_eq!(f[0].parameters.len(), 1);
        assert_eq!(f[0].parameters[0].name, "name");
        assert_eq!(f[0].parameters[0].param_type, "string");
        assert_eq!(f[0].return_type, Some("string".to_string()));
        // Not exported -> not public.
        assert!(!f[0].is_public);
    }

    #[tokio::test]
    async fn test_async_function() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "async function load(): Promise<void> {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "load");
        assert!(f[0].is_async);
        assert_eq!(f[0].return_type, Some("Promise<void>".to_string()));
    }

    #[tokio::test]
    async fn test_exported_function() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export function pub() {}\nfunction priv() {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        let pub_fn = f.iter().find(|x| x.name == "pub").unwrap();
        let priv_fn = f.iter().find(|x| x.name == "priv").unwrap();
        assert!(pub_fn.is_public);
        assert!(!priv_fn.is_public);
    }

    #[tokio::test]
    async fn test_arrow_function_const() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "const add = (a: number, b: number): number => a + b;";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "add");
        assert_eq!(f[0].parameters.len(), 2);
        assert_eq!(f[0].parameters[1].param_type, "number");
        assert_eq!(f[0].return_type, Some("number".to_string()));
    }

    #[tokio::test]
    async fn test_async_arrow_function() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export const fetchIt = async (): Promise<number> => 1;";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "fetchIt");
        assert!(f[0].is_async);
        assert!(f[0].is_public);
    }

    #[tokio::test]
    async fn test_class_with_methods_and_visibility() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
export class Service {
    private secret: number;
    public name: string;
    public greet(who: string): void {}
    private helper(): void {}
    static create(): Service { return new Service(); }
}
"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let classes = &analysis.tree_node.structs;
        let funcs = &analysis.tree_node.functions;

        let svc = classes.iter().find(|c| c.name == "Service").unwrap();
        assert!(svc.is_public);
        // Two fields.
        assert_eq!(svc.fields.len(), 2);
        let secret = svc.fields.iter().find(|f| f.name == "secret").unwrap();
        assert!(!secret.is_public);
        let name = svc.fields.iter().find(|f| f.name == "name").unwrap();
        assert!(name.is_public);
        assert_eq!(name.field_type, "string");

        let greet = funcs.iter().find(|f| f.name == "greet").unwrap();
        assert!(greet.is_public);
        assert!(!greet.is_static);
        let helper = funcs.iter().find(|f| f.name == "helper").unwrap();
        assert!(!helper.is_public);
        let create = funcs.iter().find(|f| f.name == "create").unwrap();
        assert!(create.is_static);
    }

    #[tokio::test]
    async fn test_interface() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export interface Point { x: number; y: number; }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let classes = &analysis.tree_node.structs;
        let point = classes.iter().find(|c| c.name == "Point").unwrap();
        assert!(point.is_public);
        assert_eq!(point.fields.len(), 2);
        assert!(point.fields.iter().any(|f| f.name == "x"));
    }

    #[tokio::test]
    async fn test_generics_on_function() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "function identity<T, U>(a: T, b: U): T { return a; }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions[0];
        assert_eq!(f.generics.len(), 2);
        assert!(f.generics.contains(&"T".to_string()));
        assert!(f.generics.contains(&"U".to_string()));
    }

    #[tokio::test]
    async fn test_generics_on_class() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export class Box<T> { value: T; }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let c = analysis
            .tree_node
            .structs
            .iter()
            .find(|c| c.name == "Box")
            .unwrap();
        assert_eq!(c.generics.len(), 1);
        assert_eq!(c.generics[0], "T");
    }

    #[tokio::test]
    async fn test_type_alias() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export type ID = string | number;";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let t = analysis
            .tree_node
            .structs
            .iter()
            .find(|c| c.name == "ID")
            .unwrap();
        assert!(t.is_public);
    }

    #[tokio::test]
    async fn test_named_import() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "import { readFile, writeFile } from 'fs';";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "fs");
        assert!(imports[0].is_external);
        assert!(imports[0].imported_items.contains(&"readFile".to_string()));
    }

    #[tokio::test]
    async fn test_default_import() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "import React from 'react';";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "react");
        assert!(imports[0].imported_items.contains(&"React".to_string()));
    }

    #[tokio::test]
    async fn test_namespace_import() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "import * as path from 'path';";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "path");
        assert!(imports[0].is_glob);
    }

    #[tokio::test]
    async fn test_import_type() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "import type { Config } from './config';";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "./config");
        assert!(!imports[0].is_external); // relative
        assert!(imports[0].imported_items.contains(&"Config".to_string()));
    }

    #[tokio::test]
    async fn test_exports_named_and_default() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
const a = 1;
const b = 2;
export { a, b };
export default a;
export * from './other';
"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let exports = &analysis.tree_node.exports;
        let names: Vec<&String> = exports.iter().map(|e| &e.exported_item).collect();
        assert!(names.contains(&&"a".to_string()));
        assert!(names.contains(&&"b".to_string()));
        assert!(names.contains(&&"*".to_string()));
        // default marked via alias.
        assert!(
            exports
                .iter()
                .any(|e| e.alias.as_deref() == Some("default"))
        );
    }

    #[tokio::test]
    async fn test_export_declaration() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export function run() {}\nexport class Widget {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let names: Vec<&String> = analysis
            .tree_node
            .exports
            .iter()
            .map(|e| &e.exported_item)
            .collect();
        assert!(names.contains(&&"run".to_string()));
        assert!(names.contains(&&"Widget".to_string()));
    }

    #[tokio::test]
    async fn test_function_and_method_calls() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
function main() {
    doThing();
    obj.method(1);
}
"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let calls = &analysis.tree_node.function_calls;
        assert!(
            calls
                .iter()
                .any(|c| c.function_name == "doThing" && !c.is_method_call)
        );
        assert!(
            calls
                .iter()
                .any(|c| c.function_name == "method" && c.is_method_call)
        );
    }

    #[tokio::test]
    async fn test_tsx_component() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
export const App = (props: Props): JSX.Element => {
    return <div className="app">Hello</div>;
};
"#;
        // Go through analyze_file end-to-end with a .tsx path to exercise the
        // TSX grammar selection.
        let analysis = analyzer.analyze_file(code, "App.tsx").await.unwrap();
        let f = &analysis.tree_node.functions;
        let app = f.iter().find(|x| x.name == "App").unwrap();
        assert!(app.is_public);
        assert_eq!(app.parameters.len(), 1);
        assert_eq!(app.parameters[0].name, "props");
    }

    #[tokio::test]
    async fn test_tsx_class_component_end_to_end() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        // A class returning JSX from a method — only parses under the TSX grammar.
        let code = r#"
export class Panel {
    render() {
        return <section><span>{this.title}</span></section>;
    }
}
"#;
        let analysis = analyzer.analyze_file(code, "Panel.tsx").await.unwrap();
        let panel = analysis
            .tree_node
            .structs
            .iter()
            .find(|c| c.name == "Panel")
            .unwrap();
        assert!(panel.is_public);
        assert!(
            analysis
                .tree_node
                .functions
                .iter()
                .any(|f| f.name == "render")
        );
        assert!(analysis.success);
    }

    #[tokio::test]
    async fn test_fallback_parsing() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let malformed = r#"
export async function validFn(x: number) {
    return x;
}

const arrowFn = (a: number) => a + 1;

export class GoodClass {
"#; // deliberately unterminated
        let fallback = analyzer.extract_with_fallback(malformed, "a.ts");
        assert!(fallback.fallback_used);
        let fn_names: Vec<&String> = fallback.functions.iter().map(|f| &f.name).collect();
        assert!(fn_names.contains(&&"validFn".to_string()));
        assert!(fn_names.contains(&&"arrowFn".to_string()));
        let class_names: Vec<&String> = fallback.structs.iter().map(|s| &s.name).collect();
        assert!(class_names.contains(&&"GoodClass".to_string()));
    }

    #[tokio::test]
    async fn test_file_path_propagation() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
import { x } from './x';
export function f(): void {}
export interface I { a: number; }
function main() { f(); }
"#;
        let path = "src/components/thing.ts";
        let analysis = analyzer.analyze_file(code, path).await.unwrap();
        let tn = &analysis.tree_node;
        for f in &tn.functions {
            assert_eq!(f.file_path, path);
        }
        for s in &tn.structs {
            assert_eq!(s.file_path, path);
        }
        for i in &tn.imports {
            assert_eq!(i.file_path, path);
        }
        for e in &tn.exports {
            assert_eq!(e.file_path, path);
        }
        for c in &tn.function_calls {
            assert_eq!(c.file_path, path);
        }
    }

    #[test]
    fn test_content_hash() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let h1 = analyzer.calculate_content_hash("const x = 1;");
        let h2 = analyzer.calculate_content_hash("const x = 1;");
        let h3 = analyzer.calculate_content_hash("const y = 2;");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[tokio::test]
    async fn test_comprehensive() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
import { Logger } from './logger';
import type { Options } from './options';

export interface Repo<T> {
    load(id: string): Promise<T>;
}

export class Store<T> implements Repo<T> {
    private items: Map<string, T>;
    constructor() { this.items = new Map(); }
    public async load(id: string): Promise<T> {
        return this.items.get(id)!;
    }
    static empty(): Store<unknown> { return new Store(); }
}

export const makeStore = <T>(): Store<T> => new Store<T>();

export type Key = string;
"#;
        let analysis = analyzer.analyze_file(code, "store.ts").await.unwrap();
        let tn = &analysis.tree_node;
        assert!(analysis.success);
        assert!(!tn.functions.is_empty());
        assert!(!tn.structs.is_empty());
        assert!(!tn.imports.is_empty());
        assert!(!tn.exports.is_empty());

        let store = tn.structs.iter().find(|c| c.name == "Store").unwrap();
        assert_eq!(store.generics, vec!["T".to_string()]);

        let load = tn.functions.iter().find(|f| f.name == "load").unwrap();
        assert!(load.is_async);
        assert!(load.is_public);

        let empty = tn.functions.iter().find(|f| f.name == "empty").unwrap();
        assert!(empty.is_static);

        let make = tn.functions.iter().find(|f| f.name == "makeStore").unwrap();
        assert!(make.is_public);
    }
}
