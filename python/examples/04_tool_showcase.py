#!/usr/bin/env python3
"""
Tool Showcase Example - Demonstration of All 6 AI Analysis Tools

Demonstrates all 6 standardized AI analysis tools:
1. search_functions - Find functions by pattern
2. search_structs - Find structures/classes by pattern  
3. analyze_file - Detailed file analysis
4. get_dependencies - Import/export analysis
5. find_callers - Function call site analysis
6. get_repository_tree - Repository structure overview
"""

import asyncio
import tempfile
import json
from pathlib import Path
import loregrep


async def main():
    print("🛠️  LoreGrep Tool Showcase")
    print("=" * 32)
    
    # Create LoreGrep instance (try different methods)
    try:
        instance = loregrep.LoreGrep.polyglot_project(".")
        print("✅ Using polyglot preset")
    except Exception:
        try:
            instance = loregrep.LoreGrep.auto_discover(".")
            print("✅ Using auto-discovery")
        except Exception:
            instance = loregrep.LoreGrep.builder().build()
            print("✅ Using basic builder")
    
    # Create a sample project for testing
    print("\n📁 Creating sample project...")
    sample_dir = create_sample_project()
    
    try:
        # Scan the sample project
        print(f"\n🔍 Scanning...")
        result = await instance.scan(sample_dir)
        print(f"✅ Files: {result.files_scanned}, Functions: {result.functions_found}")
        
        # Get available tools
        tools = loregrep.LoreGrep.get_tool_definitions()
        print(f"\n🛠️  Available tools: {len(tools)}")
        
        # Demonstrate each tool
        await demo_tools(instance, sample_dir)
        
    finally:
        cleanup_sample_project(sample_dir)
        print("\n🧹 Cleanup complete")


async def demo_tools(instance, sample_dir):
    """Demonstrate each AI analysis tool."""
    
    tools_to_test = [
        ("search_functions", {"pattern": "load", "limit": 3}),
        ("search_structs", {"pattern": "Manager", "limit": 3}),  
        ("get_repository_tree", {"max_depth": 2}),
        ("get_dependencies", {"file_path": str(next(Path(sample_dir).glob("*.py"), ""))}),
        ("find_callers", {"function_name": "load_config"}),
        ("analyze_file", {"file_path": str(next(Path(sample_dir).glob("*.py"), "")), "include_source": False}),
    ]
    
    for i, (tool_name, params) in enumerate(tools_to_test, 1):
        print(f"\n{i}. Testing {tool_name}...")
        
        try:
            result = await instance.execute_tool(tool_name, params)
            print(f"   ✅ Success - {len(result.content)} chars")
            
            # Try to parse JSON and show summary
            try:
                data = json.loads(result.content)
                if "functions" in data:
                    print(f"   📋 Functions: {len(data['functions'])}")
                if "structs" in data:
                    print(f"   📦 Structs: {len(data['structs'])}")
                if "imports" in data:
                    print(f"   📥 Imports: {len(data['imports'])}")
                if "callers" in data:
                    print(f"   📞 Callers: {len(data['callers'])}")
            except json.JSONDecodeError:
                pass  # Not JSON, that's fine
                
        except Exception as e:
            print(f"   ⚠️  Failed: {e}")


def create_sample_project():
    """Create a minimal sample project for testing."""
    temp_dir = tempfile.mkdtemp(prefix="loregrep_tools_")
    base_path = Path(temp_dir)
    
    # Simple main file
    (base_path / "main.py").write_text('''
"""Main application."""

from config import ConfigManager

def load_config():
    """Load configuration."""
    return ConfigManager().load_config()

def main():
    """Main entry point."""
    config = load_config()
    print(f"Loaded: {config}")

if __name__ == "__main__":
    main()
''')
    
    # Config manager
    (base_path / "config.py").write_text('''
"""Configuration management."""

import json
from typing import Dict, Any

class ConfigManager:
    """Manages configuration."""
    
    def __init__(self):
        self.data = {}
    
    def load_config(self) -> Dict[str, Any]:
        """Load configuration from file."""
        return {"debug": True, "port": 8080}
    
    def save_config(self, data: Dict[str, Any]) -> bool:
        """Save configuration."""
        self.data = data
        return True

def get_default_config() -> Dict[str, Any]:
    """Get default configuration."""
    return {"debug": False, "port": 3000}
''')
    
    return temp_dir


def cleanup_sample_project(temp_dir: str):
    """Clean up sample project."""
    import shutil
    try:
        shutil.rmtree(temp_dir)
    except OSError:
        pass


if __name__ == "__main__":
    asyncio.run(main())