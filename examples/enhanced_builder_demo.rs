//! Demo of LoreGrep's enhanced builder methods and convenience APIs
//!
//! This example shows the new enhanced builder methods introduced in Phase 3:
//! Enhanced User Experience, providing convenient preset configurations.

use loregrep::LoreGrep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 LoreGrep Enhanced Builder Methods Demo");
    println!("{}", "=".repeat(50));

    // Demo 1: Performance-optimized configuration
    println!("\n1. Performance-Optimized Configuration:");
    let _performance_build = LoreGrep::builder()
        .with_rust_analyzer()
        .optimize_for_performance() // New: 512KB file limit, depth 8
        .exclude_common_build_dirs() // Enhanced: More exclusions
        .exclude_vendor_dirs() // New: Exclude vendor directories
        .build()?;
    println!("✅ Performance-optimized build created");
    println!("   📁 Max file size: 512KB, Max depth: 8");
    println!("   🚫 Excludes: build dirs, vendor dirs, binary files");

    // Demo 2: Comprehensive analysis configuration
    println!("\n2. Comprehensive Analysis Configuration:");
    let _comprehensive_build = LoreGrep::builder()
        .with_all_analyzers() // Enhanced: All available analyzers
        .comprehensive_analysis() // New: 5MB limit, depth 20, more file types
        .include_config_files() // New: Include config files
        .exclude_test_dirs() // New: Exclude test directories
        .build()?;
    println!("✅ Comprehensive analysis build created");
    println!("   📁 Max file size: 5MB, Max depth: 20");
    println!("   ✅ Includes: source files, configs, documentation");
    println!("   🚫 Excludes: test directories");

    // Demo 3: Custom combination of convenience methods
    println!("\n3. Custom Combination Build:");
    let _custom_build = LoreGrep::builder()
        .with_rust_analyzer()
        .with_python_analyzer()
        .exclude_common_build_dirs()
        .exclude_test_dirs()
        .exclude_vendor_dirs()
        .include_source_files()
        .include_config_files()
        .max_files(5000)
        .build()?;
    println!("✅ Custom combination build created");
    println!("   🔧 Custom limits with selective inclusions/exclusions");

    // Demo 4: Project-specific presets vs enhanced builder
    println!("\n4. Project Presets vs Enhanced Builder:");

    // Using preset (simple, one-line)
    let _rust_preset = LoreGrep::rust_project(".")?;
    println!("✅ Rust preset: LoreGrep::rust_project(\".\")");

    // Using enhanced builder (more control)
    let _enhanced_rust = LoreGrep::builder()
        .with_rust_analyzer()
        .include_patterns(vec!["**/*.rs".to_string(), "**/*.toml".to_string()])
        .exclude_common_build_dirs()
        .exclude_test_dirs() // Additional: exclude tests
        .optimize_for_performance() // Additional: performance optimization
        .build()?;
    println!("✅ Enhanced Rust build with additional optimizations");

    // Demo 5: Auto-discovery vs manual configuration comparison
    println!("\n5. Auto-Discovery vs Manual Configuration:");

    // Auto-discovery (zero configuration)
    let _auto_discovered = LoreGrep::auto_discover(".")?;
    println!("✅ Auto-discovery: LoreGrep::auto_discover(\".\")");

    // Manual enhanced configuration
    let _manual_enhanced = LoreGrep::builder()
        .with_all_analyzers()
        .exclude_common_build_dirs()
        .exclude_test_dirs()
        .exclude_vendor_dirs()
        .include_source_files()
        .optimize_for_performance()
        .build()?;
    println!("✅ Manual enhanced configuration with all convenience methods");

    println!("\n🎉 Enhanced Builder Demo Complete!");
    println!("💡 Key benefits:");
    println!("   🚀 Performance presets for speed-critical applications");
    println!("   🔍 Comprehensive presets for thorough analysis");
    println!("   🎛️  Granular control with convenience methods");
    println!("   🎯 One-line presets for common project types");
    println!("   🤖 Zero-config auto-discovery for quick setup");

    Ok(())
}
