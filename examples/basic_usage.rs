// Basic usage example for the LoreGrep public API
// This example demonstrates how to use LoreGrep as a library for code analysis

use loregrep::{LoreGrep, Result as LoreGrepResult};
use serde_json::json;

#[tokio::main]
async fn main() -> LoreGrepResult<()> {
    // Create a new LoreGrep instance with default configuration
    let mut loregrep = LoreGrep::builder()
        .with_rust_analyzer()                          // Enable Rust analysis
        .max_files(1000)                              // Limit files to scan
        .include_patterns(vec!["**/*.rs".to_string()]) // Only Rust files
        .exclude_patterns(vec!["**/target/**".to_string()]) // Skip build artifacts
        .build()?;

    println!("🔍 LoreGrep Public API Example");
    println!("================================\n");

    // Example 1: Get available tools
    println!("📋 Available Analysis Tools:");
    let tools = LoreGrep::get_tool_definitions();
    for tool in &tools {
        println!("  • {} - {}", tool.name, tool.description);
    }
    println!();

    // Example 2: Scan a repository (using current directory)
    println!("📁 Scanning repository...");
    let scan_result = loregrep.scan(".").await?;
    println!("✅ Scan completed:");
    println!("   Files scanned: {}", scan_result.files_scanned);
    println!("   Functions found: {}", scan_result.functions_found);
    println!("   Structs found: {}", scan_result.structs_found);
    println!("   Duration: {}ms", scan_result.duration_ms);
    println!();

    // Example 3: Search for functions
    println!("🔎 Searching for functions containing 'new'...");
    let search_result = loregrep.execute_tool("search_functions", json!({
        "pattern": "new",
        "limit": 5
    })).await?;

    if search_result.success {
        if let Some(functions) = search_result.data.as_array() {
            println!("   Found {} functions:", functions.len());
            for func in functions.iter().take(3) {
                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                    let file = func.get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    println!("   • {} (in {})", name, file);
                }
            }
        }
    } else {
        println!("   ❌ Search failed: {:?}", search_result.error);
    }
    println!();

    // Example 4: Analyze a specific file
    println!("📄 Analyzing main.rs...");
    let analyze_result = loregrep.execute_tool("analyze_file", json!({
        "file_path": "src/main.rs",
        "include_source": false
    })).await?;

    if analyze_result.success {
        println!("   ✅ Analysis successful");
        if let Some(language) = analyze_result.data.get("language") {
            println!("   Language: {}", language);
        }
        if let Some(functions) = analyze_result.data.get("functions").and_then(|v| v.as_array()) {
            println!("   Functions: {}", functions.len());
        }
        if let Some(structs) = analyze_result.data.get("structs").and_then(|v| v.as_array()) {
            println!("   Structs: {}", structs.len());
        }
    } else {
        println!("   ❌ Analysis failed: {:?}", analyze_result.error);
    }
    println!();

    // Example 5: Get repository structure
    println!("🌳 Getting repository tree...");
    let tree_result = loregrep.execute_tool("get_repository_tree", json!({
        "include_file_details": false,
        "max_depth": 2
    })).await?;

    if tree_result.success {
        println!("   ✅ Repository tree generated");
        // The tree data would be in tree_result.data
    } else {
        println!("   ❌ Tree generation failed: {:?}", tree_result.error);
    }

    println!("\n🎉 Example completed successfully!");
    println!("\nThis demonstrates the basic usage of LoreGrep as a library.");
    println!("You can integrate these patterns into your own applications,");
    println!("coding assistants, or development tools.");

    Ok(())
}