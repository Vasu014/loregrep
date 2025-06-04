# Loregrep Python Package

**Fast code analysis tools for AI coding assistants**

Loregrep is a high-performance repository indexing library that uses tree-sitter parsing to analyze codebases. It provides 6 standardized tools that supply structured code data to AI systems like Claude, GPT, and other coding assistants.

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
    # Create and configure analyzer
    lg = (loregrep.LoreGrep.builder()
          .max_file_size(1024 * 1024)  # 1MB limit
          .file_patterns(["*.py", "*.rs", "*.js", "*.ts"])
          .exclude_patterns(["node_modules/", "__pycache__/", "target/"])
          .build())
    
    # Scan your repository
    result = await lg.scan("/path/to/your/project")
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

#### 1. **search_functions** - Find functions by pattern
```python
result = await lg.execute_tool("search_functions", {
    "pattern": "config",
    "limit": 20
})
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

#### 5. **find_callers** - Locate function call sites
```python
result = await lg.execute_tool("find_callers", {
    "function_name": "authenticate_user"
})
```

#### 6. **get_repository_tree** - Repository structure overview
```python
result = await lg.execute_tool("get_repository_tree", {
    "include_file_details": True,
    "max_depth": 3
})
```

## Configuration Options

### Builder Pattern

```python
loregrep_instance = (loregrep.LoreGrep.builder()
    # File size and depth limits
    .max_file_size(2 * 1024 * 1024)     # 2MB max file size
    .max_depth(15)                       # Max directory depth
    
    # File filtering
    .file_patterns(["*.py", "*.js", "*.ts", "*.rs"])
    .exclude_patterns([
        "node_modules/", "__pycache__/", "target/",
        ".git/", "venv/", ".env/"
    ])
    
    # Git integration
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
| Python     | 🚧 Planned | -         | -       | -       |
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
    lg1 = loregrep.LoreGrep.builder().build()
    lg2 = loregrep.LoreGrep.builder().build()
    
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

### Example: Code Analysis Bot

```python
async def code_analysis_bot(user_question: str, repo_path: str):
    lg = loregrep.LoreGrep.builder().build()
    await lg.scan(repo_path)
    
    if "functions" in user_question.lower():
        result = await lg.execute_tool("search_functions", {
            "pattern": extract_pattern(user_question),
            "limit": 10
        })
    elif "structure" in user_question.lower():
        result = await lg.execute_tool("get_repository_tree", {
            "include_file_details": True,
            "max_depth": 2
        })
    
    return f"Found: {result.content}"
```

## Performance

- **Scanning**: ~1-2 seconds for medium repositories (1000+ files)
- **Queries**: ~50-100ms for most operations
- **Memory**: ~50MB for typical projects
- **Concurrency**: Thread-safe with multiple instances

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