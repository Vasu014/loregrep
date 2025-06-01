// Tool execution example for LLM integration
// This example demonstrates how to use LoreGrep's tool-based interface for LLM integration

use loregrep::{LoreGrep, ToolSchema, Result as LoreGrepResult};
use serde_json::json;

#[tokio::main]
async fn main() -> LoreGrepResult<()> {
    println!("🤖 LLM Tool Execution Example");
    println!("=============================\n");

    // Initialize LoreGrep with configuration suitable for LLM integration
    let mut loregrep = LoreGrep::builder()
        .with_rust_analyzer()
        .max_files(1000)
        .include_patterns(vec!["**/*.rs".to_string(), "**/*.toml".to_string()])
        .exclude_patterns(vec![
            "**/target/**".to_string(),
            "**/test-repos/**".to_string(),
        ])
        .build()?;

    println!("📋 Step 1: Get tool definitions for LLM");
    
    // Get all available tools for LLM integration
    let tools = LoreGrep::get_tool_definitions();
    println!("   Available tools: {}", tools.len());
    
    for tool in &tools {
        println!("   • {} - {}", tool.name, tool.description);
    }
    
    // Display a sample tool definition in JSON format (as would be sent to LLM)
    if let Some(search_tool) = tools.iter().find(|t| t.name == "search_functions") {
        println!("\n📄 Sample tool definition (JSON for LLM):");
        println!("{}", serde_json::to_string_pretty(search_tool)?);
    }

    println!("\n📁 Step 2: Scan repository for analysis");
    
    let scan_result = loregrep.scan(".").await?;
    println!("   ✅ Scanned {} files, found {} functions", 
        scan_result.files_scanned, scan_result.functions_found);

    println!("\n🔧 Step 3: Execute tools (simulating LLM tool calls)");

    // Tool 1: Search for functions containing "new"
    println!("\n   🔍 Tool: search_functions");
    let search_result = loregrep.execute_tool("search_functions", json!({
        "pattern": "new",
        "limit": 3
    })).await?;
    
    if search_result.success {
        println!("   ✅ Found functions:");
        if let Some(functions) = search_result.data.as_array() {
            for func in functions.iter().take(3) {
                if let Some(name) = func.get("name") {
                    println!("      • {}", name);
                }
            }
        }
    } else {
        println!("   ❌ Search failed: {:?}", search_result.error);
    }

    // Tool 2: Search for structs
    println!("\n   🏗️  Tool: search_structs");
    let struct_result = loregrep.execute_tool("search_structs", json!({
        "pattern": "Config",
        "limit": 3
    })).await?;
    
    if struct_result.success {
        println!("   ✅ Found structs:");
        if let Some(structs) = struct_result.data.as_array() {
            for struct_item in structs.iter().take(3) {
                if let Some(name) = struct_item.get("name") {
                    println!("      • {}", name);
                }
            }
        }
    } else {
        println!("   ❌ Struct search failed: {:?}", struct_result.error);
    }

    // Tool 3: Analyze a specific file
    println!("\n   📄 Tool: analyze_file");
    let analyze_result = loregrep.execute_tool("analyze_file", json!({
        "file_path": "src/main.rs",
        "include_source": false
    })).await?;
    
    if analyze_result.success {
        println!("   ✅ File analysis completed:");
        if let Some(language) = analyze_result.data.get("language") {
            println!("      • Language: {}", language);
        }
        if let Some(functions) = analyze_result.data.get("functions").and_then(|v| v.as_array()) {
            println!("      • Functions: {}", functions.len());
        }
        if let Some(structs) = analyze_result.data.get("structs").and_then(|v| v.as_array()) {
            println!("      • Structs: {}", structs.len());
        }
    } else {
        println!("   ⚠️  File analysis failed (file may not exist): {:?}", analyze_result.error);
    }

    // Tool 4: Get repository tree structure
    println!("\n   🌳 Tool: get_repository_tree");
    let tree_result = loregrep.execute_tool("get_repository_tree", json!({
        "include_file_details": false,
        "max_depth": 2
    })).await?;
    
    if tree_result.success {
        println!("   ✅ Repository tree generated");
        // The tree data would be used by the LLM to understand project structure
    } else {
        println!("   ❌ Tree generation failed: {:?}", tree_result.error);
    }

    // Tool 5: Find function callers
    println!("\n   📞 Tool: find_callers");
    let callers_result = loregrep.execute_tool("find_callers", json!({
        "function_name": "main",
        "limit": 5
    })).await?;
    
    if callers_result.success {
        println!("   ✅ Caller analysis completed");
    } else {
        println!("   ❌ Caller analysis failed: {:?}", callers_result.error);
    }

    // Tool 6: Get dependencies
    println!("\n   🔗 Tool: get_dependencies");
    let deps_result = loregrep.execute_tool("get_dependencies", json!({
        "file_path": "src/lib.rs"
    })).await?;
    
    if deps_result.success {
        println!("   ✅ Dependency analysis completed");
    } else {
        println!("   ⚠️  Dependency analysis failed (file may not exist): {:?}", deps_result.error);
    }

    println!("\n🎯 Integration Summary:");
    println!("==============================");
    println!("1. Get tool definitions with LoreGrep::get_tool_definitions()");
    println!("2. Send tool definitions to your LLM as available tools");
    println!("3. When LLM wants to call a tool, use execute_tool(name, params)");
    println!("4. Return the tool result back to the LLM for processing");
    println!("5. The LLM can chain multiple tool calls to analyze code");

    println!("\n💡 Example LLM Integration Pattern:");
    println!("```rust");
    println!("// In your LLM integration code:");
    println!("let tool_result = loregrep.execute_tool(llm_tool_name, llm_params).await?;");
    println!("if tool_result.success {{");
    println!("    // Send tool_result.data back to LLM");
    println!("}} else {{");
    println!("    // Handle tool error: tool_result.error");
    println!("}}");
    println!("```");

    Ok(())
}