# Loregrep Python Examples

This directory contains examples demonstrating how to use the Loregrep Python bindings.

## Installation

First, install the Python bindings locally:

```bash
# Install maturin (build tool for Python extensions)
pip install maturin

# Build and install Loregrep with Python bindings
maturin develop --features python
```

## Examples

### `basic_usage.py`
Comprehensive example showing the complete workflow:
- Creating a LoreGrep instance with builder pattern
- Scanning repositories for code analysis
- Executing AI analysis tools
- Working with scan results

```bash
python python/examples/basic_usage.py
```

### `test_bindings.py` 
Validation script for testing the Python bindings implementation:
- Tests API consistency
- Validates error handling
- Confirms async operations work correctly

```bash
python python/examples/test_bindings.py
```

## Key Features Demonstrated

### Builder Pattern Configuration
```python
loregrep_instance = (loregrep.LoreGrep.builder()
                   .max_file_size(1024 * 1024)  # 1MB max
                   .max_depth(10)                # Directory depth
                   .file_patterns(["*.py", "*.rs", "*.js", "*.ts"])
                   .exclude_patterns(["target/", "node_modules/"])
                   .respect_gitignore(True)
                   .build())
```

### Repository Scanning
```python
# Scan a directory and get statistics
scan_result = await loregrep_instance.scan("/path/to/repo")
print(f"Files scanned: {scan_result.files_scanned}")
print(f"Functions found: {scan_result.functions_found}")
print(f"Structs found: {scan_result.structs_found}")
```

### AI Tool Execution
```python
# Search for functions by pattern
func_result = await loregrep_instance.execute_tool("search_functions", {
    "pattern": "config",
    "limit": 10
})

# Analyze specific files
analyze_result = await loregrep_instance.execute_tool("analyze_file", {
    "file_path": "src/main.rs",
    "include_source": False
})

# Get repository structure
tree_result = await loregrep_instance.execute_tool("get_repository_tree", {
    "include_file_details": True,
    "max_depth": 2
})
```

## Available Tools

The Python bindings provide access to all 6 AI analysis tools:

1. **search_functions** - Find functions by name pattern
2. **search_structs** - Find structures/classes by name pattern  
3. **analyze_file** - Get detailed analysis of a specific file
4. **get_dependencies** - Find imports/exports for a file
5. **find_callers** - Get function call sites
6. **get_repository_tree** - Get repository structure and overview

## Error Handling

The bindings provide proper Python exception mapping:
- `IoError` → `OSError`
- `ToolError` → `RuntimeError` 
- `JsonError` → `ValueError`
- `AnalysisError` → `ValueError`

## Requirements

- Python 3.7+
- Rust toolchain
- maturin build tool

## Development

To rebuild after making changes to the Rust code:

```bash
maturin develop --features python
```

The changes will be immediately available in your Python environment.