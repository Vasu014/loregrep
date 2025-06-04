#!/usr/bin/env rust
//! CLI Integration Example
//! 
//! This example demonstrates how to integrate loregrep into command-line tools
//! and scripts. It shows patterns for:
//! 
//! - Command-line argument handling
//! - Progress reporting during scanning
//! - Different output formats (JSON, table, tree)
//! - Error handling and user feedback
//! - Configuration file loading

use loregrep::{LoreGrep, ScanResult};
use serde_json::json;
use std::env;
use std::path::Path;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: {} <repository_path> [--format json|table|tree]", args[0]);
        println!();
        println!("Example CLI integration showing how to:");
        println!("  • Scan repositories with progress feedback");
        println!("  • Format output in different ways");
        println!("  • Handle errors gracefully");
        return Ok(());
    }
    
    let repo_path = &args[1];
    let format = args.get(2).and_then(|f| if f == "--format" { args.get(3) } else { None })
                    .unwrap_or(&"table".to_string());
    
    if !Path::new(repo_path).exists() {
        eprintln!("❌ Error: Repository path '{}' does not exist", repo_path);
        std::process::exit(1);
    }
    
    println!("🔍 Loregrep CLI Integration Example");
    println!("📁 Repository: {}", repo_path);
    println!("📋 Output format: {}", format);
    println!();
    
    // Create loregrep instance with CLI-appropriate configuration
    let loregrep = create_cli_loregrep()?;
    
    // Scan with progress reporting
    let scan_result = scan_with_progress(&loregrep, repo_path).await?;
    
    // Display results in requested format
    display_results(&scan_result, format, &loregrep, repo_path).await?;
    
    Ok(())
}

fn create_cli_loregrep() -> Result<LoreGrep, Box<dyn std::error::Error>> {
    println!("⚙️  Configuring loregrep for CLI usage...");
    
    let loregrep = LoreGrep::builder()
        .max_file_size(2 * 1024 * 1024)  // 2MB max for CLI
        .max_depth(12)                    // Reasonable depth for CLI
        .file_patterns(vec![
            "*.rs".to_string(),
            "*.py".to_string(),
            "*.js".to_string(),
            "*.ts".to_string(),
            "*.go".to_string(),
            "*.java".to_string(),
            "*.cpp".to_string(),
            "*.c".to_string(),
            "*.h".to_string(),
        ])
        .exclude_patterns(vec![
            "target/".to_string(),        // Rust build artifacts
            "node_modules/".to_string(),  // Node.js dependencies
            ".git/".to_string(),          // Git metadata
            "__pycache__/".to_string(),   // Python cache
            "*.min.js".to_string(),       // Minified JS
            "vendor/".to_string(),        // Vendored dependencies
            ".vscode/".to_string(),       // VS Code settings
            ".idea/".to_string(),         // IntelliJ settings
        ])
        .respect_gitignore(true)
        .build()?;
    
    println!("✅ Loregrep configured successfully");
    Ok(loregrep)
}

async fn scan_with_progress(loregrep: &LoreGrep, repo_path: &str) -> Result<ScanResult, Box<dyn std::error::Error>> {
    println!("🚀 Starting repository scan...");
    let start_time = Instant::now();
    
    // In a real CLI, you might want to show a progress bar here
    // For now, we'll just show the start and completion
    
    let result = loregrep.scan(repo_path).await?;
    let duration = start_time.elapsed();
    
    println!("✅ Scan completed in {:.2}s", duration.as_secs_f64());
    println!();
    
    Ok(result)
}

async fn display_results(
    scan_result: &ScanResult, 
    format: &str, 
    loregrep: &LoreGrep,
    repo_path: &str
) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        "json" => display_json_results(scan_result, loregrep).await?,
        "tree" => display_tree_results(loregrep).await?,
        "table" | _ => display_table_results(scan_result, loregrep).await?,
    }
    Ok(())
}

async fn display_table_results(
    scan_result: &ScanResult,
    loregrep: &LoreGrep
) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Scan Results (Table Format)");
    println!("═══════════════════════════════");
    
    // Summary statistics
    println!("📁 Files scanned:     {}", scan_result.files_scanned);
    println!("🔧 Functions found:   {}", scan_result.functions_found);
    println!("📦 Structs found:     {}", scan_result.structs_found);
    println!("⏱️  Scan duration:     {}ms", scan_result.duration_ms);
    
    if !scan_result.errors.is_empty() {
        println!("⚠️  Errors encountered: {}", scan_result.errors.len());
        for (i, error) in scan_result.errors.iter().enumerate().take(3) {
            println!("   {}. {}", i + 1, error);
        }
        if scan_result.errors.len() > 3 {
            println!("   ... and {} more", scan_result.errors.len() - 3);
        }
    }
    
    println!();
    
    // Show a sample of functions
    println!("🔍 Sample Functions:");
    let functions = loregrep.execute_tool("search_functions", json!({
        "pattern": "",
        "limit": 10
    })).await?;
    
    if !functions.content.is_empty() {
        println!("{}", functions.content);
    } else {
        println!("   No functions found");
    }
    
    Ok(())
}

async fn display_json_results(
    scan_result: &ScanResult,
    loregrep: &LoreGrep
) -> Result<(), Box<dyn std::error::Error>> {
    // Get some sample data
    let functions = loregrep.execute_tool("search_functions", json!({
        "pattern": "",
        "limit": 5
    })).await?;
    
    let structs = loregrep.execute_tool("search_structs", json!({
        "pattern": "",
        "limit": 5
    })).await?;
    
    let result = json!({
        "scan_summary": {
            "files_scanned": scan_result.files_scanned,
            "functions_found": scan_result.functions_found,
            "structs_found": scan_result.structs_found,
            "duration_ms": scan_result.duration_ms,
            "errors": scan_result.errors
        },
        "sample_functions": functions.content,
        "sample_structs": structs.content
    });
    
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn display_tree_results(loregrep: &LoreGrep) -> Result<(), Box<dyn std::error::Error>> {
    println!("🌳 Repository Tree");
    println!("═══════════════════");
    
    let tree = loregrep.execute_tool("get_repository_tree", json!({
        "include_file_details": true,
        "max_depth": 3
    })).await?;
    
    println!("{}", tree.content);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    
    #[tokio::test]
    async fn test_cli_integration() {
        // Create a temporary directory with some test files
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        
        // Create a simple Rust file
        let rust_file = temp_path.join("test.rs");
        fs::write(&rust_file, r#"
fn hello_world() {
    println!("Hello, world!");
}

struct TestStruct {
    field: i32,
}
"#).unwrap();
        
        // Test scanning
        let loregrep = create_cli_loregrep().unwrap();
        let result = scan_with_progress(&loregrep, temp_path.to_str().unwrap()).await.unwrap();
        
        assert!(result.files_scanned > 0);
        assert!(result.functions_found > 0);
    }
} 