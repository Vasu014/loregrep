#!/usr/bin/env python3
"""
Simple Scan Example - Basic Repository Scanning

Shows the minimal code needed to scan a repository and get results.
Perfect for getting started quickly!
"""

import asyncio
import loregrep


async def main():
    print("📁 Simple Repository Scan")
    print("=" * 28)
    
    try:
        # Create LoreGrep instance with minimal configuration
        lg = loregrep.LoreGrep.builder().build()
        print("✅ LoreGrep instance created")
        
        # Scan current directory
        print("\n🔍 Scanning current directory...")
        result = await lg.scan(".")
        
        # Display results
        print("✅ Scan complete!")
        print(f"   📁 Files scanned: {result.files_scanned}")
        print(f"   🔧 Functions found: {result.functions_found}")
        print(f"   📦 Structs found: {result.structs_found}")
        print(f"   ⏱️  Duration: {result.duration_ms}ms")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())