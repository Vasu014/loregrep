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
    from . import loregrep as _extension
    from .loregrep import *
except ImportError as e:
    raise ImportError(
        "Failed to import loregrep Rust extension. "
        "Make sure the package was built correctly with maturin."
    ) from e


def _resolve_version() -> str:
    """Return the version of the extension module that is actually loaded.

    There is exactly one source of truth for this number: ``version`` in
    ``Cargo.toml``, compiled into the extension. Do NOT hardcode a copy here --
    a hardcoded copy cannot detect that it is sitting next to a stale ``.so``
    built from a different release, and will happily report a version that no
    code in the process implements.
    """
    version = getattr(_extension, "__version__", None)
    if version:
        return version
    # Older extensions did not set a module-level __version__ but have always
    # had the staticmethod.
    loregrep_class = getattr(_extension, "LoreGrep", None)
    if loregrep_class is not None:
        return loregrep_class.version()
    # No extension metadata at all: fall back to the installed distribution.
    from importlib.metadata import PackageNotFoundError, version as _dist_version

    try:
        return _dist_version("loregrep")
    except PackageNotFoundError:  # pragma: no cover - source tree without install
        return "unknown"


# Package metadata
__version__ = _resolve_version()
__author__ = "Vasu Bhardwaj"
__email__ = "voodoorapter014@gmail.com"

# Re-export main classes for convenient access - only the builder pattern API
__all__ = [
    "LoreGrep",           # Main API class
    "LoreGrepBuilder",    # Builder for configuration
    "ScanResult",         # Result of scanning operations
    "ToolResult",         # Result of tool execution
    "ToolSchema",         # Schema for available tools
    "IndexCoverage",      # Coverage of the current in-memory index
]