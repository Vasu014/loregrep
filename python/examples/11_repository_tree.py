#!/usr/bin/env python3
"""
Repository Tree Example - Getting Project Structure Overview

Demonstrates the get_repository_tree tool for understanding
project structure, file organization, and repository statistics.
"""

import asyncio
import json
import loregrep


async def main():
    print("🌳 Repository Tree Example")
    print("=" * 30)
    
    try:
        # Create LoreGrep instance
        lg = loregrep.LoreGrep.builder().with_rust_analyzer().with_python_analyzer().build()
        print("✅ LoreGrep instance created")
        
        # Scan repository first
        print("\n📁 Scanning repository...")
        scan_result = await lg.scan(".")
        print(f"✅ Scanned {scan_result.files_scanned} files")
        
        # Get repository tree with different detail levels
        print("\n🌳 Getting repository structure...")
        
        try:
            # Basic tree (no file details)
            print("\n1. Basic tree structure:")
            basic_result = await lg.execute_tool("get_repository_tree", {
                "max_depth": 3,
                "include_file_details": False
            })
            
            print(f"   ✅ Basic structure: {len(basic_result.content)} characters")
            
            # Detailed tree (with file details)
            print("\n2. Detailed tree with file information:")
            detailed_result = await lg.execute_tool("get_repository_tree", {
                "max_depth": 2,
                "include_file_details": True
            })
            
            print(f"   ✅ Detailed structure: {len(detailed_result.content)} characters")
            
            # Parse and show statistics
            try:
                data = json.loads(detailed_result.content)
                
                if "stats" in data:
                    stats = data["stats"]
                    print(f"\n📊 Repository Statistics:")
                    print(f"   📁 Total files: {stats.get('total_files', '?')}")
                    print(f"   🔧 Total functions: {stats.get('total_functions', '?')}")
                    print(f"   📦 Total structs: {stats.get('total_structs', '?')}")
                    print(f"   📏 Total lines: {stats.get('total_lines', '?')}")
                
                if "structure" in data:
                    print(f"\n🏗️  Structure data available for further processing")
                    
            except json.JSONDecodeError:
                print(f"   📄 Raw tree data available")
            
            # Shallow tree for quick overview
            print("\n3. Shallow overview (depth 1):")
            shallow_result = await lg.execute_tool("get_repository_tree", {
                "max_depth": 1,
                "include_file_details": False
            })
            
            print(f"   ✅ Shallow overview: {len(shallow_result.content)} characters")
            
        except Exception as e:
            print(f"❌ Tree generation failed: {e}")
        
        print(f"\n💡 Repository tree helps understand project structure and organization")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())