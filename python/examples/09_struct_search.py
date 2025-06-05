#!/usr/bin/env python3
"""
Struct Search Example - Finding Classes and Structures

Demonstrates the search_structs tool for finding classes, structs,
interfaces, and other type definitions by pattern matching.
"""

import asyncio
import json
import loregrep


async def main():
    print("📦 Struct Search Example")
    print("=" * 28)
    
    try:
        # Create LoreGrep instance with multiple analyzers
        lg = (loregrep.LoreGrep.builder()
              .with_rust_analyzer()
              .with_python_analyzer()
              .build())
        print("✅ LoreGrep instance created")
        
        # Scan repository
        print("\n📁 Scanning repository...")
        scan_result = await lg.scan(".")
        print(f"✅ Found {scan_result.structs_found} structs/classes to search")
        
        if scan_result.structs_found == 0:
            print("⚠️  No structs/classes found to search")
            return
        
        # Search for different struct patterns
        search_patterns = [
            "Config",     # Configuration classes
            "Manager",    # Manager classes  
            "Builder",    # Builder pattern classes
            "Error",      # Error types
            "Result",     # Result types
            "Client",     # Client classes
        ]
        
        found_any = False
        
        for pattern in search_patterns:
            print(f"\n🔍 Searching for structs/classes matching '{pattern}'...")
            
            try:
                search_result = await lg.execute_tool("search_structs", {
                    "pattern": pattern,
                    "limit": 8
                })
                
                # Parse and display results
                try:
                    data = json.loads(search_result.content)
                    structs = data.get("structs", [])
                    
                    if structs:
                        found_any = True
                        print(f"   ✅ Found {len(structs)} structs/classes:")
                        
                        for struct in structs:
                            name = struct.get("name", "unknown")
                            file_path = struct.get("file_path", "unknown")
                            line = struct.get("start_line", "?")
                            struct_type = struct.get("type", "struct")
                            
                            # Get filename only for cleaner display
                            filename = file_path.split("/")[-1] if "/" in file_path else file_path
                            
                            print(f"      • {name} ({struct_type}) - {filename}:{line}")
                        
                        # Show fields if available
                        if structs and "fields" in structs[0]:
                            first_struct = structs[0]
                            fields = first_struct.get("fields", [])
                            if fields:
                                print(f"        Fields in {first_struct['name']}:")
                                for field in fields[:3]:  # Show first 3 fields
                                    field_name = field.get("name", "unknown")
                                    field_type = field.get("type", "unknown")
                                    print(f"          - {field_name}: {field_type}")
                                if len(fields) > 3:
                                    print(f"          ... and {len(fields) - 3} more fields")
                    else:
                        print(f"   ⚪ No structs/classes found matching '{pattern}'")
                        
                except json.JSONDecodeError:
                    print(f"   ✅ Found results (raw): {len(search_result.content)} characters")
                    found_any = True
                    
            except Exception as e:
                print(f"   ❌ Search failed: {e}")
        
        if found_any:
            print(f"\n💡 Use specific patterns to find classes with particular naming conventions")
        else:
            print(f"\n💡 Try scanning a repository with more classes/structs")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())