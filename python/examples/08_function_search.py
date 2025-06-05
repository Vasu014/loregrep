#!/usr/bin/env python3
"""
Function Search Example - Finding Functions by Pattern

Demonstrates how to search for functions using patterns and display results.
Shows the search_functions tool in detail.
"""

import asyncio
import json
import loregrep


async def main():
    print("🔍 Function Search Example")
    print("=" * 30)
    
    try:
        # Create LoreGrep instance
        lg = loregrep.LoreGrep.builder().with_rust_analyzer().with_python_analyzer().build()
        print("✅ LoreGrep instance created")
        
        # Scan current directory first
        print("\n📁 Scanning repository...")
        result = await lg.scan(".")
        print(f"✅ Found {result.functions_found} functions to search")
        
        if result.functions_found == 0:
            print("⚠️  No functions found to search")
            return
        
        # Search for different patterns
        search_patterns = ["main", "test", "config", "new", "get"]
        
        for pattern in search_patterns:
            print(f"\n🔍 Searching for functions matching '{pattern}'...")
            
            try:
                search_result = await lg.execute_tool("search_functions", {
                    "pattern": pattern,
                    "limit": 5
                })
                
                # Parse and display results
                try:
                    data = json.loads(search_result.content)
                    functions = data.get("functions", [])
                    
                    if functions:
                        print(f"   ✅ Found {len(functions)} functions:")
                        for func in functions[:3]:  # Show first 3
                            name = func.get("name", "unknown")
                            file_path = func.get("file_path", "unknown")
                            line = func.get("start_line", "?")
                            print(f"      • {name} ({file_path}:{line})")
                        
                        if len(functions) > 3:
                            print(f"      ... and {len(functions) - 3} more")
                    else:
                        print(f"   ⚪ No functions found matching '{pattern}'")
                        
                except json.JSONDecodeError:
                    print(f"   ✅ Found results (raw): {len(search_result.content)} characters")
                    
            except Exception as e:
                print(f"   ❌ Search failed: {e}")
        
        print(f"\n💡 Use different patterns to find specific types of functions")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())