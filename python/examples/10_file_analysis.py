#!/usr/bin/env python3
"""
File Analysis Example - Detailed Analysis of Individual Files

Demonstrates the analyze_file tool for getting detailed information about
specific files including functions, structs, imports, and more.
"""

import asyncio
import json
from pathlib import Path
import loregrep


async def main():
    print("📄 File Analysis Example")
    print("=" * 28)
    
    try:
        # Create LoreGrep instance  
        lg = loregrep.LoreGrep.auto_discover(".")
        print("✅ LoreGrep instance created with auto-discovery")
        
        # Scan repository to find files
        print("\n📁 Scanning repository...")
        scan_result = await lg.scan(".")
        print(f"✅ Scanned {scan_result.files_scanned} files")
        
        # Find some source files to analyze
        source_files = []
        for pattern in ["*.py", "*.rs", "*.js", "*.ts"]:
            source_files.extend(Path(".").glob(f"**/{pattern}"))
        
        if not source_files:
            print("⚠️  No source files found to analyze")
            return
        
        # Analyze a few interesting files
        files_to_analyze = source_files[:3]  # First 3 files
        
        for file_path in files_to_analyze:
            print(f"\n📄 Analyzing {file_path}...")
            
            try:
                result = await lg.execute_tool("analyze_file", {
                    "file_path": str(file_path),
                    "include_source": False  # Don't include source code in output
                })
                
                # Parse and display analysis results
                try:
                    data = json.loads(result.content)
                    
                    print(f"   ✅ Analysis complete:")
                    
                    # Show functions
                    functions = data.get("functions", [])
                    if functions:
                        print(f"      🔧 Functions ({len(functions)}):")
                        for func in functions[:3]:  # Show first 3
                            name = func.get("name", "unknown")
                            line = func.get("start_line", "?")
                            print(f"         • {name} (line {line})")
                        if len(functions) > 3:
                            print(f"         ... and {len(functions) - 3} more")
                    
                    # Show structs/classes
                    structs = data.get("structs", [])
                    if structs:
                        print(f"      📦 Structs/Classes ({len(structs)}):")
                        for struct in structs[:3]:  # Show first 3
                            name = struct.get("name", "unknown")
                            line = struct.get("start_line", "?")
                            print(f"         • {name} (line {line})")
                        if len(structs) > 3:
                            print(f"         ... and {len(structs) - 3} more")
                    
                    # Show imports
                    imports = data.get("imports", [])
                    if imports:
                        print(f"      📥 Imports ({len(imports)}):")
                        for imp in imports[:3]:  # Show first 3
                            module = imp.get("module", "unknown")
                            print(f"         • {module}")
                        if len(imports) > 3:
                            print(f"         ... and {len(imports) - 3} more")
                    
                    if not functions and not structs and not imports:
                        print(f"      ⚪ No code elements found")
                        
                except json.JSONDecodeError:
                    print(f"   ✅ Analysis complete: {len(result.content)} characters")
                    
            except Exception as e:
                print(f"   ❌ Analysis failed: {e}")
        
        print(f"\n💡 analyze_file provides detailed insights into individual files")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())