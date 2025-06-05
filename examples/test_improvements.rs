//! Example showcasing the improved error messages and user feedback

use loregrep::LoreGrep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing Improved LoreGrep Public API");
    println!("{}", "=".repeat(50));

    // Test 1: Builder with enhanced feedback
    println!("\n1. Testing Builder with Enhanced Feedback...");
    let mut loregrep = LoreGrep::builder()
        .with_rust_analyzer()
        .with_python_analyzer()
        .build()?;

    // Test 2: Scan with improved error messages
    println!("\n2. Testing Scan with Improved Error Messages...");
    
    // Test scanning a directory that doesn't exist
    println!("\n🔍 Testing scan on non-existent directory:");
    let result = loregrep.scan("/path/that/does/not/exist").await;
    match result {
        Ok(scan_result) => {
            println!("✅ Scan completed gracefully: {} files", scan_result.files_scanned);
        }
        Err(e) => {
            println!("⚠️  Scan error (expected): {}", e);
        }
    }

    // Test scanning current directory 
    println!("\n🔍 Testing scan on current directory:");
    let scan_result = loregrep.scan(".").await?;
    println!("✅ Scan completed successfully!");

    // Test 3: Builder without any analyzers
    println!("\n3. Testing Builder without Language Analyzers...");
    let mut empty_loregrep = LoreGrep::builder()
        .build()?;
    
    println!("\n🔍 Testing scan with no analyzers:");
    let empty_result = empty_loregrep.scan(".").await?;
    println!("✅ Empty scan completed: {} files", empty_result.files_scanned);

    println!("\n🎉 All tests completed!");
    Ok(())
}