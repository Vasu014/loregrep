//! Demo of LoreGrep's auto-discovery functionality
//! 
//! This example shows how auto_discover() can automatically detect
//! project types and configure appropriate language analyzers.

use loregrep::LoreGrep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 LoreGrep Auto-Discovery Demo");
    println!("{}", "=".repeat(40));

    // Test 1: Auto-discover current directory (should detect Rust project)
    println!("\n1. Testing Auto-Discovery on Current Directory:");
    let mut loregrep = LoreGrep::auto_discover(".")?;
    // Expected output:
    // 🔍 Detected project languages: rust
    // ✅ Rust analyzer registered successfully
    // 🎆 LoreGrep configured with 1 language(s): rust
    // ✅ LoreGrep instance created successfully!

    // Test scan to verify it works
    let result = loregrep.scan(".").await?;
    println!("✅ Auto-discovered configuration scanned {} files", result.files_scanned);

    // Test 2: Auto-discover on a directory with no project indicators
    println!("\n2. Testing Auto-Discovery on Unknown Project Type:");
    let empty_loregrep = LoreGrep::auto_discover("/tmp")?;
    // Expected output:
    // ⚠️  No known project types detected in /tmp
    // 💡 Using default configuration (Rust + Python)
    // ✅ Rust analyzer registered successfully
    // ✅ Python analyzer registered successfully
    // 🎆 LoreGrep configured with 2 language(s): rust, python

    // Test 3: Test new preset methods
    println!("\n3. Testing Preset Methods:");
    let rust_project = LoreGrep::rust_project(".")?;
    println!("✅ Rust project preset created successfully!");
    
    let python_project = LoreGrep::python_project(".")?;
    println!("✅ Python project preset created successfully!");
    
    let polyglot_project = LoreGrep::polyglot_project(".")?;
    println!("✅ Polyglot project preset created successfully!");
    
    // Test 4: Test enhanced builder methods
    println!("\n4. Testing Enhanced Builder Methods:");
    let enhanced_builder = LoreGrep::builder()
        .with_all_analyzers()
        .exclude_common_build_dirs()
        .include_source_files()
        .build()?;
    println!("✅ Enhanced builder methods working!");

    // Test 5: Show project detection without creating instance
    println!("\n5. Project Detection Examples:");
    test_project_detection().await?;

    println!("\n🎉 Auto-discovery demo completed!");
    println!("💡 Use LoreGrep::auto_discover(path) for zero-configuration setup!");

    Ok(())
}

async fn test_project_detection() -> Result<(), Box<dyn std::error::Error>> {
    // Test detection on current directory (Rust project)
    println!("Current directory:");
    let _rust_project = LoreGrep::auto_discover(".")?;
    
    // Would detect other project types if present:
    println!("\nProject detection patterns:");
    println!("🦀 Rust: Looks for Cargo.toml, Cargo.lock");
    println!("🐍 Python: Looks for pyproject.toml, requirements.txt, setup.py, poetry.lock, .py files");
    println!("📜 TypeScript: Looks for tsconfig.json, package.json, .ts/.tsx files");
    println!("⚡ JavaScript: Looks for package.json, .js/.jsx/.mjs files"); 
    println!("🔷 Go: Looks for go.mod, go.sum files");
    
    Ok(())
}