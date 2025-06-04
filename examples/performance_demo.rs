#!/usr/bin/env rust
//! Performance Demo Example
//! 
//! This example demonstrates loregrep's performance characteristics by:
//! 
//! - Benchmarking scanning speed on different repository sizes
//! - Measuring memory usage during analysis
//! - Comparing different configuration settings
//! - Testing query performance after indexing
//! - Showing optimization techniques for large codebases

use loregrep::{LoreGrep, ScanResult};
use serde_json::json;
use std::env;
use std::path::Path;
use std::time::{Duration, Instant};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: {} <repository_path> [--mode small|medium|large|optimized]", args[0]);
        println!();
        println!("Performance demo showing:");
        println!("  • Scanning speed benchmarks");
        println!("  • Memory usage analysis");
        println!("  • Query performance tests");
        println!("  • Configuration optimization tips");
        return Ok(());
    }
    
    let repo_path = &args[1];
    let mode = args.get(2).and_then(|m| if m == "--mode" { args.get(3) } else { None })
                  .unwrap_or(&"medium".to_string());
    
    if !Path::new(repo_path).exists() {
        eprintln!("❌ Error: Repository path '{}' does not exist", repo_path);
        std::process::exit(1);
    }
    
    println!("⚡ Loregrep Performance Demo");
    println!("📁 Repository: {}", repo_path);
    println!("⚙️  Mode: {}", mode);
    println!("═══════════════════════════════════════");
    println!();
    
    // Run performance tests based on mode
    match mode.as_str() {
        "small" => run_small_repo_demo(repo_path).await?,
        "large" => run_large_repo_demo(repo_path).await?,
        "optimized" => run_optimized_demo(repo_path).await?,
        "medium" | _ => run_medium_repo_demo(repo_path).await?,
    }
    
    Ok(())
}

async fn run_small_repo_demo(repo_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🏃‍♂️ Small Repository Performance Test");
    println!("Target: <100 files, optimized for speed");
    println!();
    
    let config = LoreGrep::builder()
        .max_file_size(1024 * 1024)  // 1MB max
        .max_depth(8)                 // Shallow for speed
        .file_patterns(vec!["*.rs".to_string(), "*.py".to_string()])
        .respect_gitignore(true)
        .build()?;
    
    let benchmark = run_single_benchmark("Small Repo Config", &config, repo_path).await?;
    print_benchmark_results(&benchmark);
    
    // Test query performance
    println!("\n📊 Query Performance Tests:");
    test_query_performance(&config).await?;
    
    Ok(())
}

async fn run_medium_repo_demo(repo_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🏃‍♂️ Medium Repository Performance Test");
    println!("Target: 100-1000 files, balanced configuration");
    println!();
    
    let config = LoreGrep::builder()
        .max_file_size(2 * 1024 * 1024)  // 2MB max
        .max_depth(12)                    // Reasonable depth
        .file_patterns(vec![
            "*.rs".to_string(),
            "*.py".to_string(),
            "*.js".to_string(),
            "*.ts".to_string(),
        ])
        .exclude_patterns(vec![
            "target/".to_string(),
            "node_modules/".to_string(),
            ".git/".to_string(),
        ])
        .respect_gitignore(true)
        .build()?;
    
    let benchmark = run_single_benchmark("Medium Repo Config", &config, repo_path).await?;
    print_benchmark_results(&benchmark);
    
    // Test query performance
    println!("\n📊 Query Performance Tests:");
    test_query_performance(&config).await?;
    
    Ok(())
}

async fn run_large_repo_demo(repo_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🏃‍♂️ Large Repository Performance Test");
    println!("Target: 1000+ files, thorough analysis");
    println!();
    
    let config = LoreGrep::builder()
        .max_file_size(5 * 1024 * 1024)  // 5MB max
        .max_depth(15)                    // Deep analysis
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
            "target/".to_string(),
            "node_modules/".to_string(),
            ".git/".to_string(),
            "__pycache__/".to_string(),
            "vendor/".to_string(),
            "*.generated.*".to_string(),
        ])
        .respect_gitignore(true)
        .build()?;
    
    let benchmark = run_single_benchmark("Large Repo Config", &config, repo_path).await?;
    print_benchmark_results(&benchmark);
    
    // Test query performance
    println!("\n📊 Query Performance Tests:");
    test_query_performance(&config).await?;
    
    Ok(())
}

async fn run_optimized_demo(repo_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Optimized Configuration Comparison");
    println!("Comparing different optimization strategies");
    println!();
    
    let configs = vec![
        ("Standard", create_standard_config()?),
        ("Memory Optimized", create_memory_optimized_config()?),
        ("Speed Optimized", create_speed_optimized_config()?),
        ("Minimal", create_minimal_config()?),
    ];
    
    let mut results = Vec::new();
    
    for (name, config) in configs {
        println!("Testing: {}", name);
        let benchmark = run_single_benchmark(name, &config, repo_path).await?;
        results.push((name, benchmark));
        println!();
    }
    
    // Compare results
    println!("📈 Performance Comparison:");
    print_comparison_table(&results);
    
    Ok(())
}

#[derive(Debug)]
struct BenchmarkResult {
    name: String,
    scan_duration: Duration,
    scan_result: ScanResult,
    query_times: HashMap<String, Duration>,
}

async fn run_single_benchmark(
    name: &str, 
    config: &LoreGrep, 
    repo_path: &str
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    println!("⏱️  Running benchmark: {}", name);
    
    // Measure scanning time
    let scan_start = Instant::now();
    let scan_result = config.scan(repo_path).await?;
    let scan_duration = scan_start.elapsed();
    
    println!("   📁 Files scanned: {}", scan_result.files_scanned);
    println!("   🔧 Functions found: {}", scan_result.functions_found);
    println!("   📦 Structs found: {}", scan_result.structs_found);
    println!("   ⏱️  Scan time: {:.2}s", scan_duration.as_secs_f64());
    
    // Measure query times
    let mut query_times = HashMap::new();
    
    // Test function search
    let query_start = Instant::now();
    let _functions = config.execute_tool("search_functions", json!({
        "pattern": "",
        "limit": 10
    })).await?;
    query_times.insert("search_functions".to_string(), query_start.elapsed());
    
    // Test struct search
    let query_start = Instant::now();
    let _structs = config.execute_tool("search_structs", json!({
        "pattern": "",
        "limit": 10
    })).await?;
    query_times.insert("search_structs".to_string(), query_start.elapsed());
    
    // Test tree generation
    let query_start = Instant::now();
    let _tree = config.execute_tool("get_repository_tree", json!({
        "include_file_details": false,
        "max_depth": 2
    })).await?;
    query_times.insert("get_repository_tree".to_string(), query_start.elapsed());
    
    Ok(BenchmarkResult {
        name: name.to_string(),
        scan_duration,
        scan_result,
        query_times,
    })
}

fn print_benchmark_results(benchmark: &BenchmarkResult) {
    println!("📊 Results for {}:", benchmark.name);
    println!("   Scanning: {:.3}s", benchmark.scan_duration.as_secs_f64());
    
    for (query_name, duration) in &benchmark.query_times {
        println!("   {}: {:.3}ms", query_name, duration.as_secs_f64() * 1000.0);
    }
    
    if !benchmark.scan_result.errors.is_empty() {
        println!("   ⚠️  {} errors encountered", benchmark.scan_result.errors.len());
    }
}

fn print_comparison_table(results: &[(String, BenchmarkResult)]) {
    println!();
    println!("┌─────────────────┬──────────────┬─────────────┬──────────────┐");
    println!("│ Configuration   │ Scan Time(s) │ Files       │ Functions    │");
    println!("├─────────────────┼──────────────┼─────────────┼──────────────┤");
    
    for (name, result) in results {
        println!("│ {:15} │ {:12.2} │ {:11} │ {:12} │", 
                name, 
                result.scan_duration.as_secs_f64(),
                result.scan_result.files_scanned,
                result.scan_result.functions_found);
    }
    
    println!("└─────────────────┴──────────────┴─────────────┴──────────────┘");
    
    // Find the fastest
    if let Some((fastest_name, fastest_result)) = results.iter()
        .min_by(|a, b| a.1.scan_duration.cmp(&b.1.scan_duration)) {
        println!("\n🏆 Fastest configuration: {} ({:.2}s)", 
                fastest_name, fastest_result.scan_duration.as_secs_f64());
    }
}

async fn test_query_performance(config: &LoreGrep) -> Result<(), Box<dyn std::error::Error>> {
    let queries = vec![
        ("Empty search", json!({"pattern": "", "limit": 20})),
        ("Common pattern", json!({"pattern": "main", "limit": 10})),
        ("Regex pattern", json!({"pattern": "pub.*fn", "limit": 15})),
        ("No matches", json!({"pattern": "nonexistent_function_name_xyz", "limit": 10})),
    ];
    
    for (query_name, params) in queries {
        let start = Instant::now();
        let result = config.execute_tool("search_functions", params).await?;
        let duration = start.elapsed();
        
        // Count actual results (simple heuristic)
        let result_count = result.content.lines().count().saturating_sub(2); // Subtract header lines
        
        println!("   {}: {:.2}ms ({} results)", 
                query_name, 
                duration.as_secs_f64() * 1000.0,
                result_count);
    }
    
    Ok(())
}

// Configuration builders for different optimization strategies

fn create_standard_config() -> Result<LoreGrep, Box<dyn std::error::Error>> {
    Ok(LoreGrep::builder()
        .max_file_size(2 * 1024 * 1024)  // 2MB
        .max_depth(12)
        .file_patterns(vec!["*.rs".to_string(), "*.py".to_string(), "*.js".to_string()])
        .exclude_patterns(vec!["target/".to_string(), ".git/".to_string()])
        .respect_gitignore(true)
        .build()?)
}

fn create_memory_optimized_config() -> Result<LoreGrep, Box<dyn std::error::Error>> {
    Ok(LoreGrep::builder()
        .max_file_size(512 * 1024)       // 512KB - smaller files only
        .max_depth(8)                    // Shallower depth
        .file_patterns(vec!["*.rs".to_string()])  // Only Rust files
        .exclude_patterns(vec![
            "target/".to_string(),
            ".git/".to_string(),
            "vendor/".to_string(),
            "*.generated.*".to_string(),
        ])
        .respect_gitignore(true)
        .build()?)
}

fn create_speed_optimized_config() -> Result<LoreGrep, Box<dyn std::error::Error>> {
    Ok(LoreGrep::builder()
        .max_file_size(1024 * 1024)      // 1MB
        .max_depth(6)                    // Very shallow
        .file_patterns(vec!["*.rs".to_string(), "*.py".to_string()])
        .exclude_patterns(vec![
            "target/".to_string(),
            ".git/".to_string(),
            "node_modules/".to_string(),
            "__pycache__/".to_string(),
            "vendor/".to_string(),
            "*.lock".to_string(),
            "*.log".to_string(),
            "*.tmp".to_string(),
        ])
        .respect_gitignore(true)
        .build()?)
}

fn create_minimal_config() -> Result<LoreGrep, Box<dyn std::error::Error>> {
    Ok(LoreGrep::builder()
        .max_file_size(256 * 1024)       // 256KB - very small files
        .max_depth(4)                    // Minimal depth
        .file_patterns(vec!["*.rs".to_string()])  // Only Rust
        .exclude_patterns(vec![
            "target/".to_string(),
            ".git/".to_string(),
            "tests/".to_string(),
            "examples/".to_string(),
            "benches/".to_string(),
        ])
        .respect_gitignore(true)
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    
    #[tokio::test]
    async fn test_performance_benchmark() {
        // Create a temporary directory with test files
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        
        // Create multiple test files to simulate a repository
        for i in 0..5 {
            let rust_file = temp_path.join(format!("test_{}.rs", i));
            fs::write(&rust_file, format!(r#"
pub fn function_{}() {{
    println!("Function {}", {});
}}

pub struct Struct{} {{
    field: i32,
}}
"#, i, i, i, i)).unwrap();
        }
        
        // Test benchmarking
        let config = create_standard_config().unwrap();
        let benchmark = run_single_benchmark(
            "Test", 
            &config, 
            temp_path.to_str().unwrap()
        ).await.unwrap();
        
        assert!(benchmark.scan_result.files_scanned >= 5);
        assert!(benchmark.scan_result.functions_found >= 5);
        assert!(benchmark.scan_duration < Duration::from_secs(10)); // Should be fast for small test
    }
} 