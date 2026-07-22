use crate::analyzers::LanguageAnalyzer;
use crate::types::{
    AnalysisError, ExportStatement, FileAnalysis, FunctionCall, FunctionSignature, ImportStatement,
    Parameter, PartialAnalysis, Result, StructField, StructSignature, TreeNode, TypeKind,
};
use async_trait::async_trait;
use blake3;
use regex::Regex;
use std::time::Instant;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

/// Analyzer for TypeScript, TSX and JavaScript sources.
///
/// It holds both grammars from `tree-sitter-typescript` and selects between
/// them based on the file extension: JSX-bearing dialects (`.tsx`, `.jsx`) and
/// JavaScript (`.js`, `.mjs`, `.cjs`) use the TSX grammar, while `.ts`/`.mts`/
/// `.cts` use the plain TypeScript grammar. JavaScript files are labelled
/// `"javascript"` even though the same grammar and queries handle them.
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

    /// The file's extension, lowercased.
    fn extension_of(file_path: &str) -> String {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    /// Pick the grammar for a file. JSX-bearing dialects (`.tsx`, `.jsx`) and
    /// JavaScript go to the TSX grammar; `.ts`/`.mts`/`.cts` to the TypeScript one.
    ///
    /// JavaScript needs no grammar of its own: TypeScript's is generated as a
    /// superset of JavaScript's, and TSX adds the JSX syntax that plain `.js`
    /// files routinely contain. Measured over 3,979 real `.js`/`.mjs`/`.cjs`/`.jsx`
    /// files, TSX parses exactly as many cleanly as the TypeScript grammar (3,975)
    /// with zero files that TypeScript accepts and TSX rejects — and because both
    /// dialects share node-type names, every query in this file works unchanged.
    fn language_for_path(&self, file_path: &str) -> &Language {
        match Self::extension_of(file_path).as_str() {
            "tsx" | "jsx" | "js" | "mjs" | "cjs" => &self.tsx,
            _ => &self.typescript,
        }
    }

    /// What to label the file as. The analyzer handles both, but a `.js` file is
    /// JavaScript and reporting it as TypeScript would mislead language filters
    /// and the repository tree.
    fn language_label_for_path(file_path: &str) -> &'static str {
        match Self::extension_of(file_path).as_str() {
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            _ => "typescript",
        }
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

    /// Is `node` exported directly at module level? Used to decide public
    /// visibility of top-level items.
    ///
    /// A declaration is exported only when it sits DIRECTLY under an
    /// `export_statement` (`export function foo`, `export class Bar`,
    /// `export type Baz`), or, for arrow-bound `const`/`let`/`var`
    /// declarations, when the enclosing `lexical_declaration`/
    /// `variable_declaration` is itself under an `export_statement`
    /// (`export const foo = () => {}`). Functions nested inside another
    /// function/method body are NOT exported, even when the outer function is.
    fn is_exported(node: &Node) -> bool {
        match node.parent() {
            Some(p) if p.kind() == "export_statement" => true,
            Some(p) if p.kind() == "lexical_declaration" || p.kind() == "variable_declaration" => {
                matches!(p.parent().map(|g| g.kind()), Some("export_statement"))
            }
            _ => false,
        }
    }

    /// Is `node` (the `variable_declarator` captured for an arrow-bound
    /// declaration) at module top-level? True when its enclosing
    /// `lexical_declaration`/`variable_declaration` sits directly in the
    /// program, optionally wrapped in a top-level `export_statement`. Arrows
    /// declared inside a function or method body return false, so a nested
    /// `const helper = () => {}` is not surfaced as a top-level function.
    fn is_top_level_arrow_decl(node: &Node) -> bool {
        let decl = match node.parent() {
            Some(p) if p.kind() == "lexical_declaration" || p.kind() == "variable_declaration" => p,
            _ => return false,
        };
        match decl.parent() {
            Some(p) if p.kind() == "program" => true,
            Some(p) if p.kind() == "export_statement" => {
                matches!(p.parent().map(|g| g.kind()), Some("program"))
            }
            _ => false,
        }
    }

    /// Is `node` nested inside a function/method body? Walks ancestors looking
    /// for a `statement_block` whose parent is a callable. A `namespace X {}`
    /// body is also a `statement_block`, but its parent is the module node, so
    /// types declared in a namespace stay addressable and stay indexed.
    fn is_inside_function_body(node: &Node) -> bool {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if n.kind() == "statement_block"
                && let Some(p) = n.parent()
                && matches!(
                    p.kind(),
                    "function_declaration"
                        | "generator_function_declaration"
                        | "function_expression"
                        | "generator_function"
                        | "arrow_function"
                        | "method_definition"
                )
            {
                return true;
            }
            cur = n.parent();
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
    ///
    /// A single-parameter arrow written without parentheses (`x => x + 1`) has
    /// no `formal_parameters` wrapper; instead the arrow node exposes a bare
    /// `parameter` field holding the identifier. Handle that case first.
    fn extract_params(&self, sig_node: &Node, source: &str) -> Vec<Parameter> {
        let mut out = Vec::new();
        if let Some(param) = sig_node.child_by_field_name("parameter") {
            let name = self.text(&param, source);
            if !name.is_empty() {
                out.push(Parameter::new(name, String::new()));
            }
            return out;
        }
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

    /// Collect the declared supertypes of a type declaration into a flat list of
    /// raw name strings.
    ///
    /// - Classes / abstract classes carry a `class_heritage` child holding an
    ///   `extends_clause` (its `value` field is the single base class) and/or an
    ///   `implements_clause` (its named children are the implemented types). A
    ///   class may have both, e.g. `class C extends Base implements IThing` ->
    ///   `["Base", "IThing"]`.
    /// - Interfaces carry an `extends_type_clause` whose named children are the
    ///   (possibly several) extended interfaces.
    ///
    /// Anonymous tokens (`extends`, `implements`, commas) are skipped by only
    /// walking named children.
    fn extract_supertypes(&self, node: &Node, source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    // Class / abstract class heritage.
                    "class_heritage" => {
                        let mut hc = child.walk();
                        if hc.goto_first_child() {
                            loop {
                                let clause = hc.node();
                                match clause.kind() {
                                    "extends_clause" => {
                                        if let Some(v) = clause.child_by_field_name("value") {
                                            let t = self.text(&v, source);
                                            if !t.is_empty() {
                                                out.push(t);
                                            }
                                        }
                                    }
                                    "implements_clause" => {
                                        out.extend(self.named_children_text(&clause, source))
                                    }
                                    _ => {}
                                }
                                if !hc.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    }
                    // Interface heritage: `interface I extends A, B`.
                    "extends_type_clause" => out.extend(self.named_children_text(&child, source)),
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        out
    }

    /// Text of every *named* child of `parent` (skipping anonymous tokens such as
    /// `implements`/`extends` keywords and commas), dropping empties.
    fn named_children_text(&self, parent: &Node, source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut c = parent.walk();
        if c.goto_first_child() {
            loop {
                let ch = c.node();
                if ch.is_named() {
                    let t = self.text(&ch, source);
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
                if !c.goto_next_sibling() {
                    break;
                }
            }
        }
        out
    }

    /// Byte offsets of the regions the parser could not make sense of.
    fn error_regions(tree: &Tree) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut cursor = tree.walk();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.is_error() {
                out.push((node.start_byte(), node.end_byte()));
                continue;
            }
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        out
    }

    /// Does this line start a top-level declaration? Column 0 only: an indented
    /// `export` is a member, and re-parsing from there would just break again.
    fn starts_declaration(line: &str) -> bool {
        [
            "export ",
            "declare ",
            "interface ",
            "type ",
            "class ",
            "abstract ",
            "enum ",
            "namespace ",
            "module ",
            "function ",
            "async function ",
            "const ",
            "let ",
            "var ",
        ]
        .iter()
        .any(|kw| line.starts_with(kw))
    }

    /// Re-parse the damaged tail of a file declaration-by-declaration.
    ///
    /// tree-sitter does not stop at a construct it cannot parse — it degenerates.
    /// Past the first ERROR in hono's `src/types.ts`, later declarations are
    /// shredded into loose tokens (`identifier`, `<`, `extends`, ERROR) at top
    /// level instead of `interface_declaration`/`type_alias_declaration`, so every
    /// query stops matching and ~1,650 lines of real declarations vanish. The
    /// trigger is tree-sitter-typescript#335 — two anonymous generic call
    /// signatures separated by a newline rather than a semicolon, which
    /// TypeScript itself accepts. That issue has been open upstream since
    /// 2025-06 with no fix, so recovery has to live here.
    ///
    /// A grammar gap should cost the declarations it actually covers and nothing
    /// else, so from the first error onward the source is split at column-0
    /// declaration boundaries and each chunk parsed on its own: the broken
    /// construct still fails, everything after it does not.
    fn recover_error_regions(
        &self,
        tree: &Tree,
        content: &str,
        file_path: &str,
    ) -> (Vec<FunctionSignature>, Vec<StructSignature>) {
        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let language = self.language_for_path(file_path).clone();

        let first_error = match Self::error_regions(tree).into_iter().map(|(s, _)| s).min() {
            Some(offset) => offset,
            None => return (functions, structs),
        };

        let mut bounds: Vec<(usize, u32)> = Vec::new();
        let mut offset = 0usize;
        let mut line = 0u32;
        for l in content.split_inclusive('\n') {
            if offset >= first_error && Self::starts_declaration(l) {
                bounds.push((offset, line));
            }
            offset += l.len();
            line += 1;
        }
        if bounds.is_empty() {
            return (functions, structs);
        }
        bounds.push((content.len(), line));

        for window in bounds.windows(2) {
            let (from, line_offset) = window[0];
            let (to, _) = window[1];
            let Some(chunk) = content.get(from..to) else {
                continue;
            };
            if chunk.trim().is_empty() {
                continue;
            }
            let parsed = std::panic::catch_unwind(|| {
                let mut parser = Parser::new();
                match parser.set_language(&language) {
                    Ok(_) => parser.parse(chunk, None),
                    Err(_) => None,
                }
            });
            let Ok(Some(subtree)) = parsed else { continue };

            if let Ok(Ok(mut fns)) =
                std::panic::catch_unwind(|| self.extract_functions(&subtree, chunk, file_path))
            {
                for f in &mut fns {
                    f.start_line += line_offset;
                    f.end_line += line_offset;
                }
                functions.append(&mut fns);
            }
            if let Ok(Ok(mut sts)) =
                std::panic::catch_unwind(|| self.extract_structs(&subtree, chunk, file_path))
            {
                for st in &mut sts {
                    st.start_line += line_offset;
                    st.end_line += line_offset;
                }
                structs.append(&mut sts);
            }
        }
        (functions, structs)
    }
}

#[async_trait]
impl LanguageAnalyzer for TypeScriptAnalyzer {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn file_extensions(&self) -> &[&'static str] {
        // JavaScript included: the TSX grammar parses it (see `language_for_path`),
        // so leaving these out meant `.js` files were scanned, labelled
        // "javascript", and then silently never analyzed by anyone.
        &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"]
    }

    fn supports_async(&self) -> bool {
        true
    }

    async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis> {
        let start_time = Instant::now();
        let mut tree_node = TreeNode::new(
            file_path.to_string(),
            Self::language_label_for_path(file_path).to_string(),
        );
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
            let mut fb = TreeNode::new(
                file_path.to_string(),
                Self::language_label_for_path(file_path).to_string(),
            );
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

        // Salvage what an unparseable construct swallowed. Dedup on (name, line):
        // a recovered chunk re-reports whatever the primary pass already found.
        if tree.root_node().has_error() {
            let (fns, sts) = self.recover_error_regions(&tree, content, file_path);
            for f in fns {
                if !tree_node
                    .functions
                    .iter()
                    .any(|e| e.name == f.name && e.start_line == f.start_line)
                {
                    tree_node.functions.push(f);
                }
            }
            for st in sts {
                if !tree_node
                    .structs
                    .iter()
                    .any(|e| e.name == st.name && e.start_line == st.start_line)
                {
                    tree_node.structs.push(st);
                }
            }
            tree_node
                .functions
                .sort_by(|a, b| (a.start_line, &a.name).cmp(&(b.start_line, &b.name)));
            tree_node
                .structs
                .sort_by(|a, b| (a.start_line, &a.name).cmp(&(b.start_line, &b.name)));
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
        // The shapes of "function": top-level declarations (including
        // generators), function expressions and arrows bound to a
        // `const`/`let`/`var`, and class methods. A bound function expression is
        // named by its BINDING (`const f = function inner() {}` is `f`), which is
        // what callers actually write.
        let query_str = r#"
            (function_declaration name: (identifier) @name) @function
            (generator_function_declaration name: (identifier) @name) @function
            (variable_declarator name: (identifier) @name value: (arrow_function) @arrow) @arrow_decl
            (variable_declarator name: (identifier) @name value: (function_expression) @arrow) @arrow_decl
            (variable_declarator name: (identifier) @name value: (generator_function) @arrow) @arrow_decl
            (method_definition name: (property_identifier) @name) @method
            ; `#private` methods use a distinct identifier node.
            (method_definition name: (private_property_identifier) @name) @method
            ; Bodiless declarations: interface members, abstract class methods,
            ; and ambient `declare function`. All are real, callable API surface —
            ; hono alone has ~60 across its interfaces and .d.ts adapters.
            (method_signature name: (property_identifier) @name) @method
            (abstract_method_signature name: (property_identifier) @name) @method
            (function_signature name: (identifier) @name) @function
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

            // Arrow functions are only surfaced when bound to a module top-level
            // declaration. An arrow declared inside a function/method body (e.g.
            // `const helper = () => {}`) is a local, not a top-level function.
            if kind == "arrow" && !Self::is_top_level_arrow_decl(&outer) {
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
                    // Owner = the enclosing class name. Walk up to the nearest
                    // `class_declaration`/`abstract_class_declaration` (a method
                    // sits inside a `class_body`). Free/module-level functions keep
                    // `owner == None`.
                    let mut ancestor = outer.parent();
                    while let Some(a) = ancestor {
                        match a.kind() {
                            // Interfaces own their members exactly as classes do;
                            // without this an interface method surfaces ownerless
                            // and reads as a free function.
                            "class_declaration"
                            | "abstract_class_declaration"
                            | "interface_declaration" => {
                                if let Some(n) = a.child_by_field_name("name") {
                                    func.owner = Some(self.text(&n, source));
                                }
                                break;
                            }
                            _ => {}
                        }
                        ancestor = a.parent();
                    }
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
            (enum_declaration name: (identifier) @name) @enum
            (internal_module name: (identifier) @name) @namespace
            (module name: (identifier) @name) @namespace
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
                    "enum" => {
                        node = Some(capture.node);
                        kind = "enum";
                    }
                    "namespace" => {
                        node = Some(capture.node);
                        kind = "namespace";
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
            // A type declared inside a function body is a local, exactly like an
            // arrow bound inside one (see `is_top_level_arrow_decl`): it cannot be
            // referenced from outside, so surfacing it as a module-level type is
            // noise. hono's client tests declare `type Actual`/`type Expected`
            // inside `it(() => { ... })` callbacks ~370 times, which is how this
            // was found. Declarations inside a `namespace`/`module` block are NOT
            // affected — their body is a statement_block too, but its parent is
            // the module node, not a function.
            if Self::is_inside_function_body(&node) {
                continue;
            }

            let mut sig = StructSignature::new(name, file_path.to_string());
            sig.start_line = node.start_position().row as u32 + 1;
            sig.end_line = node.end_position().row as u32 + 1;
            sig.generics = self.extract_type_params(&node, source);
            sig.is_public = Self::is_exported(&node);
            // Discriminate the concrete kind from the declaration node itself
            // (abstract classes are a distinct `abstract_class_declaration` node,
            // not a modifier on `class_declaration`).
            sig.kind = match node.kind() {
                "class_declaration" => TypeKind::Class,
                "abstract_class_declaration" => TypeKind::AbstractClass,
                "interface_declaration" => TypeKind::Interface,
                "type_alias_declaration" => TypeKind::TypeAlias,
                "enum_declaration" => TypeKind::Enum,
                // `namespace X {}` and the legacy `module X {}`.
                "internal_module" | "module" => TypeKind::Namespace,
                _ => TypeKind::Struct,
            };
            // `extends`/`implements` (class) and `extends` (interface) supertypes.
            sig.supertypes = self.extract_supertypes(&node, source);

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
                            // Enum member: `Red` or `Green = 2`. The members are
                            // the useful content of an enum, so record them as
                            // fields (with the assigned value as the "type" when
                            // there is one).
                            "property_identifier" => {
                                sig.fields.push(
                                    StructField::new(self.text(&member, source), String::new())
                                        .with_visibility(true),
                                );
                            }
                            "enum_assignment" => {
                                if let Some(fname) = member.child_by_field_name("name") {
                                    let value = member
                                        .child_by_field_name("value")
                                        .map(|v| self.text(&v, source))
                                        .unwrap_or_default();
                                    sig.fields.push(
                                        StructField::new(self.text(&fname, source), value)
                                            .with_visibility(true),
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
        // Every construct that names another module. A re-export
        // (`export … from "./x"`) is an import edge as much as an `import` is —
        // barrel files are built entirely out of them — and CommonJS `require`
        // plus dynamic `import()` are how most JavaScript states a dependency.
        let query_str = r#"
            (import_statement source: (string (string_fragment) @source)) @import
            (import_statement
              (import_require_clause source: (string (string_fragment) @source))) @import
            (export_statement source: (string (string_fragment) @source)) @reexport
            (call_expression
              function: (import)
              arguments: (arguments (string (string_fragment) @source))) @dynamic
            (call_expression
              function: (identifier) @callee
              arguments: (arguments (string (string_fragment) @source))) @call
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
            let mut reexport_node: Option<Node> = None;
            let mut standalone_node: Option<Node> = None;
            // The `@call` pattern matches every one-string-argument call; only
            // `require(…)` names a module.
            let mut callee = String::new();

            for capture in query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                match capture_name {
                    "callee" => callee = self.text(&capture.node, source),
                    "reexport" => reexport_node = Some(capture.node),
                    "dynamic" | "call" => standalone_node = Some(capture.node),
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

            if let Some(node) = reexport_node {
                import_stmt.line_number = node.start_position().row as u32 + 1;
                // `export { a, b } from "./x"` names items; `export * from "./x"`
                // and `export * as ns from "./x"` take everything, so they are globs.
                let mut items = Vec::new();
                let mut has_clause = false;
                let mut c = node.walk();
                if c.goto_first_child() {
                    loop {
                        if c.node().kind() == "export_clause" {
                            has_clause = true;
                            let mut ec = c.node().walk();
                            if ec.goto_first_child() {
                                loop {
                                    if ec.node().kind() == "export_specifier"
                                        && let Some(n) = ec.node().child_by_field_name("name")
                                    {
                                        items.push(self.text(&n, source));
                                    }
                                    if !ec.goto_next_sibling() {
                                        break;
                                    }
                                }
                            }
                        }
                        if !c.goto_next_sibling() {
                            break;
                        }
                    }
                }
                import_stmt.is_glob = !has_clause;
                import_stmt.imported_items = items;
            }

            if let Some(node) = standalone_node {
                // `require("./x")` names a module; `translate("./x")` does not. The
                // dynamic-`import()` pattern captures no callee, so it always passes.
                if !callee.is_empty() && callee != "require" {
                    continue;
                }
                import_stmt.line_number = node.start_position().row as u32 + 1;
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
        let mut analysis = PartialAnalysis::new(
            file_path.to_string(),
            Self::language_label_for_path(file_path).to_string(),
        )
        .with_fallback();

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
        //
        // The variable's optional type annotation may itself contain `=>` (an
        // arrow *type*), e.g. `const cb: (n: number) => void = (n) => {}`. The
        // annotation matcher therefore tolerates `=>` while still stopping at the
        // lone `=` that begins the assignment.
        if let Ok(re) = Regex::new(
            r"(?m)^\s*(export\s+)?(?:const|let|var)\s+(\w+)\s*(?::(?:=>|[^=\n])+)?=\s*(async\s+)?(?:<[^>]*>\s*)?\([^)]*\)\s*(?::[^=]+)?=>",
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
        assert_eq!(
            analyzer.file_extensions(),
            &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"]
        );
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
    async fn test_enum_is_extracted_with_members() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export enum Color { Red, Green = 2 }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let structs = &analysis.tree_node.structs;
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "Color");
        assert_eq!(structs[0].kind, TypeKind::Enum);
        assert!(structs[0].is_public);
        let members: Vec<&str> = structs[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(members, vec!["Red", "Green"]);
        // An explicitly assigned member carries its value.
        assert_eq!(structs[0].fields[1].field_type, "2");
    }

    #[tokio::test]
    async fn test_const_and_ambient_enums_are_extracted() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "const enum Fast { A }\ndeclare enum Ambient { X }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let names: Vec<&str> = analysis
            .tree_node
            .structs
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"Fast"), "got {names:?}");
        assert!(names.contains(&"Ambient"), "got {names:?}");
    }

    #[tokio::test]
    async fn test_namespace_and_legacy_module_are_extracted() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export namespace NS { export const x = 1; }\nmodule Legacy {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let by_name: Vec<(&str, TypeKind)> = analysis
            .tree_node
            .structs
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();
        assert!(
            by_name.contains(&("NS", TypeKind::Namespace)),
            "got {by_name:?}"
        );
        assert!(
            by_name.contains(&("Legacy", TypeKind::Namespace)),
            "got {by_name:?}"
        );
    }

    #[tokio::test]
    async fn test_generator_function_is_extracted() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export function* gen() { yield 1; }\nexport async function* agen() {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let names: Vec<&str> = analysis
            .tree_node
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(names.contains(&"gen"), "got {names:?}");
        assert!(names.contains(&"agen"), "got {names:?}");
        let agen = analysis
            .tree_node
            .functions
            .iter()
            .find(|f| f.name == "agen")
            .unwrap();
        assert!(agen.is_async);
    }

    #[tokio::test]
    async fn test_bound_function_expression_is_named_by_its_binding() {
        // `const f = function inner() {}` is called as `f`, so that is the name
        // that matters; the inner name only shows up in stack traces.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code =
            "export const fnExpr = function inner() { return 4; };\nconst g = function* () {};";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let names: Vec<&str> = analysis
            .tree_node
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(names.contains(&"fnExpr"), "got {names:?}");
        assert!(names.contains(&"g"), "got {names:?}");
        assert!(!names.contains(&"inner"), "got {names:?}");
    }

    #[tokio::test]
    async fn test_function_expression_inside_a_body_is_not_top_level() {
        // Same rule as arrows: a function expression bound inside another
        // function is a local, not a module-level function.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "function outer() { const helper = function () {}; return helper; }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let names: Vec<&str> = analysis
            .tree_node
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(names, vec!["outer"]);
    }

    #[tokio::test]
    async fn test_named_reexport_is_an_import_edge() {
        // `export { a } from "./x"` is how barrel files pull modules together —
        // it is an import edge, not just an export.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"export { helper, other } from "./util";"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "./util");
        assert!(!imports[0].is_external);
        assert!(!imports[0].is_glob);
        assert!(imports[0].imported_items.contains(&"helper".to_string()));
    }

    #[tokio::test]
    async fn test_star_reexport_is_a_glob_import_edge() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"export * from "./everything";"#;
        let analysis = analyzer.analyze_file(code, "index.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "./everything");
        assert!(imports[0].is_glob);
    }

    #[tokio::test]
    async fn test_namespace_reexport_is_an_import_edge() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"export * as ns from "./bundle";"#;
        let analysis = analyzer.analyze_file(code, "index.ts").await.unwrap();
        assert_eq!(analysis.tree_node.imports.len(), 1);
        assert_eq!(analysis.tree_node.imports[0].module_path, "./bundle");
    }

    #[tokio::test]
    async fn test_dynamic_import_is_an_import_edge() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"async function load() { return await import("./lazy"); }"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let imports = &analysis.tree_node.imports;
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "./lazy");
    }

    #[tokio::test]
    async fn test_require_is_an_import_edge_but_other_calls_are_not() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
const fs = require("./fs-wrapper");
const label = translate("./not-a-module");
"#;
        let analysis = analyzer.analyze_file(code, "a.js").await.unwrap();
        let paths: Vec<&str> = analysis
            .tree_node
            .imports
            .iter()
            .map(|i| i.module_path.as_str())
            .collect();
        assert_eq!(paths, vec!["./fs-wrapper"]);
    }

    #[tokio::test]
    async fn test_import_equals_require_is_an_import_edge() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"import legacy = require("./legacy");"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        assert_eq!(analysis.tree_node.imports.len(), 1);
        assert_eq!(analysis.tree_node.imports[0].module_path, "./legacy");
    }

    #[tokio::test]
    async fn test_javascript_file_is_analyzed_and_labelled_javascript() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
export function greet(name) { return `hi ${name}`; }
class Widget { render() { return 1; } }
"#;
        let analysis = analyzer.analyze_file(code, "app.js").await.unwrap();
        assert_eq!(analysis.tree_node.language, "javascript");
        let names: Vec<&str> = analysis
            .tree_node
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(names.contains(&"greet"), "got {names:?}");
        assert!(names.contains(&"render"), "got {names:?}");
        // Extraction results are the real signal: `parse_errors` records
        // extractor failures, not a tree with ERROR nodes, so asserting it is
        // empty would pass even if the grammar rejected the file.
        assert_eq!(analysis.tree_node.structs.len(), 1, "class Widget");
    }

    #[tokio::test]
    async fn test_jsx_in_a_js_file_parses() {
        // JSX in a plain `.js` file is idiomatic React and is why JavaScript is
        // routed to the TSX grammar rather than the TypeScript one.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
import { Child } from "./Child";
export function App(props) {
  return <div className="app">{props.children}<Child /></div>;
}
"#;
        let analysis = analyzer.analyze_file(code, "App.js").await.unwrap();
        // If the grammar choked on the JSX, the enclosing function and the
        // import inside it would not be extracted at all.
        assert_eq!(analysis.tree_node.functions.len(), 1);
        assert_eq!(analysis.tree_node.functions[0].name, "App");
        assert_eq!(analysis.tree_node.imports.len(), 1);
        assert_eq!(analysis.tree_node.imports[0].module_path, "./Child");
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
    async fn test_nested_arrow_not_top_level() {
        // Regression: an arrow bound inside a function body must NOT be surfaced
        // as a top-level function.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
function outer(): void {
    const helper = () => 42;
    helper();
}
"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        assert!(
            f.iter().any(|x| x.name == "outer"),
            "outer should be surfaced"
        );
        assert!(
            !f.iter().any(|x| x.name == "helper"),
            "nested arrow `helper` must not be a top-level function"
        );
    }

    #[tokio::test]
    async fn test_nested_function_not_exported() {
        // Regression: a function nested inside an exported function must not be
        // marked public just because an ancestor is exported.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "export function outer() { function inner() {} }";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        let outer = f.iter().find(|x| x.name == "outer").unwrap();
        let inner = f.iter().find(|x| x.name == "inner").unwrap();
        assert!(outer.is_public, "outer is exported -> public");
        assert!(!inner.is_public, "nested inner must not be public");
    }

    #[tokio::test]
    async fn test_single_bare_identifier_arrow_param() {
        // Regression: `x => x + 1` has a bare identifier parameter (no
        // formal_parameters wrapper); the single param must still be captured.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "const inc = x => x + 1;";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let f = &analysis.tree_node.functions;
        let inc = f.iter().find(|x| x.name == "inc").unwrap();
        assert_eq!(inc.parameters.len(), 1);
        assert_eq!(inc.parameters[0].name, "x");
    }

    #[tokio::test]
    async fn test_fallback_arrow_with_arrow_typed_annotation() {
        // Regression: the fallback arrow regex used to stop at the first `=`,
        // so a declaration whose type annotation contains `=>` was missed.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let content = "export const cb: (n: number) => void = (n) => {};";
        let fallback = analyzer.extract_with_fallback(content, "a.ts");
        let names: Vec<&String> = fallback.functions.iter().map(|f| &f.name).collect();
        assert!(
            names.contains(&&"cb".to_string()),
            "fallback should capture `cb`, got {:?}",
            names
        );
    }

    // ---- P1-4: kind / owner / supertypes ----

    #[tokio::test]
    async fn test_p1_4_kind_discrimination() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
interface IThing {}
class C {}
type T = string | number;
"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let structs = &analysis.tree_node.structs;
        let ithing = structs.iter().find(|s| s.name == "IThing").unwrap();
        let c = structs.iter().find(|s| s.name == "C").unwrap();
        let t = structs.iter().find(|s| s.name == "T").unwrap();
        assert_eq!(ithing.kind, TypeKind::Interface);
        assert_eq!(c.kind, TypeKind::Class);
        assert_eq!(t.kind, TypeKind::TypeAlias);
    }

    #[tokio::test]
    async fn test_p1_4_abstract_class_kind() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "abstract class A {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let a = analysis
            .tree_node
            .structs
            .iter()
            .find(|s| s.name == "A")
            .unwrap();
        assert_eq!(a.kind, TypeKind::AbstractClass);
    }

    #[tokio::test]
    async fn test_p1_4_class_extends_and_implements_supertypes() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "class C extends Base implements IThing {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let c = analysis
            .tree_node
            .structs
            .iter()
            .find(|s| s.name == "C")
            .unwrap();
        assert_eq!(c.kind, TypeKind::Class);
        assert!(
            c.supertypes.contains(&"Base".to_string()),
            "supertypes should contain Base, got {:?}",
            c.supertypes
        );
        assert!(
            c.supertypes.contains(&"IThing".to_string()),
            "supertypes should contain IThing, got {:?}",
            c.supertypes
        );
    }

    #[tokio::test]
    async fn test_p1_4_interface_extends_multiple() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = "interface I extends A, B {}";
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let i = analysis
            .tree_node
            .structs
            .iter()
            .find(|s| s.name == "I")
            .unwrap();
        assert_eq!(i.kind, TypeKind::Interface);
        assert!(i.supertypes.contains(&"A".to_string()));
        assert!(i.supertypes.contains(&"B".to_string()));
    }

    #[tokio::test]
    async fn test_p1_4_method_owner_and_free_function_none() {
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
class C {
    m(): void {}
}
function f() {}
"#;
        let analysis = analyzer.analyze_file(code, "a.ts").await.unwrap();
        let funcs = &analysis.tree_node.functions;
        let m = funcs.iter().find(|x| x.name == "m").unwrap();
        let f = funcs.iter().find(|x| x.name == "f").unwrap();
        assert_eq!(m.owner.as_deref(), Some("C"));
        assert_eq!(f.owner, None);
    }

    #[tokio::test]
    async fn test_recovers_declarations_after_an_unparseable_construct() {
        // tree-sitter-typescript#335: two anonymous generic call signatures
        // separated by a newline (no semicolon) fail to parse, and the parser
        // then degenerates — without recovery every declaration AFTER the broken
        // interface is lost. TypeScript itself accepts this code.
        let analyzer = TypeScriptAnalyzer::new().unwrap();
        let code = r#"
interface Broken<E> {
  <K extends keyof E>(key: K): E[K]
  <K extends string>(key: K): number
}

export type AfterAlias = string

export interface AfterInterface {
  field: number
}

export class AfterClass {
  method(): void {}
}

export function afterFunction(): number {
  return 1
}
"#;
        let analysis = analyzer.analyze_file(code, "broken.ts").await.unwrap();
        let tn = &analysis.tree_node;

        let type_names: Vec<&str> = tn.structs.iter().map(|s| s.name.as_str()).collect();
        for expected in ["AfterAlias", "AfterInterface", "AfterClass"] {
            assert!(
                type_names.contains(&expected),
                "{expected} lost after the unparseable interface: {type_names:?}"
            );
        }
        let fn_names: Vec<&str> = tn.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            fn_names.contains(&"afterFunction"),
            "afterFunction lost: {fn_names:?}"
        );
        assert!(fn_names.contains(&"method"), "method lost: {fn_names:?}");

        // Recovery must not duplicate what the primary pass already found.
        assert_eq!(
            type_names.iter().filter(|n| **n == "AfterAlias").count(),
            1,
            "duplicate symbol from the recovery pass"
        );
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

        // Two `load`s exist now: the interface's bodiless signature and the
        // class's implementation. Select by owner rather than by name alone.
        let load = tn
            .functions
            .iter()
            .find(|f| f.name == "load" && f.owner.as_deref() == Some("Store"))
            .expect("Store::load");
        assert!(load.is_async);
        assert!(load.is_public);

        // The interface member is real callable API surface and is attributed to
        // the interface that declares it.
        let decl = tn
            .functions
            .iter()
            .find(|f| f.name == "load" && f.owner.as_deref() == Some("Repo"))
            .expect("Repo::load signature");
        assert!(
            !decl.is_async,
            "a bodiless signature carries no async keyword"
        );

        let empty = tn.functions.iter().find(|f| f.name == "empty").unwrap();
        assert!(empty.is_static);

        let make = tn.functions.iter().find(|f| f.name == "makeStore").unwrap();
        assert!(make.is_public);
    }
}
