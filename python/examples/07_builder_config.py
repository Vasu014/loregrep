#!/usr/bin/env python3
"""
Builder Configuration Example - Advanced Builder Pattern Usage

Demonstrates the builder pattern with various configuration options:
- File filtering and patterns
- Size and depth limits  
- Analyzer selection
- Performance optimizations
"""

import asyncio
import loregrep


async def main():
    print("🔧 Builder Configuration Example")
    print("=" * 37)
    
    try:
        # Create LoreGrep instance with detailed configuration
        print("Building LoreGrep instance with custom configuration...")
        
        lg = (loregrep.LoreGrep.builder()
              .with_rust_analyzer()                    # Add Rust support
              .with_python_analyzer()                  # Add Python support
              .max_file_size(5 * 1024 * 1024)         # 5MB file limit
              .max_depth(8)                           # Max directory depth
              .file_patterns(["*.rs", "*.py", "*.js", "*.ts"])  # File types
              .exclude_patterns(["target/", "__pycache__/", "node_modules/"])  # Skip dirs
              .respect_gitignore(True)                # Honor .gitignore
              .build())
        
        print("✅ LoreGrep instance configured successfully!")
        
        # Show configuration by scanning
        print("\n🔍 Testing configuration with scan...")
        result = await lg.scan(".")
        
        print("✅ Configuration test complete!")
        print(f"   📁 Files: {result.files_scanned}")
        print(f"   🔧 Functions: {result.functions_found}")  
        print(f"   📦 Structs: {result.structs_found}")
        print(f"   ⏱️  Time: {result.duration_ms}ms")
        
        # Test tool availability
        tools = loregrep.LoreGrep.get_tool_definitions()
        print(f"\n🛠️  Available tools: {len(tools)}")
        for tool in tools[:3]:  # Show first 3 tools
            print(f"   • {tool.name}: {tool.description[:50]}...")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Configuration error: {e}")


if __name__ == "__main__":
    asyncio.run(main())