#!/usr/bin/env python3
"""
Enhanced API usage example for the loregrep Python package.

This example demonstrates the enhanced Python API of loregrep with real-time feedback
and convenient setup options for repository indexing and code analysis. It covers:

1. Importing and checking the loregrep package
2. Getting available AI tools for code analysis
3. Three enhanced setup options:
   - Auto-discovery (zero configuration)
   - Enhanced builder with convenience methods
   - Project-specific presets
4. Scanning a sample repository with multiple file types
5. Executing various analysis tools (search_functions, search_structs, etc.)

Prerequisites:
- Install with: maturin develop --features python
- Or build wheel with: maturin build --features python --release

The example creates temporary files for demonstration and cleans them up afterwards.
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
    
    # Create a LoreGrep instance using the enhanced API
    print("\n2. Creating LoreGrep instance with enhanced API...")
    
    # Demo 1: Zero-configuration auto-discovery
    print("   Option 1: Auto-discovery (zero configuration)")
    try:
        auto_loregrep = loregrep.LoreGrep.auto_discover(".")
        print("   ✅ Auto-discovery instance created successfully")
        print(f"      🔍 Detected project languages and configured analyzers")
    except Exception as e:
        print(f"   ⚠️  Auto-discovery failed: {e}")
        auto_loregrep = None
    
    # Demo 2: Enhanced builder with convenience methods
    print("\n   Option 2: Enhanced builder with convenience methods")
    try:
        enhanced_loregrep = (loregrep.LoreGrep.builder()
                           .with_rust_analyzer()         # ✅ Real-time feedback
                           .with_python_analyzer()       # ✅ Registration confirmation  
                           .optimize_for_performance()   # 🚀 Speed-optimized preset
                           .exclude_test_dirs()          # 🚫 Skip test directories
                           .build())                     # 🎆 Configuration summary
        print("   ✅ Enhanced builder instance created successfully")
    except Exception as e:
        print(f"   ⚠️  Enhanced builder failed: {e}")
        enhanced_loregrep = None
    
    # Demo 3: Project-specific presets
    print("\n   Option 3: Project-specific presets")
    try:
        preset_loregrep = loregrep.LoreGrep.polyglot_project(".")  # Multi-language preset
        print("   ✅ Polyglot project preset created successfully")
    except Exception as e:
        print(f"   ⚠️  Project preset failed: {e}")
        preset_loregrep = None
    
    # Use the first successful instance for the rest of the demo
    loregrep_instance = auto_loregrep or enhanced_loregrep or preset_loregrep
    if not loregrep_instance:
        print("❌ Failed to create any LoreGrep instance")
        return
        
    print(f"\n   📋 Using instance: {loregrep_instance}")
    
    # Create some sample files for demonstration
    print("\n3. Creating sample repository...")
    temp_dir = create_sample_repository()
    
    try:
        # Scan the sample repository
        print(f"\n4. Scanning repository at {temp_dir}...")
        result = await loregrep_instance.scan(temp_dir)
        
        print("✅ Repository scan completed!")
        print(f"   📁 Files scanned: {result.files_scanned}")
        print(f"   🔧 Functions found: {result.functions_found}")
        print(f"   📦 Structs found: {result.structs_found}")
        print(f"   ⏱️  Duration: {result.duration_ms}ms")
        
        # Demonstrate tool execution (if available)
        if len(tools) > 0:
            print(f"\n5. Demonstrating tool execution...")
            
            # Test search_functions tool
            try:
                print("   🔍 Testing search_functions tool...")
                func_result = await loregrep_instance.execute_tool("search_functions", {
                    "pattern": "Config",
                    "limit": 5
                })
                print(f"   ✅ search_functions executed successfully")
                print(f"   📄 Functions found:")
                print(f"   {func_result.content}")
                
            except Exception as e:
                print(f"   ⚠️  search_functions demo failed: {e}")
            
            # Test search_structs tool
            try:
                print("\n   🔍 Testing search_structs tool...")
                struct_result = await loregrep_instance.execute_tool("search_structs", {
                    "pattern": "Config",
                    "limit": 5
                })
                print(f"   ✅ search_structs executed successfully")
                print(f"   📦 Structs found:")
                print(f"   {struct_result.content}")
                
            except Exception as e:
                print(f"   ⚠️  search_structs demo failed: {e}")
            
            # Test get_repository_tree tool
            try:
                print("\n   🌳 Testing get_repository_tree tool...")
                tree_result = await loregrep_instance.execute_tool("get_repository_tree", {
                    "include_file_details": True,
                    "max_depth": 2
                })
                print(f"   ✅ get_repository_tree executed successfully")
                print(f"   🏗️  Repository structure:")
                print(f"   {tree_result.content}")
                
            except Exception as e:
                print(f"   ⚠️  get_repository_tree demo failed: {e}")
            
            # Test analyze_file tool (if we have Rust files)
            try:
                rust_file_path = os.path.join(temp_dir, "config.rs")
                if os.path.exists(rust_file_path):
                    print("\n   📄 Testing analyze_file tool...")
                    analyze_result = await loregrep_instance.execute_tool("analyze_file", {
                        "file_path": rust_file_path,
                        "include_source": False
                    })
                    print(f"   ✅ analyze_file executed successfully")
                    print(f"   🔍 File analysis (config.rs):")
                    # Parse and display the JSON result in a more readable way
                    import json
                    try:
                        analysis_data = json.loads(analyze_result.content)
                        if "functions" in analysis_data:
                            print(f"   📋 Functions: {len(analysis_data['functions'])}")
                            for func in analysis_data['functions'][:3]:  # Show first 3
                                print(f"     • {func.get('name', 'unknown')} (line {func.get('line_number', '?')})")
                        if "structs" in analysis_data:
                            print(f"   📦 Structs: {len(analysis_data['structs'])}")
                            for struct in analysis_data['structs'][:3]:  # Show first 3
                                print(f"     • {struct.get('name', 'unknown')} (line {struct.get('line_number', '?')})")
                    except json.JSONDecodeError:
                        # If it's not JSON, just show the raw content (truncated)
                        content_preview = analyze_result.content[:200] + "..." if len(analyze_result.content) > 200 else analyze_result.content
                        print(f"   {content_preview}")
                
            except Exception as e:
                print(f"   ⚠️  analyze_file demo failed: {e}")
        
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