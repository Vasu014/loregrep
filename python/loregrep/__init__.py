"""
Loregrep: Repository indexing library for AI coding assistants

A high-performance repository indexing and code analysis library built in Rust
with Python bindings. Provides efficient code parsing, semantic analysis, and
search capabilities for AI-powered coding tools.

Key Features:
- Fast repository scanning and indexing
- Tree-sitter based code parsing for multiple languages
- Semantic code analysis and pattern matching
- AI-ready code representations
- Cross-language support (Rust, Python, JavaScript, TypeScript, and more)

Example usage:
    >>> import loregrep
    >>> # Create a LoreGrep instance using the builder pattern
    >>> loregrep_instance = (loregrep.LoreGrep.builder()
    ...                     .max_file_size(1024 * 1024)  # 1MB max
    ...                     .max_depth(10)
    ...                     .file_patterns(["*.py", "*.rs", "*.js"])
    ...                     .exclude_patterns(["target/", "node_modules/"])
    ...                     .respect_gitignore(True)
    ...                     .build())
    >>> 
    >>> # Scan a repository
    >>> result = await loregrep_instance.scan("/path/to/repo")
    >>> print(f"Processed {result.files_processed} files")
    >>> 
    >>> # Execute AI tools
    >>> tools = loregrep.LoreGrep.get_tool_definitions()
    >>> for tool in tools:
    ...     print(f"Available tool: {tool.name}")
"""

# Import the Rust extension module
try:
    from .loregrep import *
except ImportError as e:
    raise ImportError(
        "Failed to import loregrep Rust extension. "
        "Make sure the package was built correctly with maturin."
    ) from e

# Package metadata
__version__ = "0.3.1"
__author__ = "Vasu Bhardwaj"
__email__ = "voodoorapter014@gmail.com"

# Re-export main classes for convenient access - only the builder pattern API
__all__ = [
    "LoreGrep",           # Main API class
    "LoreGrepBuilder",    # Builder for configuration
    "ScanResult",         # Result of scanning operations
    "ToolResult",         # Result of tool execution
    "ToolSchema",         # Schema for available tools
]