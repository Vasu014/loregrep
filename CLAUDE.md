# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Loregrep is an AI-powered code analysis tool that combines Tree-sitter parsing with natural language queries via Anthropic's Claude API. It provides fast repository scanning, code analysis, and intelligent search capabilities.

## Common Development Commands

### Building and Running
```bash
# Build the release binary
cargo build --release

# Run the CLI
cargo run -- <command>

# Run with specific command
cargo run -- scan src --verbose
cargo run -- "What functions handle authentication?"
```

### Testing
```bash
# Run all tests
cargo test

# Run specific test modules
cargo test cli::tests
cargo test ai_tools::tests
cargo test conversation::tests

# Run with output
cargo test -- --nocapture
```

### Code Quality
```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Check without building
cargo check
```

## Architecture Overview

### Core Components

1. **Analyzers** (`src/analyzers/`) - Language-specific parsing using Tree-sitter
   - Implement `LanguageAnalyzer` trait for new languages
   - Currently supports Rust, with Python/TypeScript/JavaScript/Go planned

2. **Storage** (`src/storage/`) - In-memory repository indexing
   - `RepoMap` stores analyzed code structures
   - Fast lookup for functions, structs, imports

3. **AI Integration** - Natural language processing
   - `AnthropicClient` (`src/anthropic.rs`) - Claude API client
   - `LocalAnalysisTools` (`src/ai_tools.rs`) - Pseudo-MCP tools
   - `ConversationEngine` (`src/conversation.rs`) - Query processing

4. **CLI** (`src/cli.rs`) - Command-line interface
   - Main entry point for all commands
   - Routes to appropriate subsystems

### Key Design Patterns

- **Trait-based extensibility**: New languages implement `LanguageAnalyzer`
- **Builder patterns**: Used for complex configurations
- **Result-based error handling**: All operations return `Result<T, LoreError>`
- **Async/await**: Tokio runtime for concurrent operations

### Testing Strategy

- Unit tests embedded in source files
- Mock-based testing for external dependencies (Anthropic API)
- Integration tests for command execution
- Use `#[tokio::test]` for async tests

### Environment Setup

Required environment variable for AI features:
```bash
export ANTHROPIC_API_KEY="your-api-key"
```

### Adding New Features

When implementing new functionality:
1. Update relevant traits in `src/types/`
2. Add tests in the same file under `#[cfg(test)]`
3. Update CLI commands in `src/cli.rs` if needed
4. Document new tools in `src/ai_tools.rs` for AI integration

### Current Development Focus

The project is in active development with completed core functionality (Phases 1-3B). Current work focuses on:
- Performance optimization
- Multi-language support expansion
- Advanced analysis features (call graphs, dependencies)
- MCP server implementation