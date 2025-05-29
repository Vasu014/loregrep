# Dependency Analysis & Call Graph System

## Overview
Comprehensive dependency analysis system that tracks import/export relationships, function call graphs, and provides impact analysis for code changes across multi-language codebases.

## Core Architecture

### Dependency Analyzer Components
```rust
pub struct DependencyAnalyzer {
    import_tracker: ImportTracker,
    call_graph_builder: CallGraphBuilder,
    dependency_resolver: DependencyResolver,
    impact_analyzer: ImpactAnalyzer,
    cycle_detector: CycleDetector,
    database: Arc<DatabaseManager>,
    config: DependencyConfig,
}

pub struct DependencyConfig {
    pub max_depth: usize,
    pub include_external_deps: bool,
    pub track_function_calls: bool,
    pub analyze_inheritance: bool,
    pub detect_cycles: bool,
    pub cache_dependency_graphs: bool,
    pub parallel_analysis: bool,
}
```

## Import/Export Tracking

### Import Analysis System
```rust
pub struct ImportTracker {
    resolvers: HashMap<String, Box<dyn ImportResolver>>,
    module_cache: ModuleCache,
    external_registry: ExternalDependencyRegistry,
}

pub trait ImportResolver: Send + Sync {
    fn language(&self) -> &str;
    fn resolve_import(&self, import: &ImportStatement, context: &ResolveContext) -> Result<ResolvedImport>;
    fn get_available_exports(&self, module_path: &str, context: &ResolveContext) -> Result<Vec<ExportedItem>>;
}

pub struct ResolveContext {
    pub current_file: PathBuf,
    pub project_root: PathBuf,
    pub language_config: LanguageConfig,
    pub workspace_layout: WorkspaceLayout,
}

pub struct ResolvedImport {
    pub import_statement: ImportStatement,
    pub resolved_path: Option<PathBuf>,
    pub resolution_type: ResolutionType,
    pub imported_symbols: Vec<ImportedSymbol>,
    pub is_external: bool,
    pub confidence: f64,
}

pub enum ResolutionType {
    LocalFile,           // Same project file
    LocalModule,         // Same project module/package
    WorkspaceDependency, // Other workspace package
    ExternalDependency,  // Third-party library
    SystemLibrary,       // Language standard library
    Unresolved,         // Could not resolve
}

pub struct ImportedSymbol {
    pub name: String,
    pub original_name: String,
    pub symbol_type: SymbolType,
    pub source_location: Option<SourceLocation>,
}

pub enum SymbolType {
    Function,
    Struct,
    Enum,
    Trait,
    Constant,
    Type,
    Module,
    Namespace,
    Unknown,
}
```

### Language-Specific Import Resolvers

#### Rust Import Resolver
```rust
pub struct RustImportResolver {
    cargo_registry: CargoRegistry,
    std_library_index: StdLibraryIndex,
}

impl ImportResolver for RustImportResolver {
    fn resolve_import(&self, import: &ImportStatement, context: &ResolveContext) -> Result<ResolvedImport> {
        let module_path = &import.module_path;
        
        // 1. Check if it's a relative import
        if module_path.starts_with("super") || module_path.starts_with("self") {
            return self.resolve_relative_import(import, context);
        }
        
        // 2. Check if it's a crate root import
        if module_path.starts_with("crate") {
            return self.resolve_crate_import(import, context);
        }
        
        // 3. Check workspace dependencies
        if let Some(resolved) = self.resolve_workspace_dependency(import, context)? {
            return Ok(resolved);
        }
        
        // 4. Check external crates
        if let Some(resolved) = self.resolve_external_crate(import, context)? {
            return Ok(resolved);
        }
        
        // 5. Check standard library
        if let Some(resolved) = self.resolve_std_library(import, context)? {
            return Ok(resolved);
        }
        
        // Unresolved
        Ok(ResolvedImport {
            import_statement: import.clone(),
            resolved_path: None,
            resolution_type: ResolutionType::Unresolved,
            imported_symbols: vec![],
            is_external: false,
            confidence: 0.0,
        })
    }
    
    fn resolve_relative_import(&self, import: &ImportStatement, context: &ResolveContext) -> Result<ResolvedImport> {
        let current_dir = context.current_file.parent().unwrap();
        let module_path = &import.module_path;
        
        // Handle super::module and self::module
        let resolved_path = if module_path.starts_with("super") {
            self.resolve_super_path(current_dir, module_path)?
        } else if module_path.starts_with("self") {
            self.resolve_self_path(current_dir, module_path)?
        } else {
            return Err(DependencyError::InvalidRelativeImport(module_path.clone()));
        };
        
        Ok(ResolvedImport {
            import_statement: import.clone(),
            resolved_path: Some(resolved_path),
            resolution_type: ResolutionType::LocalFile,
            imported_symbols: self.extract_symbols_from_file(&resolved_path, import)?,
            is_external: false,
            confidence: 0.95,
        })
    }
}
```

#### Python Import Resolver
```rust
pub struct PythonImportResolver {
    sys_path: Vec<PathBuf>,
    installed_packages: InstalledPackagesIndex,
    virtual_env: Option<VirtualEnvironment>,
}

impl ImportResolver for PythonImportResolver {
    fn resolve_import(&self, import: &ImportStatement, context: &ResolveContext) -> Result<ResolvedImport> {
        match import.import_type {
            ImportType::Module => self.resolve_module_import(import, context),
            ImportType::Function | ImportType::Struct => self.resolve_symbol_import(import, context),
            _ => self.resolve_generic_import(import, context),
        }
    }
    
    fn resolve_module_import(&self, import: &ImportStatement, context: &ResolveContext) -> Result<ResolvedImport> {
        let module_path = &import.module_path;
        
        // 1. Check relative imports
        if import.is_relative {
            return self.resolve_relative_python_import(import, context);
        }
        
        // 2. Check local modules
        if let Some(resolved) = self.find_local_module(module_path, context)? {
            return Ok(resolved);
        }
        
        // 3. Check installed packages
        if let Some(resolved) = self.find_installed_package(module_path)? {
            return Ok(resolved);
        }
        
        // 4. Check standard library
        if let Some(resolved) = self.find_stdlib_module(module_path)? {
            return Ok(resolved);
        }
        
        Ok(ResolvedImport {
            import_statement: import.clone(),
            resolved_path: None,
            resolution_type: ResolutionType::Unresolved,
            imported_symbols: vec![],
            is_external: false,
            confidence: 0.0,
        })
    }
}
```

#### TypeScript Import Resolver
```rust
pub struct TypeScriptImportResolver {
    tsconfig: Option<TSConfig>,
    node_modules: NodeModulesIndex,
    type_definitions: TypeDefinitionsIndex,
}

impl ImportResolver for TypeScriptImportResolver {
    fn resolve_import(&self, import: &ImportStatement, context: &ResolveContext) -> Result<ResolvedImport> {
        let module_path = &import.module_path;
        
        // 1. Check relative imports
        if module_path.starts_with('.') {
            return self.resolve_relative_ts_import(import, context);
        }
        
        // 2. Check path mapping from tsconfig.json
        if let Some(tsconfig) = &self.tsconfig {
            if let Some(resolved) = self.resolve_path_mapping(import, tsconfig, context)? {
                return Ok(resolved);
            }
        }
        
        // 3. Check node_modules
        if let Some(resolved) = self.resolve_node_module(import, context)? {
            return Ok(resolved);
        }
        
        // 4. Check type definitions
        if let Some(resolved) = self.resolve_type_definitions(import, context)? {
            return Ok(resolved);
        }
        
        Ok(ResolvedImport {
            import_statement: import.clone(),
            resolved_path: None,
            resolution_type: ResolutionType::Unresolved,
            imported_symbols: vec![],
            is_external: false,
            confidence: 0.0,
        })
    }
}
```

## Call Graph Analysis

### Call Graph Builder
```rust
pub struct CallGraphBuilder {
    function_call_extractors: HashMap<String, Box<dyn FunctionCallExtractor>>,
    call_graph_cache: CallGraphCache,
    database: Arc<DatabaseManager>,
}

pub trait FunctionCallExtractor: Send + Sync {
    fn extract_calls(&self, file_content: &str, tree: &Tree) -> Result<Vec<FunctionCall>>;
    fn extract_method_calls(&self, file_content: &str, tree: &Tree) -> Result<Vec<MethodCall>>;
    fn extract_constructor_calls(&self, file_content: &str, tree: &Tree) -> Result<Vec<ConstructorCall>>;
}

pub struct FunctionCall {
    pub caller_location: SourceLocation,
    pub called_function: String,
    pub qualified_name: Option<String>,
    pub call_type: CallType,
    pub arguments: Vec<CallArgument>,
    pub is_async: bool,
    pub confidence: f64,
}

pub enum CallType {
    Direct,      // function_name()
    Method,      // object.method()
    Static,      // Class::method()
    Constructor, // new Class()
    Callback,    // function passed as argument
    Indirect,    // function pointer/variable call
}

pub struct CallArgument {
    pub position: usize,
    pub value: Option<String>,
    pub argument_type: Option<String>,
    pub is_function: bool,
}

impl CallGraphBuilder {
    pub fn build_call_graph(&self, file_id: i64) -> Result<CallGraph> {
        let file_info = self.database.get_file_info(file_id)?;
        let file_content = std::fs::read_to_string(&file_info.path)?;
        
        // Get language-specific extractor
        let extractor = self.function_call_extractors
            .get(&file_info.language)
            .ok_or_else(|| DependencyError::UnsupportedLanguage(file_info.language.clone()))?;
        
        // Parse file to get AST
        let tree = self.parse_file(&file_content, &file_info.language)?;
        
        // Extract function calls
        let function_calls = extractor.extract_calls(&file_content, &tree)?;
        let method_calls = extractor.extract_method_calls(&file_content, &tree)?;
        let constructor_calls = extractor.extract_constructor_calls(&file_content, &tree)?;
        
        // Build call graph
        let mut call_graph = CallGraph::new(file_id);
        
        for call in function_calls {
            call_graph.add_function_call(call);
        }
        
        for call in method_calls {
            call_graph.add_method_call(call);
        }
        
        for call in constructor_calls {
            call_graph.add_constructor_call(call);
        }
        
        // Resolve call targets
        self.resolve_call_targets(&mut call_graph)?;
        
        Ok(call_graph)
    }
    
    fn resolve_call_targets(&self, call_graph: &mut CallGraph) -> Result<()> {
        for call in call_graph.calls.iter_mut() {
            // Try to resolve the called function to a specific file/function
            if let Some(target) = self.resolve_function_target(&call.called_function, call_graph.file_id)? {
                call.resolved_target = Some(target);
            }
        }
        Ok(())
    }
}

pub struct CallGraph {
    pub file_id: i64,
    pub calls: Vec<ResolvedCall>,
    pub incoming_calls: Vec<IncomingCall>,
    pub call_count: usize,
    pub complexity_score: f64,
}

pub struct ResolvedCall {
    pub call: FunctionCall,
    pub resolved_target: Option<CallTarget>,
}

pub struct CallTarget {
    pub target_file_id: Option<i64>,
    pub target_function_id: Option<i64>,
    pub target_signature: Option<String>,
    pub is_external: bool,
}

pub struct IncomingCall {
    pub caller_file_id: i64,
    pub caller_function_id: Option<i64>,
    pub call_location: SourceLocation,
}
```

### Language-Specific Call Extractors

#### Rust Call Extractor
```rust
pub struct RustCallExtractor;

impl FunctionCallExtractor for RustCallExtractor {
    fn extract_calls(&self, file_content: &str, tree: &Tree) -> Result<Vec<FunctionCall>> {
        let mut calls = Vec::new();
        let mut cursor = tree.walk();
        
        // Walk the AST looking for call expressions
        self.traverse_for_calls(&mut cursor, file_content, &mut calls)?;
        
        Ok(calls)
    }
    
    fn extract_method_calls(&self, file_content: &str, tree: &Tree) -> Result<Vec<MethodCall>> {
        let mut method_calls = Vec::new();
        let mut cursor = tree.walk();
        
        self.traverse_for_method_calls(&mut cursor, file_content, &mut method_calls)?;
        
        Ok(method_calls)
    }
}

impl RustCallExtractor {
    fn traverse_for_calls(&self, cursor: &mut TreeCursor, content: &str, calls: &mut Vec<FunctionCall>) -> Result<()> {
        loop {
            let node = cursor.node();
            
            match node.kind() {
                "call_expression" => {
                    if let Some(call) = self.extract_call_from_node(node, content)? {
                        calls.push(call);
                    }
                }
                "macro_invocation" => {
                    if let Some(call) = self.extract_macro_call(node, content)? {
                        calls.push(call);
                    }
                }
                _ => {}
            }
            
            // Recursively traverse children
            if cursor.goto_first_child() {
                self.traverse_for_calls(cursor, content, calls)?;
                cursor.goto_parent();
            }
            
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        
        Ok(())
    }
    
    fn extract_call_from_node(&self, node: Node, content: &str) -> Result<Option<FunctionCall>> {
        // Extract function name from call expression
        let function_node = node.child_by_field_name("function")
            .ok_or_else(|| DependencyError::MalformedCallExpression)?;
            
        let function_name = self.get_node_text(function_node, content)?;
        
        // Extract arguments
        let arguments = if let Some(args_node) = node.child_by_field_name("arguments") {
            self.extract_arguments(args_node, content)?
        } else {
            vec![]
        };
        
        // Determine call type
        let call_type = match function_node.kind() {
            "field_expression" => CallType::Method,
            "scoped_identifier" => CallType::Static,
            _ => CallType::Direct,
        };
        
        Ok(Some(FunctionCall {
            caller_location: SourceLocation::from_node(node),
            called_function: function_name,
            qualified_name: self.extract_qualified_name(function_node, content)?,
            call_type,
            arguments,
            is_async: self.is_async_call(node, content)?,
            confidence: 0.9,
        }))
    }
}
```

## Dependency Graph Construction

### Graph Builder
```rust
pub struct DependencyGraphBuilder {
    import_tracker: ImportTracker,
    call_graph_builder: CallGraphBuilder,
    database: Arc<DatabaseManager>,
}

impl DependencyGraphBuilder {
    pub fn build_repository_dependency_graph(&self, repo_id: i64) -> Result<DependencyGraph> {
        let files = self.database.get_repository_files(repo_id)?;
        let mut graph = DependencyGraph::new(repo_id);
        
        // Build import-based dependencies
        for file in &files {
            let file_dependencies = self.build_file_dependencies(file.id)?;
            graph.add_file_dependencies(file_dependencies);
        }
        
        // Build call-based dependencies
        for file in &files {
            let call_graph = self.call_graph_builder.build_call_graph(file.id)?;
            graph.add_call_dependencies(call_graph);
        }
        
        // Analyze inheritance relationships
        self.analyze_inheritance_dependencies(&mut graph)?;
        
        // Detect circular dependencies
        let cycles = self.detect_cycles(&graph)?;
        graph.circular_dependencies = cycles;
        
        Ok(graph)
    }
    
    fn build_file_dependencies(&self, file_id: i64) -> Result<FileDependencies> {
        let imports = self.database.get_file_imports(file_id)?;
        let mut dependencies = FileDependencies::new(file_id);
        
        for import in imports {
            let resolved_import = self.import_tracker.resolve_import(&import)?;
            
            if let Some(target_path) = resolved_import.resolved_path {
                if let Some(target_file_id) = self.database.get_file_id_by_path(&target_path)? {
                    dependencies.add_dependency(Dependency {
                        source_file_id: file_id,
                        target_file_id,
                        dependency_type: DependencyType::Import,
                        strength: 1,
                        import_info: Some(resolved_import),
                    });
                }
            }
        }
        
        Ok(dependencies)
    }
}

pub struct DependencyGraph {
    pub repo_id: i64,
    pub files: HashMap<i64, FileNode>,
    pub dependencies: Vec<Dependency>,
    pub circular_dependencies: Vec<CircularDependency>,
    pub metrics: GraphMetrics,
}

pub struct FileNode {
    pub file_id: i64,
    pub file_path: String,
    pub language: String,
    pub incoming_dependencies: Vec<i64>,
    pub outgoing_dependencies: Vec<i64>,
    pub dependency_score: f64,
}

pub struct Dependency {
    pub source_file_id: i64,
    pub target_file_id: i64,
    pub dependency_type: DependencyType,
    pub strength: u32,
    pub import_info: Option<ResolvedImport>,
}

pub enum DependencyType {
    Import,
    Export,
    FunctionCall,
    Inheritance,
    Composition,
}

pub struct CircularDependency {
    pub cycle_files: Vec<i64>,
    pub cycle_length: usize,
    pub severity: CycleSeverity,
}

pub enum CycleSeverity {
    Low,    // Import cycles that might be acceptable
    Medium, // Call cycles that could indicate design issues
    High,   // Strong cycles that definitely need attention
}
```

## Impact Analysis

### Change Impact Analyzer
```rust
pub struct ImpactAnalyzer {
    dependency_graph: Arc<DependencyGraph>,
    call_graph_cache: CallGraphCache,
    database: Arc<DatabaseManager>,
}

impl ImpactAnalyzer {
    pub fn analyze_function_change_impact(&self, function_id: i64) -> Result<ImpactAnalysis> {
        let function_info = self.database.get_function_info(function_id)?;
        let mut impact = ImpactAnalysis::new(ChangeType::FunctionModification, function_id);
        
        // Find all direct callers
        let direct_callers = self.database.get_function_callers(function_id)?;
        for caller in direct_callers {
            impact.add_direct_impact(caller);
        }
        
        // Find transitive impacts through call chain
        let transitive_impacts = self.find_transitive_impacts(function_id, 3)?; // max depth 3
        for transitive_impact in transitive_impacts {
            impact.add_transitive_impact(transitive_impact);
        }
        
        // Analyze interface changes
        if function_info.visibility == Visibility::Public {
            let interface_impacts = self.analyze_interface_change_impact(&function_info)?;
            impact.interface_impacts = interface_impacts;
        }
        
        // Calculate risk score
        impact.risk_score = self.calculate_risk_score(&impact);
        
        Ok(impact)
    }
    
    pub fn analyze_file_change_impact(&self, file_id: i64) -> Result<ImpactAnalysis> {
        let mut impact = ImpactAnalysis::new(ChangeType::FileModification, file_id);
        
        // Get all files that import this file
        let importing_files = self.dependency_graph.get_incoming_dependencies(file_id);
        for importing_file_id in importing_files {
            impact.add_direct_impact(importing_file_id);
        }
        
        // Get all files that call functions in this file
        let calling_files = self.database.get_files_calling_into(file_id)?;
        for calling_file_id in calling_files {
            impact.add_direct_impact(calling_file_id);
        }
        
        // Calculate transitive impacts
        let transitive_impacts = self.find_file_transitive_impacts(file_id, 2)?;
        for transitive_impact in transitive_impacts {
            impact.add_transitive_impact(transitive_impact);
        }
        
        impact.risk_score = self.calculate_risk_score(&impact);
        
        Ok(impact)
    }
    
    fn find_transitive_impacts(&self, function_id: i64, max_depth: usize) -> Result<Vec<TransitiveImpact>> {
        let mut impacts = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        queue.push_back((function_id, 0));
        
        while let Some((current_function_id, depth)) = queue.pop_front() {
            if depth >= max_depth || visited.contains(&current_function_id) {
                continue;
            }
            
            visited.insert(current_function_id);
            
            let callers = self.database.get_function_callers(current_function_id)?;
            for caller in callers {
                impacts.push(TransitiveImpact {
                    affected_function_id: caller.function_id,
                    affected_file_id: caller.file_id,
                    impact_depth: depth + 1,
                    impact_strength: 1.0 / (depth + 1) as f64, // Decreasing strength with depth
                });
                
                queue.push_back((caller.function_id, depth + 1));
            }
        }
        
        Ok(impacts)
    }
    
    fn calculate_risk_score(&self, impact: &ImpactAnalysis) -> f64 {
        let mut score = 0.0;
        
        // Base score from direct impacts
        score += impact.direct_impacts.len() as f64 * 2.0;
        
        // Add transitive impacts with decreasing weight
        for transitive in &impact.transitive_impacts {
            score += transitive.impact_strength;
        }
        
        // Interface changes are higher risk
        if !impact.interface_impacts.is_empty() {
            score *= 1.5;
        }
        
        // Normalize to 0-10 scale
        (score / 10.0).min(10.0)
    }
}

pub struct ImpactAnalysis {
    pub change_type: ChangeType,
    pub changed_item_id: i64,
    pub direct_impacts: Vec<DirectImpact>,
    pub transitive_impacts: Vec<TransitiveImpact>,
    pub interface_impacts: Vec<InterfaceImpact>,
    pub risk_score: f64,
    pub analysis_timestamp: DateTime<Utc>,
}

pub enum ChangeType {
    FunctionModification,
    FunctionDeletion,
    FunctionSignatureChange,
    FileModification,
    FileDeletion,
    StructModification,
    InterfaceChange,
}

pub struct DirectImpact {
    pub affected_file_id: i64,
    pub affected_function_id: Option<i64>,
    pub impact_type: ImpactType,
}

pub struct TransitiveImpact {
    pub affected_function_id: i64,
    pub affected_file_id: i64,
    pub impact_depth: usize,
    pub impact_strength: f64,
}

pub enum ImpactType {
    ImportBreakage,
    CallSiteUpdate,
    TypeCheckFailure,
    InterfaceViolation,
}
```

## Cycle Detection

### Circular Dependency Detection
```rust
pub struct CycleDetector {
    graph: Arc<DependencyGraph>,
}

impl CycleDetector {
    pub fn detect_all_cycles(&self) -> Result<Vec<CircularDependency>> {
        let mut all_cycles = Vec::new();
        
        // Detect import cycles
        let import_cycles = self.detect_import_cycles()?;
        all_cycles.extend(import_cycles);
        
        // Detect call cycles
        let call_cycles = self.detect_call_cycles()?;
        all_cycles.extend(call_cycles);
        
        // Sort by severity
        all_cycles.sort_by(|a, b| b.severity.cmp(&a.severity));
        
        Ok(all_cycles)
    }
    
    fn detect_import_cycles(&self) -> Result<Vec<CircularDependency>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();
        let mut path = Vec::new();
        
        for file_id in self.graph.files.keys() {
            if !visited.contains(file_id) {
                self.dfs_import_cycle_detection(
                    *file_id,
                    &mut visited,
                    &mut recursion_stack,
                    &mut path,
                    &mut cycles,
                )?;
            }
        }
        
        Ok(cycles)
    }
    
    fn dfs_import_cycle_detection(
        &self,
        file_id: i64,
        visited: &mut HashSet<i64>,
        recursion_stack: &mut HashSet<i64>,
        path: &mut Vec<i64>,
        cycles: &mut Vec<CircularDependency>,
    ) -> Result<()> {
        visited.insert(file_id);
        recursion_stack.insert(file_id);
        path.push(file_id);
        
        // Get all files this file imports
        let dependencies = self.graph.get_outgoing_dependencies(file_id);
        
        for dep_file_id in dependencies {
            if recursion_stack.contains(&dep_file_id) {
                // Found a cycle - extract the cycle from the path
                if let Some(cycle_start) = path.iter().position(|&id| id == dep_file_id) {
                    let cycle_files = path[cycle_start..].to_vec();
                    cycles.push(CircularDependency {
                        cycle_files,
                        cycle_length: path.len() - cycle_start,
                        severity: self.determine_import_cycle_severity(&path[cycle_start..]),
                    });
                }
            } else if !visited.contains(&dep_file_id) {
                self.dfs_import_cycle_detection(
                    dep_file_id,
                    visited,
                    recursion_stack,
                    path,
                    cycles,
                )?;
            }
        }
        
        recursion_stack.remove(&file_id);
        path.pop();
        
        Ok(())
    }
    
    fn determine_import_cycle_severity(&self, cycle: &[i64]) -> CycleSeverity {
        // Analyze the types of dependencies in the cycle
        let mut has_strong_coupling = false;
        let mut has_inheritance = false;
        
        for window in cycle.windows(2) {
            let source = window[0];
            let target = window[1];
            
            if let Some(deps) = self.graph.get_dependencies_between(source, target) {
                for dep in deps {
                    match dep.dependency_type {
                        DependencyType::Inheritance => has_inheritance = true,
                        DependencyType::FunctionCall => has_strong_coupling = true,
                        _ => {}
                    }
                }
            }
        }
        
        match (has_inheritance, has_strong_coupling, cycle.len()) {
            (true, _, _) => CycleSeverity::High,
            (_, true, len) if len <= 3 => CycleSeverity::High,
            (_, true, _) => CycleSeverity::Medium,
            (_, _, len) if len > 5 => CycleSeverity::Medium,
            _ => CycleSeverity::Low,
        }
    }
}
```

## Metrics & Visualization

### Dependency Metrics
```rust
pub struct DependencyMetrics {
    pub total_files: usize,
    pub total_dependencies: usize,
    pub average_dependencies_per_file: f64,
    pub max_dependencies_per_file: usize,
    pub circular_dependency_count: usize,
    pub dependency_depth_distribution: Vec<usize>,
    pub coupling_score: f64,
    pub cohesion_score: f64,
}

impl DependencyGraph {
    pub fn calculate_metrics(&self) -> DependencyMetrics {
        let total_files = self.files.len();
        let total_dependencies = self.dependencies.len();
        
        let dependencies_per_file: Vec<usize> = self.files
            .values()
            .map(|file| file.outgoing_dependencies.len())
            .collect();
            
        let average_dependencies_per_file = if total_files > 0 {
            total_dependencies as f64 / total_files as f64
        } else {
            0.0
        };
        
        let max_dependencies_per_file = dependencies_per_file.iter().max().copied().unwrap_or(0);
        
        let coupling_score = self.calculate_coupling_score();
        let cohesion_score = self.calculate_cohesion_score();
        
        DependencyMetrics {
            total_files,
            total_dependencies,
            average_dependencies_per_file,
            max_dependencies_per_file,
            circular_dependency_count: self.circular_dependencies.len(),
            dependency_depth_distribution: self.calculate_depth_distribution(),
            coupling_score,
            cohesion_score,
        }
    }
    
    fn calculate_coupling_score(&self) -> f64 {
        // Calculate afferent and efferent coupling
        let mut total_coupling = 0.0;
        
        for file in self.files.values() {
            let efferent_coupling = file.outgoing_dependencies.len() as f64;
            let afferent_coupling = file.incoming_dependencies.len() as f64;
            
            // Instability metric: Ce / (Ca + Ce)
            let instability = if afferent_coupling + efferent_coupling > 0.0 {
                efferent_coupling / (afferent_coupling + efferent_coupling)
            } else {
                0.0
            };
            
            total_coupling += instability;
        }
        
        if self.files.is_empty() {
            0.0
        } else {
            total_coupling / self.files.len() as f64
        }
    }
}
```

## Configuration

### Dependency Analysis Configuration
```toml
[dependency_analysis]
max_depth = 5
include_external_deps = false
track_function_calls = true
analyze_inheritance = true
detect_cycles = true
cache_dependency_graphs = true
parallel_analysis = true

[import_resolution]
timeout_seconds = 30
confidence_threshold = 0.7
resolve_external_packages = true
follow_symlinks = false

[call_graph]
extract_method_calls = true
extract_constructor_calls = true
extract_callback_calls = false
confidence_threshold = 0.8

[impact_analysis]
max_transitive_depth = 3
include_test_files = false
weight_by_usage_frequency = true
consider_visibility = true

[cycle_detection]
max_cycle_length = 10
severity_thresholds = { low = 2, medium = 4, high = 6 }
ignore_test_cycles = true
```

## Testing

### Unit Tests
- Import resolution accuracy for each language
- Call graph extraction correctness
- Cycle detection algorithms
- Impact analysis precision

### Integration Tests
- Multi-language dependency resolution
- Large codebase performance
- Real-world cycle detection
- Change impact accuracy

### Performance Tests
- Dependency graph construction time
- Memory usage for large repositories
- Incremental update performance
- Query response times 