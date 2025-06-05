# Loregrep Python Examples

This directory contains examples demonstrating how to use the Loregrep Python bindings. Each example focuses on a specific aspect of the API to showcase different capabilities.

## Installation

First, install the Python bindings locally:

```bash
# Install maturin (build tool for Python extensions)
pip install maturin

# Build and install Loregrep with Python bindings
maturin develop --features python
```

## Examples

All examples follow a numbered naming convention for easy navigation:

### Core Setup and Configuration

**`01_auto_discovery.py`** - Zero Configuration Setup  
Demonstrates automatic project type detection and analyzer configuration.
```bash
python python/examples/01_auto_discovery.py
```

**`02_enhanced_builder.py`** - Enhanced Builder Pattern  
Shows advanced builder configuration with convenience methods and real-time feedback.
```bash
python python/examples/02_enhanced_builder.py
```

**`03_project_presets.py`** - Language-Specific Presets  
Demonstrates project-specific preset methods for Rust, Python, and polyglot projects.
```bash
python python/examples/03_project_presets.py
```

### Analysis Tools and Features

**`04_tool_showcase.py`** - Complete Tool Demonstration  
Comprehensive showcase of all 6 AI analysis tools with detailed examples.
```bash
python python/examples/04_tool_showcase.py
```

**`05_basic_usage.py`** - Simple Getting Started  
Basic usage example showing the minimal code needed to get started.
```bash
python python/examples/05_basic_usage.py
```

**`06_simple_scan.py`** - Basic Repository Scanning  
Shows the minimal code needed to scan a repository and get results.
```bash
python python/examples/06_simple_scan.py
```

**`07_builder_config.py`** - Advanced Builder Configuration  
Detailed builder pattern usage with various configuration options.
```bash
python python/examples/07_builder_config.py
```

### Specific Tool Examples

**`08_function_search.py`** - Function Search  
Finding functions by pattern using the search_functions tool.
```bash
python python/examples/08_function_search.py
```

**`09_struct_search.py`** - Structure/Class Search  
Finding classes and structures using the search_structs tool.
```bash
python python/examples/09_struct_search.py
```

**`10_file_analysis.py`** - File Analysis  
Detailed analysis of individual files using the analyze_file tool.
```bash
python python/examples/10_file_analysis.py
```

**`11_repository_tree.py`** - Repository Structure  
Getting project structure overview using the get_repository_tree tool.
```bash
python python/examples/11_repository_tree.py
```

**`12_dependency_analysis.py`** - Dependency Analysis  
Understanding import/export relationships using get_dependencies and find_callers tools.
```bash
python python/examples/12_dependency_analysis.py
```

### Testing and Validation

**`13_test_bindings.py`** - Comprehensive Testing  
Validation script for testing the Python bindings with detailed test coverage.
```bash
python python/examples/13_test_bindings.py
```

## Quick Start

For first-time users, run the examples in this order:
1. `01_auto_discovery.py` - Learn the simplest setup method
2. `06_simple_scan.py` - Understand basic scanning
3. `08_function_search.py` - Try searching for functions
4. `04_tool_showcase.py` - See all available tools

## Available AI Analysis Tools

The Python bindings provide access to 6 standardized AI analysis tools:

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