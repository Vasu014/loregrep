// File watcher example - Updating on file changes
// This example demonstrates how to watch for file changes and update the LoreGrep index

use loregrep::{LoreGrep, Result as LoreGrepResult};
use notify::{Watcher, RecursiveMode, watcher, DebouncedEvent};
use std::sync::mpsc::channel;
use std::time::Duration;
use std::path::Path;

#[tokio::main]
async fn main() -> LoreGrepResult<()> {
    println!("👁️  File Watcher Integration Example");
    println!("===================================\n");

    // Initialize LoreGrep
    let mut loregrep = LoreGrep::builder()
        .with_rust_analyzer()
        .max_files(1000)
        .include_patterns(vec!["**/*.rs".to_string(), "**/*.toml".to_string()])
        .exclude_patterns(vec![
            "**/target/**".to_string(),
            "**/.git/**".to_string(),
            "**/test-repos/**".to_string(),
        ])
        .build()?;

    println!("📁 Initial repository scan...");
    let initial_scan = loregrep.scan(".").await?;
    println!("   ✅ Scanned {} files, found {} functions", 
        initial_scan.files_scanned, initial_scan.functions_found);

    // Set up file watcher
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_secs(2))
        .map_err(|e| loregrep::LoreGrepError::InternalError(format!("Failed to create file watcher: {}", e)))?;

    println!("\n👁️  Setting up file watcher for current directory...");
    watcher.watch(".", RecursiveMode::Recursive)
        .map_err(|e| loregrep::LoreGrepError::InternalError(format!("Failed to start watching: {}", e)))?;

    println!("   ✅ File watcher started");
    println!("   📝 Watching for changes to .rs and .toml files");
    println!("   ⏱️  Changes are debounced for 2 seconds");
    println!("\n💡 Try modifying a Rust file in another terminal to see updates...");
    println!("   Press Ctrl+C to stop watching\n");

    let mut update_count = 0;

    // Watch for file changes
    loop {
        match rx.recv() {
            Ok(event) => {
                match event {
                    DebouncedEvent::Write(path) | 
                    DebouncedEvent::Create(path) | 
                    DebouncedEvent::Remove(path) => {
                        if should_rescan(&path) {
                            update_count += 1;
                            println!("🔄 File change detected: {}", path.display());
                            println!("   📊 Update #{}", update_count);
                            
                            // Rescan the repository
                            match loregrep.scan(".").await {
                                Ok(scan_result) => {
                                    println!("   ✅ Rescan completed:");
                                    println!("      • Files: {}", scan_result.files_scanned);
                                    println!("      • Functions: {}", scan_result.functions_found);
                                    println!("      • Structs: {}", scan_result.structs_found);
                                    println!("      • Duration: {}ms", scan_result.duration_ms);
                                    
                                    // Show change in statistics if available
                                    let change_functions = scan_result.functions_found as i32 - initial_scan.functions_found as i32;
                                    let change_structs = scan_result.structs_found as i32 - initial_scan.structs_found as i32;
                                    
                                    if change_functions != 0 || change_structs != 0 {
                                        println!("   📈 Changes since initial scan:");
                                        if change_functions != 0 {
                                            println!("      • Functions: {:+}", change_functions);
                                        }
                                        if change_structs != 0 {
                                            println!("      • Structs: {:+}", change_structs);
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("   ❌ Rescan failed: {}", e);
                                }
                            }
                            
                            println!("   🎯 Index updated and ready for queries");
                            println!();
                        }
                    }
                    DebouncedEvent::Rename(from, to) => {
                        if should_rescan(&from) || should_rescan(&to) {
                            update_count += 1;
                            println!("🔄 File renamed: {} -> {}", from.display(), to.display());
                            println!("   📊 Update #{} (rescanning...)", update_count);
                            
                            match loregrep.scan(".").await {
                                Ok(scan_result) => {
                                    println!("   ✅ Rescan completed: {} files, {} functions", 
                                        scan_result.files_scanned, scan_result.functions_found);
                                }
                                Err(e) => {
                                    println!("   ❌ Rescan failed: {}", e);
                                }
                            }
                            println!();
                        }
                    }
                    _ => {
                        // Ignore other events like chmod, etc.
                    }
                }
            }
            Err(e) => {
                println!("❌ Watch error: {:?}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Check if a file change should trigger a rescan
fn should_rescan(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        matches!(extension.to_str(), Some("rs") | Some("toml"))
    } else {
        false
    }
}

// Note: This example requires the `notify` crate as a dependency.
// Add this to your Cargo.toml:
//
// [dependencies]
// notify = "4.0"
// loregrep = { path = "." }
// tokio = { version = "1.0", features = ["full"] }

#[cfg(feature = "file-watching")]
mod integration_patterns {
    use super::*;
    
    /// Example of integrating file watching with a coding assistant
    pub struct WatchingCodingAssistant {
        loregrep: LoreGrep,
        _watcher: notify::RecommendedWatcher,
    }
    
    impl WatchingCodingAssistant {
        pub async fn new(project_path: &str) -> LoreGrepResult<Self> {
            let mut loregrep = LoreGrep::builder()
                .with_rust_analyzer()
                .build()?;
            
            // Initial scan
            loregrep.scan(project_path).await?;
            
            // Set up file watcher (implementation details omitted for brevity)
            let (tx, _rx) = std::sync::mpsc::channel();
            let watcher = notify::watcher(tx, Duration::from_secs(1))
                .map_err(|e| loregrep::LoreGrepError::InternalError(e.to_string()))?;
            
            Ok(Self {
                loregrep,
                _watcher: watcher,
            })
        }
        
        pub async fn handle_file_change(&mut self, _changed_path: &Path) -> LoreGrepResult<()> {
            // Rescan when files change
            self.loregrep.scan(".").await?;
            Ok(())
        }
        
        pub async fn query(&self, tool_name: &str, params: serde_json::Value) -> LoreGrepResult<serde_json::Value> {
            let result = self.loregrep.execute_tool(tool_name, params).await?;
            Ok(serde_json::to_value(result)?)
        }
    }
}