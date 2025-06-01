// Basic repository scanning example
// This example demonstrates the simplest way to scan a repository and get basic statistics

use loregrep::{LoreGrep, Result as LoreGrepResult};

#[tokio::main]
async fn main() -> LoreGrepResult<()> {
    println!("🔍 Basic Repository Scanning Example");
    println!("====================================\n");

    // Create a LoreGrep instance with minimal configuration
    let mut loregrep = LoreGrep::builder()
        .with_rust_analyzer()                 // Enable Rust language support
        .max_files(500)                       // Limit to 500 files for quick scanning
        .build()?;

    println!("📁 Scanning current directory...");
    
    // Scan the current directory
    let scan_result = loregrep.scan(".").await?;
    
    // Display scan results
    println!("✅ Scan completed successfully!\n");
    println!("📊 Scan Statistics:");
    println!("   • Files scanned: {}", scan_result.files_scanned);
    println!("   • Functions found: {}", scan_result.functions_found);
    println!("   • Structs found: {}", scan_result.structs_found);
    println!("   • Scan duration: {}ms", scan_result.duration_ms);
    
    if !scan_result.languages.is_empty() {
        println!("   • Languages detected: {:?}", scan_result.languages);
    }

    // Check if repository was successfully indexed
    if loregrep.is_scanned() {
        println!("\n🎉 Repository is now indexed and ready for analysis!");
        
        // Get updated statistics
        let stats = loregrep.get_stats()?;
        println!("📋 Current index contains:");
        println!("   • {} files", stats.files_scanned);
        println!("   • {} functions", stats.functions_found);
        println!("   • {} structs", stats.structs_found);
    } else {
        println!("\n⚠️  Repository scanning completed but no files were indexed.");
        println!("    This might happen if no supported files were found.");
    }

    println!("\n💡 Next steps:");
    println!("   • Use the search tools to find specific functions or structs");
    println!("   • Analyze individual files for detailed information");
    println!("   • Integrate with your coding assistant or development workflow");

    Ok(())
}