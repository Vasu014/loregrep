# Loregrep Implementation Tasks

*Refactoring `src/tree-sitter.rs` into Full Code Analysis System*

---

## **Phase 1: Foundation & Core Architecture** (Week 1-2) ✅

### **P0 - Critical Foundation**

#### **Task 1.1: Project Structure & Module Organization** ✅
- [x] Create module structure:
  ```
  src/
  ├── main.rs              # CLI entry point
  ├── server.rs            # MCP server entry point  
  ├── lib.rs               # Library root
  ├── types/               # Common data structures
  │   ├── mod.rs
  │   ├── function.rs      # FunctionSignature, etc.
  │   ├── struct_def.rs    # StructSignature, etc.
  │   ├── analysis.rs      # TreeNode, RepoMap
  │   └── errors.rs        # Custom error types
  ├── analyzers/           # Language-specific analyzers
  │   ├── mod.rs
  │   ├── traits.rs        # LanguageAnalyzer trait
  │   ├── rust.rs          # RustAnalyzer
  │   ├── python.rs        # PythonAnalyzer
  │   ├── typescript.rs    # TypeScriptAnalyzer
  │   ├── javascript.rs    # JavaScriptAnalyzer
  │   └── go.rs            # GoAnalyzer
  ├── parser/              # Tree-sitter management
  │   ├── mod.rs
  │   ├── pool.rs          # Parser pooling
  │   └── cache.rs         # Parse result caching
  ├── scanner/             # Repository scanning
  │   ├── mod.rs
  │   ├── discovery.rs     # File discovery
  │   └── filters.rs       # Include/exclude patterns
  └── storage/             # In-memory storage (future: database)
      ├── mod.rs
      ├── memory.rs        # Enhanced RepoMap
      └── persistence.rs   # JSON/MessagePack serialization
  ```

#### **Task 1.2: Enhanced Data Structures** ✅
- [x] Update `FunctionSignature` for better memory efficiency:
  ```rust
  pub struct FunctionSignature {
      pub name: String,
      pub parameters: Vec<Parameter>,    // Not Vec<String>
      pub return_type: Option<String>,
      pub is_public: bool,
      pub is_async: bool,
      pub start_line: u32,
      pub end_line: u32,
      // Add later: signature_hash for deduplication
  }
  
  pub struct Parameter {
      pub name: String,
      pub param_type: String,
      pub default_value: Option<String>,
      pub is_mutable: bool,
  }
  ```

- [x] Update `StructSignature` with structured fields
- [x] Add missing types: `ImportStatement`, `ExportStatement`, `ParseError`
- [x] Replace `io::Error` with custom error types using `thiserror`

#### **Task 1.3: Enhanced LanguageAnalyzer Trait** ✅
- [x] Redesign trait to be async and comprehensive:
  ```rust
  #[async_trait]
  pub trait LanguageAnalyzer: Send + Sync {
      fn language(&self) -> &'static str;
      fn file_extensions(&self) -> &[&'static str];
      fn supports_async(&self) -> bool;
      
      async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis>;
      fn extract_functions(&self, tree: &Tree, source: &str) -> Result<Vec<FunctionSignature>>;
      fn extract_structs(&self, tree: &Tree, source: &str) -> Result<Vec<StructSignature>>;
      fn extract_imports(&self, tree: &Tree, source: &str) -> Result<Vec<ImportStatement>>;
      fn extract_exports(&self, tree: &Tree, source: &str) -> Result<Vec<ExportStatement>>;
      fn extract_function_calls(&self, tree: &Tree, source: &str) -> Result<Vec<FunctionCall>>;
      
      // Error recovery
      fn extract_with_fallback(&self, content: &str) -> PartialAnalysis;
  }
  ```

---

## **Phase 2: Enhanced In-Memory Storage** (Week 2) ✅

### **P0 - Critical Performance**

#### **Task 2.1: Enhanced RepoMap Implementation** ✅
- [x] Create enhanced RepoMap with fast lookups:
  ```rust
  pub struct RepoMap {
      // Core data
      files: Vec<TreeNode>,
      
      // Fast indexes
      file_index: HashMap<String, usize>,              // file_path -> index
      function_index: HashMap<String, Vec<usize>>,     // function_name -> file indices
      struct_index: HashMap<String, Vec<usize>>,       // struct_name -> file indices
      import_index: HashMap<String, Vec<usize>>,       // import_path -> file indices
      export_index: HashMap<String, Vec<usize>>,       // export_name -> file indices
      language_index: HashMap<String, Vec<usize>>,     // language -> file indices
      
      // Call graph
      call_graph: HashMap<String, Vec<CallSite>>,      // function_name -> call sites
      
      // Metadata
      metadata: RepoMapMetadata,
      
      // Memory management
      max_files: Option<usize>,
      
      // Query caching
      query_cache: HashMap<String, (Vec<usize>, SystemTime)>,
      cache_ttl_seconds: u64,
  }
  ```

#### **Task 2.2: Fast Query Operations** ✅
- [x] Implement efficient search methods:
  ```rust
  impl RepoMap {
      pub fn find_functions(&self, pattern: &str) -> QueryResult<&FunctionSignature>;
      pub fn find_structs(&self, pattern: &str) -> QueryResult<&StructSignature>;
      pub fn get_file_dependencies(&self, file_path: &str) -> Vec<String>;
      pub fn find_function_callers(&self, function_name: &str) -> Vec<CallSite>;
      pub fn get_files_by_language(&self, language: &str) -> Vec<&TreeNode>;
      pub fn get_changed_files(&self, since: SystemTime) -> Vec<&TreeNode>;
      pub fn fuzzy_search(&self, query: &str, limit: Option<usize>) -> Vec<(String, f64)>;
  }
  ```
- [x] Add regex and fuzzy matching support
- [x] Implement result caching for expensive queries
- [x] Add memory usage tracking and limits

#### **Task 2.3: Persistence & Serialization** ✅
- [x] Add JSON/Gzip serialization for startup speed:
  ```rust
  impl RepoMap {
      pub fn save_to_disk(&self, path: &Path) -> Result<()>;
      pub fn load_from_disk(path: &Path) -> Result<Self>;
      pub fn is_cache_valid(&self, repo_path: &Path) -> bool;
  }
  ```
- [x] Implement incremental cache updates with `IncrementalUpdateInfo`
- [x] Add compression for storage efficiency (Gzip support)
- [x] Create `PersistenceManager` for cache management
- [x] Add cache cleanup and versioning

---

## **Phase 3: Language Analyzer Refactoring** (Week 2-3)

### **P1 - Core Functionality**

#### **Task 3.1: Rust Analyzer Enhancement** ✅
- [x] Move current Rust parsing logic to `src/analyzers/rust.rs`
- [x] Fix parameter parsing (currently incomplete)
- [x] Add function call extraction
- [x] Improve struct field parsing
- [x] Add proper error recovery
- [x] Add support for:
  - [x] Const functions (`const fn`)
  - [x] Static functions (`static`)
  - [x] External functions (`extern`)
  - [x] Trait implementations
  - [x] Generics handling

#### **Task 3.2: Language Registry System**
- [ ] Create `LanguageAnalyzerRegistry`:
  ```rust
  pub struct LanguageAnalyzerRegistry {
      analyzers: HashMap<String, Box<dyn LanguageAnalyzer>>,
  }
  ```
- [ ] Implement language detection by:
  - [ ] File extension
  - [ ] Filename patterns
  - [ ] Content-based detection (shebangs, etc.)
- [ ] Add analyzer registration and lookup

#### **Task 3.3: Parser Pool Implementation**
- [ ] Create thread-safe parser pool to avoid recreation overhead
- [ ] Implement parser reuse and cleanup
- [ ] Add parser configuration management

---

## **Phase 4: Multi-Language Support** (Week 3-4)

### **P1 - Incremental Implementation**

#### **Task 4.1: Python Analyzer**
- [ ] Implement full Python analysis in `src/analyzers/python.rs`
- [ ] Add Python-specific query patterns
- [ ] Support async/await detection
- [ ] Handle class methods vs functions
- [ ] Add import resolution (relative vs absolute)

#### **Task 4.2: TypeScript Analyzer**
- [ ] Implement TypeScript analysis
- [ ] Handle interfaces, types, and classes
- [ ] Support import/export variations
- [ ] Add generic type extraction

#### **Task 4.3: JavaScript Analyzer**
- [ ] Implement JavaScript analysis
- [ ] Handle ES6+ features (arrow functions, destructuring)
- [ ] Support different module systems (CommonJS, ES modules)

#### **Task 4.4: Go Analyzer**
- [ ] Implement Go analysis
- [ ] Handle package declarations
- [ ] Support Go-specific function signatures
- [ ] Add interface and struct handling

---

## **Phase 5: Repository Scanning & File Management** (Week 4)

### **P1 - System Integration**

#### **Task 5.1: File Discovery System**
- [ ] Implement repository scanner in `src/scanner/`
- [ ] Add gitignore support using `ignore` crate
- [ ] Implement include/exclude pattern matching
- [ ] Add parallel file discovery

#### **Task 5.2: Change Detection & Incremental Updates**
- [ ] Implement file content hashing (blake3)
- [ ] Add modification time tracking
- [ ] Create incremental update detection:
  ```rust
  impl RepoMap {
      pub fn update_file(&mut self, file_path: &str) -> Result<UpdateResult>;
      pub fn remove_file(&mut self, file_path: &str) -> Result<()>;
      pub fn get_changed_files(&self, since: SystemTime) -> Vec<&str>;
  }
  ```
- [ ] Add file deletion handling
- [ ] Implement dependency invalidation (when imports change)

#### **Task 5.3: Configuration System**
- [ ] Create configuration structure matching `specs/` requirements
- [ ] Add TOML configuration file support
- [ ] Implement runtime configuration updates

---

## **Phase 6: Analysis Service Architecture** (Week 5)

### **P1 - Service Layer**

#### **Task 6.1: Analysis Service**
- [ ] Create main `AnalysisService` struct:
  ```rust
  pub struct AnalysisService {
      repo_map: Arc<RwLock<RepoMap>>,
      registry: LanguageAnalyzerRegistry,
      scanner: RepositoryScanner,
      config: AnalysisConfig,
  }
  ```
- [ ] Implement service lifecycle management
- [ ] Add service configuration and initialization
- [ ] Add thread-safe access to RepoMap

#### **Task 6.2: Batch Operations**
- [ ] Implement parallel file analysis
- [ ] Add progress tracking and reporting
- [ ] Implement graceful error handling for failed files
- [ ] Add analysis metrics collection

#### **Task 6.3: Memory Management & Limits**
- [ ] Add memory usage monitoring
- [ ] Implement memory pressure handling
- [ ] Add configurable memory limits
- [ ] Create LRU eviction for large repositories

---

## **Phase 7: Advanced Features** (Week 5-6)

### **P2 - Enhanced Functionality**

#### **Task 7.1: Function Call Analysis**
- [ ] Implement function call extraction across languages
- [ ] Build call graph construction in-memory
- [ ] Add cross-file function resolution
- [ ] Create call site tracking

#### **Task 7.2: Dependency Analysis**
- [ ] Implement import resolution
- [ ] Create dependency graph construction in-memory
- [ ] Add circular dependency detection
- [ ] Implement impact analysis

#### **Task 7.3: Query Engine Integration**
- [ ] Create query interface for the analysis service
- [ ] Implement pattern-based searching
- [ ] Add filtering and ranking
- [ ] Create result caching

---

## **Phase 8: Performance & Optimization** (Week 6-7)

### **P2 - Performance Targets**

#### **Task 8.1: Performance Optimization**
- [ ] Implement result caching strategies
- [ ] Add memory usage optimization
- [ ] Create benchmark tests
- [ ] Profile and optimize query performance

#### **Task 8.2: Parallel Processing**
- [ ] Implement worker thread pools
- [ ] Add async analysis pipeline
- [ ] Optimize parser pool usage
- [ ] Create processing queue management

#### **Task 8.3: Memory Efficiency**
- [ ] Optimize data structure sizes
- [ ] Implement string interning for common values
- [ ] Add compression for stored analysis data
- [ ] Create memory-mapped file support for large repos

---

## **Phase 9: Integration & Testing** (Week 7-8)

### **P1 - System Reliability**

#### **Task 9.1: Error Recovery**
- [ ] Implement graceful parse failure handling
- [ ] Add partial analysis results
- [ ] Create error reporting and logging
- [ ] Add retry mechanisms

#### **Task 9.2: Testing Suite**
- [ ] Create unit tests for all analyzers
- [ ] Add integration tests for full workflows
- [ ] Create performance benchmarks
- [ ] Add property-based tests for edge cases

#### **Task 9.3: MCP/CLI Integration Points**
- [ ] Create async-compatible interfaces for MCP server
- [ ] Add event emission for file analysis completion
- [ ] Create CLI-friendly output formatting
- [ ] Add progress reporting interfaces

---

## **Phase 10: Database Storage (Optional)** (Week 8+)

### **P3 - Future Enhancement**

#### **Task 10.1: Database Schema (When Needed)**
- [ ] Create SQLite schema from `specs/database-storage.md`
- [ ] Add migrations system for schema updates
- [ ] Implement connection pooling with `r2d2_sqlite`

#### **Task 10.2: Hybrid Storage Strategy**
- [ ] Create hybrid RepoMap that uses both memory and database
- [ ] Implement hot/cold data separation
- [ ] Add background persistence
- [ ] Create migration tools from in-memory to database

#### **Task 10.3: Advanced Database Features**
- [ ] Add historical analysis data
- [ ] Implement cross-repository queries
- [ ] Add advanced indexing strategies
- [ ] Create data export/import tools

---

## **Dependencies & Blocking Relationships**

```mermaid
graph TD
    A[Task 1.1: Module Structure] --> B[Task 1.2: Data Structures]
    B --> C[Task 1.3: Enhanced Trait]
    A --> D[Task 2.1: Optimized RepoMap]
    D --> E[Task 2.2: Fast Queries]
    E --> F[Task 2.3: Persistence]
    C --> G[Task 3.1: Rust Analyzer]
    G --> H[Task 3.2: Language Registry]
    H --> I[Task 4.x: Multi-Language]
    F --> J[Task 5.1: File Discovery]
    J --> K[Task 5.2: Change Detection]
    K --> L[Task 6.1: Analysis Service]
    L --> M[Task 7.x: Advanced Features]
    M --> N[Task 8.x: Performance]
    N --> O[Task 9.x: Integration]
    O --> P[Task 10.x: Database (Optional)]
```

## **Success Criteria**

### **Phase 1-3 Success:**
- [ ] Clean module structure with separated concerns
- [ ] Optimized in-memory RepoMap with fast lookups
- [ ] Enhanced Rust analyzer with all features

### **Phase 4-6 Success:**
- [ ] At least 3 languages fully supported
- [ ] Repository scanning and incremental updates working
- [ ] Service architecture ready for MCP/CLI integration
- [ ] Memory usage <100MB for repositories up to 50,000 files

### **Phase 7-9 Success:**
- [ ] Performance targets met (≤1s file analysis, ≤10s repo scan)
- [ ] All advanced features implemented
- [ ] Comprehensive test coverage
- [ ] Ready for production use with in-memory storage

### **Phase 10 Success (Optional):**
- [ ] Database storage available for very large repositories
- [ ] Hybrid storage strategy working
- [ ] Migration path from in-memory to database

## **Memory Usage Targets**

| Repository Size | Target Memory | Status |
|----------------|---------------|---------|
| Small (100 files) | <1MB | ✅ Excellent |
| Medium (1,000 files) | <10MB | ✅ Great |
| Large (10,000 files) | <100MB | ✅ Good |
| Very Large (50,000 files) | <500MB | ⚠️ Acceptable |
| Massive (100,000+ files) | Database recommended | 🔄 Future |

## **Notes**

- **In-Memory First**: Start with optimized in-memory storage for faster development and better performance
- **Database Optional**: Add database storage only when memory becomes a constraint (>50K files)
- **Incremental Migration**: Each phase builds on the previous, allowing gradual enhancement
- **Performance Focus**: Prioritize query speed and memory efficiency over persistent storage
- **Production Ready**: In-memory approach suitable for 95% of real-world projects
