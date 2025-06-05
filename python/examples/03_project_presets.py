#!/usr/bin/env python3
"""
Project Presets Example - Language-Specific Optimized Configurations

Demonstrates project-specific preset methods for common language configurations:
- rust_project() - Optimized for Rust codebases  
- python_project() - Optimized for Python codebases
- polyglot_project() - Optimized for multi-language projects
"""

import asyncio
import loregrep


async def main():
    print("🎯 LoreGrep Project Presets Example")
    print("=" * 42)
    
    # Test the three main preset types
    presets = [
        ("Rust Project", loregrep.LoreGrep.rust_project),
        ("Python Project", loregrep.LoreGrep.python_project), 
        ("Polyglot Project", loregrep.LoreGrep.polyglot_project),
    ]
    
    working_instance = None
    
    for preset_name, preset_func in presets:
        print(f"\n🔧 Testing {preset_name}...")
        
        try:
            # Create instance with preset
            instance = preset_func(".")
            print(f"✅ {preset_name} created successfully!")
            
            # Quick scan to verify it works
            result = await instance.scan(".")
            print(f"   📁 Files: {result.files_scanned}, Functions: {result.functions_found}")
            
            if not working_instance:
                working_instance = instance
                
        except Exception as e:
            print(f"⚠️  {preset_name} failed: {e}")
    
    # Demonstrate tools with working instance
    if working_instance:
        print(f"\n🔍 Testing tools...")
        
        try:
            # Test function search
            search_result = await working_instance.execute_tool("search_functions", {
                "pattern": "main",
                "limit": 3
            })
            print("✅ search_functions working")
            
            # Test repository tree
            tree_result = await working_instance.execute_tool("get_repository_tree", {
                "max_depth": 2
            })
            print("✅ get_repository_tree working")
            
        except Exception as e:
            print(f"⚠️  Tool execution: {e}")
    
    print(f"\n💡 Presets provide optimized configurations for different project types")


if __name__ == "__main__":
    asyncio.run(main())