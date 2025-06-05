#!/usr/bin/env python3
"""
Auto-Discovery Example - Zero Configuration Setup

This example demonstrates the simplest way to use LoreGrep:
automatic project type detection and analyzer configuration.

Perfect for getting started quickly!
"""

import asyncio
import loregrep


async def main():
    print("🔍 LoreGrep Auto-Discovery Example")
    print("=" * 40)
    
    try:
        # Zero-configuration setup - LoreGrep detects your project type automatically
        print("Creating LoreGrep instance with auto-discovery...")
        loregrep_instance = loregrep.LoreGrep.auto_discover(".")
        
        print("✅ Auto-discovery successful!")
        print("   🔍 Project languages detected and analyzers configured")
        print("   ✅ Ready to scan and analyze code")
        
        # Quick scan of current directory
        print("\n📁 Scanning current directory...")
        result = await loregrep_instance.scan(".")
        
        print(f"✅ Scan completed!")
        print(f"   📁 Files scanned: {result.files_scanned}")
        print(f"   🔧 Functions found: {result.functions_found}")
        print(f"   📦 Structs found: {result.structs_found}")
        print(f"   ⏱️  Duration: {result.duration_ms}ms")
        
        # Quick function search
        if result.functions_found > 0:
            print("\n🔍 Searching for functions containing 'main'...")
            search_result = await loregrep_instance.execute_tool("search_functions", {
                "pattern": "main",
                "limit": 5
            })
            print("✅ Search completed!")
            print(f"   Found functions: {len(search_result.content)} characters of data")
        
        print("\n🎉 Auto-discovery example completed successfully!")
        print("💡 This is the easiest way to get started with LoreGrep")
        
    except Exception as e:
        print(f"❌ Auto-discovery failed: {e}")
        print("💡 This might happen if:")
        print("   - No supported languages detected in current directory")
        print("   - LoreGrep bindings not installed properly")
        print("   - Current directory doesn't contain code files")


if __name__ == "__main__":
    asyncio.run(main())