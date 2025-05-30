# Loregrep

**AI-Powered Code Analysis and Search Tool**

Loregrep is a modern command-line tool that combines traditional code analysis with AI-powered natural language queries. It helps developers understand, search, and analyze codebases using both structured commands and conversational queries.

## Features

### Core Capabilities
- **Repository Scanning**: Fast analysis of entire codebases with support for multiple languages
- **AI-Powered Queries**: Ask questions about your code in natural language
- **Code Search**: Find functions, structs, and other code elements with pattern matching
- **Dependency Analysis**: Understand import/export relationships and function call graphs
- **Interactive CLI**: Beautiful, responsive command-line interface with progress indicators

### Supported Languages
- **Rust** (full support)
- **Python, TypeScript, JavaScript, Go** (planned)

### AI Integration
- **Natural Language Queries**: "What functions handle authentication?" or "Show me all public structs"
- **Code Relationship Analysis**: "Find all callers of parse_config" or "What would break if I change this function?"
- **Contextual Understanding**: AI maintains conversation context for follow-up questions

## Installation

### Prerequisites
- Rust 1.70 or later
- An Anthropic API key (for AI features)

### From Source
```bash
git clone https://github.com/yourusername/loregrep.git
cd loregrep
cargo build --release
```

The binary will be available at `target/release/loregrep`.

### Configuration
Create a configuration file or set environment variables:

```bash
# Set API key
export ANTHROPIC_API_KEY="your-api-key-here"

# Or create a config file
loregrep config
```

## Quick Start

### Basic Repository Analysis
```bash
# Scan and analyze current directory
loregrep scan .

# Analyze a specific file
loregrep analyze src/main.rs

# Search for functions
loregrep search "parse_*" --type function
```

### AI-Powered Queries
```bash
# Ask questions about your code
loregrep "What functions handle file I/O?"
loregrep "Show me all error handling code"
loregrep "Find functions that could cause memory leaks"

# Analyze dependencies
loregrep "What modules depend on the config system?"
loregrep "Which functions are called most frequently?"
```

### Configuration Management
```bash
# View current configuration
loregrep config

# Scan with specific patterns
loregrep scan src --include "*.rs" --exclude "target/"
```

## Command Reference

### Repository Operations
- `loregrep scan <path>` - Scan and analyze a directory
- `loregrep analyze <file>` - Analyze a specific file
- `loregrep search <pattern>` - Search for code patterns

### AI Queries
- `loregrep "<natural language query>"` - Ask questions about your code
- Use quotes around natural language queries to distinguish from commands

### Configuration
- `loregrep config` - Show current configuration
- `loregrep --help` - Display help information

### Search Options
- `--type function|struct|import` - Filter by code element type
- `--language rust|python|typescript` - Filter by programming language
- `--limit <number>` - Limit number of results
- `--include <pattern>` - Include file patterns
- `--exclude <pattern>` - Exclude file patterns

## Configuration

Loregrep supports multiple configuration methods:

### Environment Variables
```bash
export ANTHROPIC_API_KEY="your-api-key"
export LOREGREP_CACHE_ENABLED=true
export LOREGREP_MAX_RESULTS=50
```

### Configuration File
Create `loregrep.toml` in your project root or home directory:

```toml
[scanning]
include_patterns = ["*.rs", "*.py"]
exclude_patterns = ["target/", "node_modules/", "*.test.js"]
max_file_size = 1048576  # 1MB
follow_symlinks = false

[analysis]
languages = ["rust", "python"]
cache_enabled = true
cache_path = ".loregrep/cache"

[output]
colors = true
verbose = false
max_results = 100

[ai]
anthropic_api_key = "your-key-here"  # Better to use env var
conversation_history = 10
```

### Command Line Arguments
All configuration options can be overridden via command line:
```bash
loregrep scan . --include "*.rs" --exclude "target/" --verbose
```

## Examples

### Code Discovery
```bash
# Find all public functions
loregrep search "pub fn" --type function

# Find specific struct patterns
loregrep search "*Config" --type struct

# Find error handling patterns
loregrep "How does this codebase handle errors?"
```

### Dependency Analysis
```bash
# Find function callers
loregrep "What calls the main function?"

# Analyze imports
loregrep "What modules import std::collections?"

# Impact analysis
loregrep "What would break if I rename this function?"
```

### Code Understanding
```bash
# Get overview
loregrep "Give me an overview of this codebase"

# Find entry points
loregrep "What are the main entry points?"

# Understand architecture
loregrep "How is this project structured?"
```

## Architecture

Loregrep is built with a modular architecture:

- **Analyzers**: Language-specific code analysis using Tree-sitter
- **Storage**: In-memory repository maps with fast indexing
- **Scanner**: File discovery with gitignore support
- **AI Tools**: Local analysis tools that work with Anthropic's Claude
- **CLI**: Command-line interface with enhanced user experience

## Performance

Typical performance characteristics:
- **Small repos** (100 files): <1 second analysis, <1MB memory
- **Medium repos** (1,000 files): <10 seconds analysis, <10MB memory  
- **Large repos** (10,000 files): <60 seconds analysis, <100MB memory

## Contributing

We welcome contributions! Areas where help is needed:

1. **Language Support**: Adding analyzers for Python, TypeScript, JavaScript, Go
2. **Performance**: Optimizing analysis speed and memory usage
3. **Features**: Advanced dependency analysis, code metrics, refactoring suggestions
4. **Testing**: Expanding test coverage and edge case handling

### Development Setup
```bash
git clone https://github.com/yourusername/loregrep.git
cd loregrep
cargo build
cargo test
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.


### Upcoming Features
- Multi-language support (Python, TypeScript, JavaScript, Go)
- Advanced dependency analysis and call graph visualization
- Performance optimizations for large repositories
- Integration with popular editors and IDEs
- Web interface for team collaboration

## Support

- **Issues**: Report bugs and request features on GitHub Issues
- **Discussions**: Join conversations on GitHub Discussions
- **Documentation**: Full documentation available in the `docs/` directory

---

**Note**: This project is under active development. APIs and command syntax may change between versions. Please check the changelog for breaking changes. 