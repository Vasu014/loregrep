#!/usr/bin/env python3
"""
Dependency Analysis Example - Understanding Import/Export Relationships

Demonstrates the get_dependencies and find_callers tools for analyzing
how code modules interact and depend on each other.
"""

import asyncio
import json
from pathlib import Path
import loregrep


async def main():
    print("🔗 Dependency Analysis Example")
    print("=" * 35)
    
    try:
        # Create LoreGrep instance
        lg = loregrep.LoreGrep.polyglot_project(".")
        print("✅ LoreGrep instance created")
        
        # Scan repository
        print("\n📁 Scanning repository...")
        scan_result = await lg.scan(".")
        print(f"✅ Scanned {scan_result.files_scanned} files")
        
        # Find source files to analyze
        source_files = []
        for pattern in ["*.py", "*.rs", "*.js", "*.ts"]:
            source_files.extend(Path(".").glob(f"**/{pattern}"))
        
        if not source_files:
            print("⚠️  No source files found")
            return
        
        # Analyze dependencies for a few files
        files_to_analyze = source_files[:3]
        
        for file_path in files_to_analyze:
            print(f"\n📄 Analyzing dependencies for {file_path.name}...")
            
            try:
                dep_result = await lg.execute_tool("get_dependencies", {
                    "file_path": str(file_path)
                })
                
                # Parse dependency results
                try:
                    data = json.loads(dep_result.content)
                    
                    imports = data.get("imports", [])
                    exports = data.get("exports", [])
                    
                    if imports:
                        print(f"   📥 Imports ({len(imports)}):")
                        for imp in imports[:5]:  # Show first 5
                            module = imp.get("module", "unknown")
                            import_type = imp.get("type", "unknown")
                            print(f"      • {module} ({import_type})")
                        if len(imports) > 5:
                            print(f"      ... and {len(imports) - 5} more")
                    
                    if exports:
                        print(f"   📤 Exports ({len(exports)}):")
                        for exp in exports[:5]:  # Show first 5
                            name = exp.get("name", "unknown")
                            export_type = exp.get("type", "unknown")
                            print(f"      • {name} ({export_type})")
                        if len(exports) > 5:
                            print(f"      ... and {len(exports) - 5} more")
                    
                    if not imports and not exports:
                        print(f"   ⚪ No dependencies found")
                        
                except json.JSONDecodeError:
                    print(f"   ✅ Dependencies analyzed: {len(dep_result.content)} characters")
                    
            except Exception as e:
                print(f"   ❌ Dependency analysis failed: {e}")
        
        # Demonstrate caller analysis
        print(f"\n📞 Finding function callers...")
        common_functions = ["main", "new", "init", "create", "load"]
        
        for func_name in common_functions:
            try:
                caller_result = await lg.execute_tool("find_callers", {
                    "function_name": func_name
                })
                
                try:
                    data = json.loads(caller_result.content)
                    callers = data.get("callers", [])
                    
                    if callers:
                        print(f"   📞 '{func_name}' called from {len(callers)} locations:")
                        for caller in callers[:3]:  # Show first 3
                            file_path = caller.get("file_path", "unknown")
                            line = caller.get("line_number", "?")
                            print(f"      • {Path(file_path).name}:{line}")
                        if len(callers) > 3:
                            print(f"      ... and {len(callers) - 3} more locations")
                        break  # Found some callers, stop here
                        
                except json.JSONDecodeError:
                    pass
                    
            except Exception:
                pass  # Try next function
        
        print(f"\n💡 Dependency analysis helps understand code relationships and architecture")
        
    except ImportError:
        print("❌ LoreGrep not installed. Run: maturin develop --features python")
    except Exception as e:
        print(f"❌ Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())