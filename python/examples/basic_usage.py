#!/usr/bin/env python3
"""
Basic usage example for the loregrep Python package.

This example demonstrates how to use the builder pattern API of loregrep
for repository indexing and code analysis.
"""

import os
import tempfile
import asyncio
from pathlib import Path


async def main():
    """Main example function."""
    print("Loregrep Python Package - Builder Pattern API Example")
    print("=" * 55)
    
    try:
        import loregrep
        print(f"✅ Successfully imported loregrep v{loregrep.__version__}")
    except ImportError as e:
        print(f"❌ Failed to import loregrep: {e}")
        print("Make sure to build the package with: maturin develop --features python")
        return
    
    # Show available tools
    print("\n1. Available AI Tools:")
    tools = loregrep.LoreGrep.get_tool_definitions()
    for i, tool in enumerate(tools, 1):
        print(f"   {i}. {tool.name}: {tool.description}")
    
    # Create a LoreGrep instance using the builder pattern
    print("\n2. Creating LoreGrep instance with builder pattern...")
    try:
        loregrep_instance = (loregrep.LoreGrep.builder()
                           .max_file_size(1024 * 1024)  # 1MB max
                           .max_depth(10)
                           .file_patterns(["*.py", "*.rs", "*.js", "*.ts"])
                           .exclude_patterns(["target/", "node_modules/", "__pycache__/"])
                           .respect_gitignore(True)
                           .build())
        print("✅ LoreGrep instance created successfully")
    except Exception as e:
        print(f"❌ Failed to create LoreGrep instance: {e}")
        return
    
    # Create some sample files for demonstration
    print("\n3. Creating sample repository...")
    temp_dir = create_sample_repository()
    
    try:
        # Scan the sample repository
        print(f"\n4. Scanning repository at {temp_dir}...")
        result = await loregrep_instance.scan(temp_dir)
        
        print("✅ Repository scan completed!")
        print(f"   📁 Files processed: {result.files_processed}")
        print(f"   🔧 Functions found: {result.functions_found}")
        print(f"   📦 Structs found: {result.structs_found}")
        print(f"   ⏱️  Duration: {result.duration_ms}ms")
        
        # Demonstrate tool execution (if available)
        if len(tools) > 0:
            print(f"\n5. Demonstrating tool execution...")
            try:
                # Example: Try to execute the first available tool
                tool = tools[0]
                print(f"   Executing tool: {tool.name}")
                
                # Create sample arguments (this will vary by tool)
                args = {}
                
                tool_result = await loregrep_instance.execute_tool(tool.name, args)
                print(f"   ✅ Tool executed successfully")
                print(f"   📄 Content length: {len(tool_result.content)}")
                
            except Exception as e:
                print(f"   ⚠️  Tool execution demo skipped: {e}")
        
    except Exception as e:
        print(f"❌ Error during scanning: {e}")
    
    finally:
        # Clean up sample files
        cleanup_sample_repository(temp_dir)
        print("\n🧹 Sample repository cleaned up")


def create_sample_repository():
    """Create a temporary sample repository with various file types."""
    temp_dir = tempfile.mkdtemp(prefix="loregrep_example_")
    base_path = Path(temp_dir)
    
    # Python file
    python_file = base_path / "calculator.py"
    python_file.write_text('''
"""
A simple calculator module demonstrating Python code parsing.
"""

def fibonacci(n):
    """Calculate the nth Fibonacci number."""
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

class Calculator:
    """A simple calculator class."""
    
    def __init__(self):
        self.history = []
    
    def add(self, a, b):
        """Add two numbers."""
        result = a + b
        self.history.append(f"{a} + {b} = {result}")
        return result
    
    def multiply(self, a, b):
        """Multiply two numbers."""
        result = a * b
        self.history.append(f"{a} * {b} = {result}")
        return result
    
    def get_history(self):
        """Get calculation history."""
        return self.history.copy()
''')
    
    # JavaScript file
    js_file = base_path / "utils.js"
    js_file.write_text('''
/**
 * Utility functions for various operations.
 */

function calculateSum(numbers) {
    return numbers.reduce((sum, num) => sum + num, 0);
}

function findMax(numbers) {
    return Math.max(...numbers);
}

class UserManager {
    constructor() {
        this.users = new Map();
    }
    
    addUser(id, name, email) {
        this.users.set(id, { name, email, createdAt: new Date() });
    }
    
    getUser(id) {
        return this.users.get(id);
    }
    
    getAllUsers() {
        return Array.from(this.users.values());
    }
    
    deleteUser(id) {
        return this.users.delete(id);
    }
}

export { calculateSum, findMax, UserManager };
''')
    
    # Rust file
    rust_file = base_path / "config.rs"
    rust_file.write_text('''
//! Configuration management module
//! 
//! This module provides utilities for handling application configuration.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub name: String,
    pub values: HashMap<String, String>,
}

impl Config {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            values: HashMap::new(),
        }
    }
    
    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
    }
    
    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }
    
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        // Simple key=value parser
        let mut config = Config::new("loaded");
        
        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                config.set(key.trim(), value.trim());
            }
        }
        
        Ok(config)
    }
}

pub fn process_data(data: &[i32]) -> Vec<i32> {
    data.iter()
        .filter(|&&x| x > 0)
        .map(|&x| x * 2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = Config::new("test");
        assert_eq!(config.name, "test");
        assert!(config.values.is_empty());
    }
    
    #[test]
    fn test_process_data() {
        let input = vec![-1, 0, 1, 2, 3];
        let output = process_data(&input);
        assert_eq!(output, vec![2, 4, 6]);
    }
}
''')
    
    # Create a subdirectory with more files
    subdir = base_path / "lib"
    subdir.mkdir()
    
    # TypeScript file in subdirectory
    ts_file = subdir / "types.ts"
    ts_file.write_text('''
/**
 * Type definitions for the application
 */

export interface User {
    id: number;
    name: string;
    email: string;
    createdAt: Date;
}

export interface Config {
    apiUrl: string;
    timeout: number;
    retries: number;
}

export class ApiClient {
    private config: Config;
    
    constructor(config: Config) {
        this.config = config;
    }
    
    async fetchUser(id: number): Promise<User | null> {
        try {
            const response = await fetch(`${this.config.apiUrl}/users/${id}`);
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            return await response.json();
        } catch (error) {
            console.error('Failed to fetch user:', error);
            return null;
        }
    }
    
    async createUser(userData: Omit<User, 'id' | 'createdAt'>): Promise<User | null> {
        try {
            const response = await fetch(`${this.config.apiUrl}/users`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(userData)
            });
            
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            
            return await response.json();
        } catch (error) {
            console.error('Failed to create user:', error);
            return null;
        }
    }
}

export enum Status {
    Pending = 'pending',
    Success = 'success',
    Error = 'error'
}
''')
    
    return temp_dir


def cleanup_sample_repository(temp_dir):
    """Clean up the temporary sample repository."""
    import shutil
    try:
        shutil.rmtree(temp_dir)
    except OSError:
        pass  # Ignore cleanup errors


if __name__ == "__main__":
    asyncio.run(main()) 