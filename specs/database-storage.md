# Database & Storage System

## Overview
SQLite-based metadata storage system for code analysis results with optimized schema design for fast queries and efficient storage.

## Database Schema

### Core Tables

#### repositories
```sql
CREATE TABLE repositories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    last_scan TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    file_count INTEGER DEFAULT 0,
    total_functions INTEGER DEFAULT 0,
    total_structs INTEGER DEFAULT 0,
    total_imports INTEGER DEFAULT 0,
    languages TEXT, -- JSON array: ["rust", "python", "typescript"]
    config_hash TEXT, -- Configuration fingerprint for invalidation
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_repositories_path ON repositories(path);
CREATE INDEX idx_repositories_last_scan ON repositories(last_scan);
```

#### files
```sql
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    absolute_path TEXT NOT NULL,
    language TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    content_hash TEXT NOT NULL, -- SHA-256 of file content
    last_modified TIMESTAMP NOT NULL,
    last_analyzed TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    parse_success BOOLEAN DEFAULT TRUE,
    parse_errors TEXT, -- JSON array of error objects
    functions_count INTEGER DEFAULT 0,
    structs_count INTEGER DEFAULT 0,
    imports_count INTEGER DEFAULT 0,
    exports_count INTEGER DEFAULT 0,
    lines_of_code INTEGER DEFAULT 0,
    complexity_score INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX idx_files_repo_path ON files(repo_id, relative_path);
CREATE INDEX idx_files_language ON files(language);
CREATE INDEX idx_files_content_hash ON files(content_hash);
CREATE INDEX idx_files_last_modified ON files(last_modified);
CREATE INDEX idx_files_parse_success ON files(parse_success);
```

#### functions
```sql
CREATE TABLE functions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    qualified_name TEXT, -- module::function or class.method
    parameters TEXT, -- JSON array: [{"name": "param", "type": "String", "default": null}]
    return_type TEXT,
    visibility TEXT CHECK(visibility IN ('public', 'private', 'protected', 'internal', 'package')),
    is_async BOOLEAN DEFAULT FALSE,
    is_static BOOLEAN DEFAULT FALSE,
    is_method BOOLEAN DEFAULT FALSE,
    is_constructor BOOLEAN DEFAULT FALSE,
    decorators TEXT, -- JSON array: ["@async", "@property"]
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_column INTEGER DEFAULT 0,
    end_column INTEGER DEFAULT 0,
    doc_comment TEXT,
    signature_hash TEXT NOT NULL, -- Hash of normalized signature
    complexity_score INTEGER DEFAULT 0,
    parameter_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_functions_file_id ON functions(file_id);
CREATE INDEX idx_functions_name ON functions(name);
CREATE INDEX idx_functions_qualified_name ON functions(qualified_name);
CREATE INDEX idx_functions_visibility ON functions(visibility);
CREATE INDEX idx_functions_is_async ON functions(is_async);
CREATE INDEX idx_functions_signature_hash ON functions(signature_hash);
CREATE INDEX idx_functions_start_line ON functions(start_line);
```

#### structs
```sql
CREATE TABLE structs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    qualified_name TEXT, -- module::Struct
    struct_type TEXT CHECK(struct_type IN ('struct', 'class', 'interface', 'enum', 'trait', 'union')),
    fields TEXT, -- JSON array: [{"name": "field", "type": "String", "visibility": "public"}]
    methods TEXT, -- JSON array of method IDs or signatures
    visibility TEXT CHECK(visibility IN ('public', 'private', 'protected', 'internal', 'package')),
    is_abstract BOOLEAN DEFAULT FALSE,
    is_interface BOOLEAN DEFAULT FALSE,
    inheritance TEXT, -- JSON array: ["BaseClass", "Interface1"]
    implementations TEXT, -- JSON array: ["Trait1", "Interface2"]
    generics TEXT, -- JSON array: ["T", "U where U: Clone"]
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_column INTEGER DEFAULT 0,
    end_column INTEGER DEFAULT 0,
    doc_comment TEXT,
    field_count INTEGER DEFAULT 0,
    method_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_structs_file_id ON structs(file_id);
CREATE INDEX idx_structs_name ON structs(name);
CREATE INDEX idx_structs_qualified_name ON structs(qualified_name);
CREATE INDEX idx_structs_struct_type ON structs(struct_type);
CREATE INDEX idx_structs_visibility ON structs(visibility);
CREATE INDEX idx_structs_start_line ON structs(start_line);
```

#### imports
```sql
CREATE TABLE imports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    module_path TEXT NOT NULL,
    import_type TEXT CHECK(import_type IN ('module', 'function', 'struct', 'enum', 'trait', 'constant', 'namespace', 'wildcard')),
    imported_name TEXT, -- Specific item being imported
    alias TEXT, -- Local alias for the import
    is_external BOOLEAN DEFAULT TRUE, -- External dependency vs internal module
    is_relative BOOLEAN DEFAULT FALSE, -- Relative vs absolute import
    line_number INTEGER NOT NULL,
    column_number INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_imports_file_id ON imports(file_id);
CREATE INDEX idx_imports_module_path ON imports(module_path);
CREATE INDEX idx_imports_import_type ON imports(import_type);
CREATE INDEX idx_imports_imported_name ON imports(imported_name);
CREATE INDEX idx_imports_is_external ON imports(is_external);
CREATE INDEX idx_imports_line_number ON imports(line_number);
```

#### exports
```sql
CREATE TABLE exports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    exported_name TEXT NOT NULL,
    export_type TEXT CHECK(export_type IN ('function', 'struct', 'enum', 'trait', 'constant', 'module', 'default')),
    qualified_name TEXT, -- Full qualified name
    is_default BOOLEAN DEFAULT FALSE,
    is_re_export BOOLEAN DEFAULT FALSE, -- Re-exported from another module
    original_module TEXT, -- Source module for re-exports
    line_number INTEGER NOT NULL,
    column_number INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_exports_file_id ON exports(file_id);
CREATE INDEX idx_exports_exported_name ON exports(exported_name);
CREATE INDEX idx_exports_export_type ON exports(export_type);
CREATE INDEX idx_exports_is_default ON exports(is_default);
CREATE INDEX idx_exports_line_number ON exports(line_number);
```

#### function_calls
```sql
CREATE TABLE function_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    caller_file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    caller_function_id INTEGER REFERENCES functions(id) ON DELETE SET NULL,
    called_function_name TEXT NOT NULL,
    called_qualified_name TEXT,
    called_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL, -- NULL for external calls
    called_function_id INTEGER REFERENCES functions(id) ON DELETE SET NULL,
    call_type TEXT CHECK(call_type IN ('direct', 'method', 'constructor', 'callback', 'async')),
    line_number INTEGER NOT NULL,
    column_number INTEGER DEFAULT 0,
    arguments_count INTEGER DEFAULT 0,
    is_external_call BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_function_calls_caller_file ON function_calls(caller_file_id);
CREATE INDEX idx_function_calls_caller_function ON function_calls(caller_function_id);
CREATE INDEX idx_function_calls_called_name ON function_calls(called_function_name);
CREATE INDEX idx_function_calls_called_file ON function_calls(called_file_id);
CREATE INDEX idx_function_calls_called_function ON function_calls(called_function_id);
CREATE INDEX idx_function_calls_line_number ON function_calls(line_number);
```

### Metadata Tables

#### dependencies
```sql
CREATE TABLE dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    source_file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    target_file_id INTEGER REFERENCES files(id) ON DELETE CASCADE, -- NULL for external deps
    dependency_type TEXT CHECK(dependency_type IN ('import', 'export', 'call', 'inheritance', 'composition')),
    strength INTEGER DEFAULT 1, -- Dependency strength (number of references)
    is_circular BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_dependencies_repo_id ON dependencies(repo_id);
CREATE INDEX idx_dependencies_source_file ON dependencies(source_file_id);
CREATE INDEX idx_dependencies_target_file ON dependencies(target_file_id);
CREATE INDEX idx_dependencies_type ON dependencies(dependency_type);
CREATE INDEX idx_dependencies_circular ON dependencies(is_circular);
```

#### analysis_metrics
```sql
CREATE TABLE analysis_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    file_id INTEGER REFERENCES files(id) ON DELETE CASCADE, -- NULL for repo-level metrics
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    metric_type TEXT CHECK(metric_type IN ('count', 'ratio', 'score', 'time', 'size')),
    calculated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_analysis_metrics_repo ON analysis_metrics(repo_id);
CREATE INDEX idx_analysis_metrics_file ON analysis_metrics(file_id);
CREATE INDEX idx_analysis_metrics_name ON analysis_metrics(metric_name);
CREATE INDEX idx_analysis_metrics_type ON analysis_metrics(metric_type);
```

## Database Operations

### Connection Management
```rust
pub struct DatabaseManager {
    connection_pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    database_path: PathBuf,
    max_connections: u32,
    pragma_settings: PragmaSettings,
}

pub struct PragmaSettings {
    pub journal_mode: String,     // WAL for better concurrency
    pub synchronous: String,      // NORMAL for balance
    pub cache_size: i32,          // Negative value = KB
    pub temp_store: String,       // MEMORY for temp tables
    pub mmap_size: i64,          // Memory-mapped I/O size
}

impl DatabaseManager {
    pub fn new(database_path: PathBuf) -> Result<Self> {
        // Initialize connection pool with optimized settings
    }
    
    pub fn execute_transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut rusqlite::Transaction) -> Result<R>
    {
        // Execute operations within a transaction
    }
}
```

### CRUD Operations

#### Repository Operations
```rust
impl DatabaseManager {
    pub fn insert_repository(&self, repo: &Repository) -> Result<i64> {
        // Insert new repository record
    }
    
    pub fn update_repository_stats(&self, repo_id: i64, stats: &RepoStats) -> Result<()> {
        // Update file counts and metrics
    }
    
    pub fn get_repository_by_path(&self, path: &Path) -> Result<Option<Repository>> {
        // Find repository by path
    }
}
```

#### File Operations
```rust
impl DatabaseManager {
    pub fn insert_file_batch(&self, files: &[FileRecord]) -> Result<Vec<i64>> {
        // Bulk insert files with prepared statement
    }
    
    pub fn update_file_hash(&self, file_id: i64, new_hash: &str) -> Result<()> {
        // Update content hash after file change
    }
    
    pub fn get_files_needing_analysis(&self, repo_id: i64) -> Result<Vec<FileRecord>> {
        // Find files with outdated analysis
    }
    
    pub fn delete_file_data(&self, file_id: i64) -> Result<()> {
        // Remove file and all related data (cascades)
    }
}
```

#### Function/Struct Operations
```rust
impl DatabaseManager {
    pub fn insert_functions_batch(&self, functions: &[FunctionRecord]) -> Result<()> {
        // Bulk insert functions
    }
    
    pub fn search_functions(&self, query: &FunctionQuery) -> Result<Vec<FunctionRecord>> {
        // Complex function search with filters
    }
    
    pub fn get_function_calls(&self, function_id: i64) -> Result<Vec<FunctionCall>> {
        // Find all calls to a specific function
    }
}
```

### Query Optimization

#### Prepared Statements
```rust
pub struct PreparedQueries {
    // Commonly used queries pre-compiled
    pub insert_file: rusqlite::Statement<'static>,
    pub insert_function: rusqlite::Statement<'static>,
    pub search_functions_by_name: rusqlite::Statement<'static>,
    pub get_file_dependencies: rusqlite::Statement<'static>,
}
```

#### Query Patterns
```sql
-- Efficient function search with text matching
CREATE VIRTUAL TABLE functions_fts USING fts5(
    name, qualified_name, doc_comment,
    content='functions', content_rowid='id'
);

-- Repository-wide statistics view
CREATE VIEW repo_statistics AS
SELECT 
    r.id,
    r.name,
    r.file_count,
    COUNT(DISTINCT f.language) as language_count,
    SUM(f.functions_count) as total_functions,
    SUM(f.structs_count) as total_structs,
    SUM(f.lines_of_code) as total_loc
FROM repositories r
JOIN files f ON r.id = f.repo_id
WHERE f.parse_success = TRUE
GROUP BY r.id;

-- Dependency graph query
WITH RECURSIVE dependency_chain(file_id, dependency_path, depth) AS (
    SELECT source_file_id, source_file_id::TEXT, 0
    FROM dependencies d
    WHERE source_file_id = ?
    
    UNION ALL
    
    SELECT d.target_file_id, 
           dc.dependency_path || ' -> ' || d.target_file_id,
           dc.depth + 1
    FROM dependencies d
    JOIN dependency_chain dc ON d.source_file_id = dc.file_id
    WHERE dc.depth < 10 AND d.target_file_id IS NOT NULL
)
SELECT * FROM dependency_chain;
```

## Performance Optimization

### Database Configuration
```toml
[database]
# SQLite pragma settings for performance
journal_mode = "WAL"           # Write-Ahead Logging for concurrency
synchronous = "NORMAL"         # Balance safety vs performance  
cache_size = -64000           # 64MB cache
temp_store = "MEMORY"         # Keep temp tables in memory
mmap_size = 268435456         # 256MB memory-mapped I/O
page_size = 4096              # Standard page size

# Connection pool settings
max_connections = 8
connection_timeout_seconds = 30
busy_timeout_ms = 5000
```

### Indexing Strategy
- Primary keys for all tables
- Foreign key indexes for joins
- Compound indexes for common query patterns
- Partial indexes for boolean columns
- FTS indexes for text search

### Batch Operations
- Use transactions for bulk inserts
- Prepared statements for repeated queries
- Batch size limits to prevent memory issues
- Progress reporting for long operations

## Data Integrity

### Constraints
- Foreign key constraints with CASCADE deletes
- Check constraints for enums
- Unique constraints for natural keys
- NOT NULL constraints for required fields

### Validation
```rust
pub trait DatabaseValidation {
    fn validate_before_insert(&self) -> Result<()>;
    fn validate_relationships(&self, db: &DatabaseManager) -> Result<()>;
}
```

### Backup Strategy
- Regular SQLite backups using `.backup` command
- Incremental backups for large repositories
- Corruption detection and repair
- Schema migration support

## Testing

### Unit Tests
- CRUD operations for each table
- Complex query correctness
- Transaction rollback scenarios
- Constraint violation handling

### Performance Tests
- Bulk insert benchmarks
- Query performance profiling
- Concurrent access testing
- Memory usage monitoring

### Integration Tests
- Full repository analysis workflow
- Incremental update scenarios
- Error recovery testing
- Schema migration testing 