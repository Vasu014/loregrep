# Loregrep Python Package

**Fast code analysis tools for AI coding assistants with complete file path tracking**

Loregrep is a high-performance repository indexing library that uses tree-sitter parsing to analyze codebases. It provides 6 standardized tools that supply structured code data to AI systems like Claude, GPT, and other coding assistants.

## ✨ **What's New in v0.4.2: Enhanced User Experience**

**Transform from complex setup to delightful developer experience:**

- 🎯 **Real-time feedback**: See analyzer registration and validation immediately  
- 📊 **Comprehensive summaries**: Detailed scan results with performance metrics
- 🔧 **Actionable errors**: Clear guidance when something goes wrong
- 🌟 **Professional polish**: Emoji indicators and user-friendly messages
- 📁 **Complete file path tracking**: Every code element includes its originating file

### Before vs After
```python
# Before: Silent, unclear feedback
lg = loregrep.LoreGrep.builder().build()

# After: Rich, helpful feedback
lg = (loregrep.LoreGrep.builder()
      .with_rust_analyzer()     # ✅ Rust analyzer registered successfully
      .with_python_analyzer()   # ✅ Python analyzer registered successfully  
      .build())                 # 🎆 LoreGrep configured with 2 languages

# Enhanced scan with comprehensive feedback
result = await lg.scan("./my-project")
# 🔍 Starting scan... 📁 Found X files... 📊 Summary with metrics
```

## 🎯 **Enhanced with File Path Storage for Better AI Integration**

**Every code element now includes its originating file path** - functions, structs, imports, exports, and function calls all include complete file location information for superior cross-file reference tracking.

```python
# Example: Functions now include file paths
functions = await lg.execute_tool("search_functions", {"pattern": "config"})
# Returns: {
#   "functions": [
#     {
#       "name": "parse_config",
#       "file_path": "src/config.py",     # ← Always included
#       "start_line": 45,
#       "signature": "def parse_config(path: str) -> Config"
#     }
#   ]
# }
```

**Why This Matters for AI Assistants:**
- **Cross-file Navigation**: AI can track where functions are defined vs. called
- **Impact Analysis**: Understand which files are affected by changes
- **Module Awareness**: AI knows the context and organization of your code
- **Refactoring Safety**: Track all references across the entire codebase

[![PyPI version](https://badge.fury.io/py/loregrep.svg)](https://badge.fury.io/py/loregrep)
[![Python 3.7+](https://img.shields.io/badge/python-3.7+-blue.svg)](https://www.python.org/downloads/)

## Quick Start

### Installation

```bash
pip install loregrep
```

### Basic Usage

```python
import asyncio
import loregrep

async def analyze_repository():
    # Easiest way: Zero-configuration auto-discovery (Recommended)
    lg = loregrep.LoreGrep.auto_discover(".")
    # 🔍 Detected project languages: rust, python
    # ✅ Rust analyzer registered successfully
    # ✅ Python analyzer registered successfully
    
    # Alternative: Enhanced builder for fine control
    # lg = (loregrep.LoreGrep.builder()
    #       .with_rust_analyzer()         # ✅ Real-time feedback
    #       .with_python_analyzer()       # ✅ Registration confirmation  
    #       .optimize_for_performance()   # 🚀 Speed-optimized preset
    #       .exclude_test_dirs()          # 🚫 Skip test directories
    #       .max_file_size(1024 * 1024)  # 1MB limit
    #       .build())                     # 🎆 Configuration summary
    
    # Alternative: Project-specific presets
    # lg = loregrep.LoreGrep.rust_project(".")      # Rust-optimized
    # lg = loregrep.LoreGrep.python_project(".")    # Python-optimized
    # lg = loregrep.LoreGrep.polyglot_project(".")  # Multi-language
    
    # Scan your repository with enhanced feedback
    result = await lg.scan("/path/to/your/project")
    # 🔍 Starting repository scan... 📁 Found X files... 📊 Scan Summary
    print(f"📁 Scanned {result.files_scanned} files")
    print(f"🔧 Found {result.functions_found} functions")
    print(f"📦 Found {result.structs_found} structures")
    
    # Search for functions
    functions = await lg.execute_tool("search_functions", {
        "pattern": "auth",
        "limit": 10
    })
    print("🔍 Authentication functions:")
    print(functions.content)
    
    # Get repository overview
    overview = await lg.execute_tool("get_repository_tree", {
        "include_file_details": True,
        "max_depth": 2
    })
    print("🌳 Repository structure:")
    print(overview.content)

# Run the analysis
asyncio.run(analyze_repository())
```

## AI Integration

Loregrep provides 6 standardized tools that supply structured code data to AI coding assistants:

### Available Tools

```python
# Get all available tools
tools = loregrep.LoreGrep.get_tool_definitions()
for tool in tools:
    print(f"🛠️  {tool.name}: {tool.description}")
```

#### 1. **search_functions** - Find functions by pattern (with file paths)
```python
result = await lg.execute_tool("search_functions", {
    "pattern": "config",
    "limit": 20
})

# Example response with file path information:
# {
#   "functions": [
#     {
#       "name": "parse_config",
#       "file_path": "src/config.py",
#       "start_line": 45,
#       "signature": "def parse_config(path: str) -> Config",
#       "is_async": False
#     },
#     {
#       "name": "load_config",
#       "file_path": "src/utils/loader.py",  # Different file!
#       "start_line": 12,
#       "signature": "async def load_config() -> Config",
#       "is_async": True
#     }
#   ]
# }
```

#### 2. **search_structs** - Find classes/structures by pattern  
```python
result = await lg.execute_tool("search_structs", {
    "pattern": "User",
    "limit": 10
})
```

#### 3. **analyze_file** - Detailed analysis of specific files
```python
result = await lg.execute_tool("analyze_file", {
    "file_path": "src/main.py",
    "include_source": False
})
```

#### 4. **get_dependencies** - Find imports and exports
```python
result = await lg.execute_tool("get_dependencies", {
    "file_path": "src/utils.py"
})
```

#### 5. **find_callers** - Locate function call sites (cross-file tracking)
```python
result = await lg.execute_tool("find_callers", {
    "function_name": "authenticate_user"
})

# Example response showing calls across multiple files:
# {
#   "callers": [
#     {
#       "function_name": "authenticate_user",
#       "file_path": "src/api/auth.py",        # Called from API module
#       "line_number": 23,
#       "caller_function": "login_endpoint"
#     },
#     {
#       "function_name": "authenticate_user", 
#       "file_path": "src/middleware/auth.py",  # Also called from middleware
#       "line_number": 45,
#       "caller_function": "auth_middleware"
#     },
#     {
#       "function_name": "authenticate_user",
#       "file_path": "tests/test_auth.py",     # And from tests
#       "line_number": 67,
#       "caller_function": "test_valid_user"
#     }
#   ]
# }
```

#### 6. **get_repository_tree** - Repository structure overview
```python
result = await lg.execute_tool("get_repository_tree", {
    "include_file_details": True,
    "max_depth": 3
})
```

## Configuration Options

### Enhanced Builder Pattern with Convenience Methods

```python
# Performance-optimized configuration
fast_loregrep = (loregrep.LoreGrep.builder()
    .with_rust_analyzer()           # ✅ Analyzer registration feedback
    .optimize_for_performance()     # 🚀 512KB limit, depth 8, skip binaries
    .exclude_test_dirs()            # 🚫 Skip test directories  
    .exclude_vendor_dirs()          # 🚫 Skip vendor/dependencies
    .build())                       # 🎆 Configuration summary

# Comprehensive analysis configuration  
thorough_loregrep = (loregrep.LoreGrep.builder()
    .with_all_analyzers()           # ✅ All available language analyzers
    .comprehensive_analysis()       # 🔍 5MB limit, depth 20, more file types
    .include_config_files()         # ✅ Include TOML, JSON, YAML configs
    .build())

# Traditional manual configuration (still supported)
manual_loregrep = (loregrep.LoreGrep.builder()
    .max_file_size(2 * 1024 * 1024)     # 2MB file size limit
    .max_depth(15)                       # Max directory depth
    .file_patterns(["*.py", "*.js", "*.ts", "*.rs"])
    .exclude_patterns([
        "node_modules/", "__pycache__/", "target/",
        ".git/", "venv/", ".env/"
    ])
    .respect_gitignore(True)             # Honor .gitignore files
    .build())
```

### Scan Results

```python
result = await lg.scan("/path/to/repo")

# Access scan statistics
print(f"Files scanned: {result.files_scanned}")
print(f"Functions found: {result.functions_found}")
print(f"Structs found: {result.structs_found}")
print(f"Duration: {result.duration_ms}ms")
print(f"Errors: {result.errors}")  # List of any scan errors
```

## How It Works

### Core Technology
- **Tree-sitter parsing**: Fast, accurate syntax analysis (not AI)
- **In-memory indexing**: Quick lookups and search (not AI)
- **Structured data extraction**: Functions, classes, imports, etc. (not AI)

### AI Integration
- **Tool interface**: 6 standardized tools that provide data **to** AI systems
- **AI assistants**: Use the structured data to answer questions about your code
- **LLM compatibility**: Works with Claude [Other integrations planned..]

*Loregrep does the fast parsing and indexing - AI systems use that data to understand your code.*

## Language Support

| Language   | Status     | Functions | Classes | Imports | 
|------------|------------|-----------|---------|---------|
| Rust       | ✅ Full    | ✅        | ✅      | ✅      |
| Python     | ✅ Full    | ✅        | ✅      | ✅      |
| TypeScript | 🚧 Planned | -         | -       | -       |
| JavaScript | 🚧 Planned | -         | -       | -       |

*Additional language support coming soon*

## Error Handling

```python
try:
    result = await lg.scan("/invalid/path")
except OSError as e:
    print(f"Path error: {e}")
except RuntimeError as e:
    print(f"Analysis error: {e}")
except ValueError as e:
    print(f"Configuration error: {e}")
```

## Async and Threading

Loregrep is fully async and thread-safe:

```python
import asyncio

# Multiple concurrent operations
async def parallel_analysis():
    lg1 = loregrep.LoreGrep.auto_discover("/project1")
    lg2 = loregrep.LoreGrep.auto_discover("/project2")
    
    # Concurrent scanning
    results = await asyncio.gather(
        lg1.scan("/project1"),
        lg2.scan("/project2")
    )
    
    # Concurrent tool execution
    analyses = await asyncio.gather(
        lg1.execute_tool("search_functions", {"pattern": "api"}),
        lg2.execute_tool("get_repository_tree", {"max_depth": 2})
    )
```

## Integration with AI Assistants

### Claude/OpenAI Integration

```python
# Get tool schemas for AI systems
tools = loregrep.LoreGrep.get_tool_definitions()

# Send to Claude/OpenAI as available tools
# When AI calls a tool, execute it:
result = await lg.execute_tool(tool_name, tool_args)

# Send result back to AI
ai_response = send_to_ai(result.content)
```

### Example: Enhanced Code Analysis Bot with File Path Awareness

```python
import json
from typing import List, Dict

async def code_analysis_bot(user_question: str, repo_path: str):
    lg = loregrep.LoreGrep.auto_discover(repo_path)
    await lg.scan(repo_path)
    
    if "functions" in user_question.lower():
        result = await lg.execute_tool("search_functions", {
            "pattern": extract_pattern(user_question),
            "limit": 10
        })
        
        # AI can now understand file organization
        response_data = json.loads(result.content)
        functions_by_file = {}
        for func in response_data.get("functions", []):
            file_path = func["file_path"]
            if file_path not in functions_by_file:
                functions_by_file[file_path] = []
            functions_by_file[file_path].append(func["name"])
        
        return f"Found functions across {len(functions_by_file)} files: {functions_by_file}"
    
    elif "impact" in user_question.lower():
        # Find potential impact of changing a function
        function_name = extract_function_name(user_question)
        
        # Find where it's defined
        definitions = await lg.execute_tool("search_functions", {
            "pattern": function_name,
            "limit": 1
        })
        
        # Find where it's called
        callers = await lg.execute_tool("find_callers", {
            "function_name": function_name
        })
        
        # Analyze impact across files
        def_data = json.loads(definitions.content)
        call_data = json.loads(callers.content)
        
        affected_files = set()
        if def_data.get("functions"):
            affected_files.add(def_data["functions"][0]["file_path"])
        
        for caller in call_data.get("callers", []):
            affected_files.add(caller["file_path"])
        
        return f"Changing '{function_name}' would affect {len(affected_files)} files: {list(affected_files)}"
    
    elif "structure" in user_question.lower():
        result = await lg.execute_tool("get_repository_tree", {
            "include_file_details": True,
            "max_depth": 2
        })
        return f"Repository structure: {result.content}"
    
    return "I can help analyze functions, impact, or structure. Please be more specific!"

# Helper functions
def extract_pattern(question: str) -> str:
    # Extract search pattern from user question
    # Implementation depends on your NLP approach
    pass

def extract_function_name(question: str) -> str:
    # Extract function name from user question
    # Implementation depends on your NLP approach
    pass
```


## Requirements

- Python 3.7+
- No external dependencies (uses native Rust extensions)

## Examples

See the [examples directory](https://github.com/your-repo/loregrep/tree/main/python/examples) for complete working examples:

- [`basic_usage.py`](https://github.com/your-repo/loregrep/blob/main/python/examples/basic_usage.py) - Complete workflow demonstration
- [`test_bindings.py`](https://github.com/your-repo/loregrep/blob/main/python/examples/test_bindings.py) - Comprehensive test suite

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

## Contributing

Contributions are welcome! Please see our [contribution guidelines](https://github.com/your-repo/loregrep/blob/main/CONTRIBUTING.md).