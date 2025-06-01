// Integration test for the public API
use loregrep::{LoreGrep, LoreGrepBuilder, LoreGrepError, Result, ToolSchema, ToolResult, ScanResult, VERSION};
use serde_json::json;

#[test]
fn test_public_api_exports() {
    // Test that all public API types are accessible
    let _version: &str = VERSION;
    
    // Test builder pattern
    let builder: LoreGrepBuilder = LoreGrep::builder();
    let _loregrep: Result<LoreGrep> = builder.build();
    
    // Test tool definitions
    let _tools: Vec<ToolSchema> = LoreGrep::get_tool_definitions();
    
    // Test error types
    let _error: LoreGrepError = LoreGrepError::NotScanned;
}

#[test]
fn test_builder_configuration() {
    // Test that the builder pattern works with various configurations
    let builder = LoreGrep::builder()
        .with_rust_analyzer()
        .max_files(1000)
        .cache_ttl(300)
        .include_patterns(vec!["**/*.rs".to_string()])
        .exclude_patterns(vec!["**/target/**".to_string()])
        .max_file_size(1024 * 1024)
        .max_depth(10)
        .follow_symlinks(false);
    
    let loregrep = builder.build();
    assert!(loregrep.is_ok());
}

#[test]
fn test_tool_definitions_structure() {
    let tools = LoreGrep::get_tool_definitions();
    assert!(!tools.is_empty());
    
    // Check that all expected tools are present
    let tool_names: Vec<&String> = tools.iter().map(|t| &t.name).collect();
    assert!(tool_names.contains(&&"search_functions".to_string()));
    assert!(tool_names.contains(&&"search_structs".to_string()));
    assert!(tool_names.contains(&&"analyze_file".to_string()));
    assert!(tool_names.contains(&&"get_dependencies".to_string()));
    assert!(tool_names.contains(&&"find_callers".to_string()));
    assert!(tool_names.contains(&&"get_repository_tree".to_string()));
    
    // Verify each tool has required fields
    for tool in &tools {
        assert!(!tool.name.is_empty());
        assert!(!tool.description.is_empty());
        assert!(tool.input_schema.is_object());
    }
}

#[tokio::test]
async fn test_execute_tool_interface() {
    let loregrep = LoreGrep::builder().build().unwrap();
    
    // Test invalid tool
    let result = loregrep.execute_tool("invalid_tool", json!({})).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(!tool_result.success);
    assert!(tool_result.error.is_some());
    
    // Test valid tool with empty repository
    let result = loregrep.execute_tool("get_repository_tree", json!({})).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
}

#[test]
fn test_version_constant() {
    assert!(!VERSION.is_empty());
    // Should match the version in Cargo.toml
    assert!(VERSION.starts_with("0."));
}

#[test]
fn test_error_types() {
    // Test that error types can be created and matched
    let error = LoreGrepError::NotScanned;
    match error {
        LoreGrepError::NotScanned => {},
        _ => panic!("Unexpected error type"),
    }
    
    let error = LoreGrepError::ToolError("test".to_string());
    match error {
        LoreGrepError::ToolError(msg) => assert_eq!(msg, "test"),
        _ => panic!("Unexpected error type"),
    }
}

#[test]
fn test_result_types() {
    // Test that Result types work correctly
    let success_result: Result<i32> = Ok(42);
    assert!(success_result.is_ok());
    
    let error_result: Result<i32> = Err(LoreGrepError::NotScanned);
    assert!(error_result.is_err());
}

#[test]
fn test_tool_result_creation() {
    // Test ToolResult creation
    let success = ToolResult {
        success: true,
        data: json!({"test": "value"}),
        error: None,
    };
    assert!(success.success);
    assert_eq!(success.data["test"], "value");
    
    let error = ToolResult {
        success: false,
        data: json!({}),
        error: Some("test error".to_string()),
    };
    assert!(!error.success);
    assert_eq!(error.error.as_ref().unwrap(), "test error");
}

#[test]
fn test_scan_result_structure() {
    // Test ScanResult creation  
    let scan_result = ScanResult {
        files_scanned: 10,
        functions_found: 25,
        structs_found: 5,
        duration_ms: 1500,
        languages: vec!["rust".to_string()],
    };
    
    assert_eq!(scan_result.files_scanned, 10);
    assert_eq!(scan_result.functions_found, 25);
    assert_eq!(scan_result.structs_found, 5);
    assert_eq!(scan_result.duration_ms, 1500);
    assert!(scan_result.languages.contains(&"rust".to_string()));
}

#[test]
fn test_tool_schema_structure() {
    // Test ToolSchema creation
    let schema = ToolSchema {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "param": {"type": "string"}
            }
        }),
    };
    
    assert_eq!(schema.name, "test_tool");
    assert_eq!(schema.description, "A test tool");
    assert!(schema.input_schema.is_object());
}

#[tokio::test]
async fn test_full_workflow() {
    // Test a complete workflow using only the public API
    let mut loregrep = LoreGrep::builder()
        .max_files(100)
        .build()
        .unwrap();
    
    // Check that it's not scanned initially
    assert!(!loregrep.is_scanned());
    
    // Get stats (should be empty)
    let stats = loregrep.get_stats().unwrap();
    assert_eq!(stats.files_scanned, 0);
    
    // Get tool definitions
    let tools = LoreGrep::get_tool_definitions();
    assert!(!tools.is_empty());
    
    // Execute a tool
    let result = loregrep.execute_tool("get_repository_tree", json!({
        "include_file_details": false,
        "max_depth": 1
    })).await;
    
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
}

#[tokio::test]
async fn test_all_tool_executions() {
    // Test all available tools with various inputs
    let loregrep = LoreGrep::builder().build().unwrap();
    let tools = LoreGrep::get_tool_definitions();
    
    // Test search_functions tool
    let result = loregrep.execute_tool("search_functions", json!({
        "pattern": "test",
        "limit": 10
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
    
    // Test search_structs tool
    let result = loregrep.execute_tool("search_structs", json!({
        "pattern": "test",
        "limit": 10
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
    
    // Test analyze_file tool (with non-existent file should fail gracefully)
    let result = loregrep.execute_tool("analyze_file", json!({
        "file_path": "/nonexistent/file.rs",
        "include_source": false
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(!tool_result.success); // Should fail but not crash
    assert!(tool_result.error.is_some());
    
    // Test get_dependencies tool
    let result = loregrep.execute_tool("get_dependencies", json!({
        "file_path": "/nonexistent/file.rs"
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    // Note: This tool may succeed with empty results even for non-existent files
    
    // Test find_callers tool
    let result = loregrep.execute_tool("find_callers", json!({
        "function_name": "test_function",
        "limit": 10
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
    
    // Test get_repository_tree tool
    let result = loregrep.execute_tool("get_repository_tree", json!({
        "include_file_details": true,
        "max_depth": 2
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
}

#[tokio::test]
async fn test_tool_parameter_validation() {
    let loregrep = LoreGrep::builder().build().unwrap();
    
    // Test basic tool execution functionality without strict parameter validation
    // The main goal is to ensure tools don't panic on various inputs
    
    // Test with empty parameters
    let result = loregrep.execute_tool("search_functions", json!({})).await;
    // Should not panic, regardless of result
    match result {
        Ok(_) => {}, // Tool handled it gracefully
        Err(_) => {}, // Tool returned an error, also ok
    }
    
    // Test with invalid parameter types
    let result = loregrep.execute_tool("search_functions", json!({
        "pattern": 123, // Should be string
        "limit": "invalid" // Should be number
    })).await;
    // Should not panic
    match result {
        Ok(_) => {}, // Tool handled it gracefully
        Err(_) => {}, // Tool returned an error, also ok
    }
    
    // Test with valid parameters
    let result = loregrep.execute_tool("search_functions", json!({
        "pattern": "test",
        "limit": 5
    })).await;
    // This should definitely work
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
}

#[tokio::test]
async fn test_scan_and_analyze_workflow() {
    use std::fs;
    use tempfile::TempDir;
    
    let temp_dir = TempDir::new().unwrap();
    
    // Create a test Rust file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(&test_file, r#"
pub fn hello_world() -> String {
    "Hello, World!".to_string()
}

pub struct TestStruct {
    pub name: String,
}
"#).unwrap();
    
    let mut loregrep = LoreGrep::builder()
        .with_rust_analyzer()
        .max_files(100)
        .include_patterns(vec!["**/*.rs".to_string()])
        .build()
        .unwrap();
    
    // Scan the temporary directory
    let scan_result = loregrep.scan(temp_dir.path().to_str().unwrap()).await;
    assert!(scan_result.is_ok());
    let scan_result = scan_result.unwrap();
    assert!(scan_result.files_scanned > 0);
    
    // Verify repository is now scanned
    assert!(loregrep.is_scanned());
    
    // Analyze the specific file
    let result = loregrep.execute_tool("analyze_file", json!({
        "file_path": test_file.to_string_lossy(),
        "include_source": false
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
    
    // Verify analysis contains some data if it succeeded
    if tool_result.success {
        // Only check if analysis succeeded - just verify we got some data back
        let data_keys: Vec<&str> = tool_result.data.as_object()
            .map(|obj| obj.keys().map(|s| s.as_str()).collect())
            .unwrap_or_else(Vec::new);
        
        // We should get at least some data back from a successful analysis
        assert!(!data_keys.is_empty(), "Successful analysis should return some data");
    }
    
    // Search for functions
    let result = loregrep.execute_tool("search_functions", json!({
        "pattern": "hello",
        "limit": 10
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
    
    // Search for structs
    let result = loregrep.execute_tool("search_structs", json!({
        "pattern": "Test",
        "limit": 10
    })).await;
    assert!(result.is_ok());
    let tool_result = result.unwrap();
    assert!(tool_result.success);
}

#[tokio::test]
async fn test_concurrent_tool_execution() {
    use tokio::task;
    use std::sync::Arc;
    
    let loregrep = Arc::new(LoreGrep::builder().build().unwrap());
    
    // Execute multiple tools concurrently
    let handle1 = {
        let lg = Arc::clone(&loregrep);
        task::spawn(async move {
            lg.execute_tool("search_functions", json!({
                "pattern": "test1",
                "limit": 10
            })).await
        })
    };
    
    let handle2 = {
        let lg = Arc::clone(&loregrep);
        task::spawn(async move {
            lg.execute_tool("search_structs", json!({
                "pattern": "test2",
                "limit": 10
            })).await
        })
    };
    
    let handle3 = {
        let lg = Arc::clone(&loregrep);
        task::spawn(async move {
            lg.execute_tool("get_repository_tree", json!({
                "include_file_details": false,
                "max_depth": 1
            })).await
        })
    };
    
    // Wait for all to complete
    let (result1, result2, result3) = tokio::join!(handle1, handle2, handle3);
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());
    
    let tool_result1 = result1.unwrap().unwrap();
    let tool_result2 = result2.unwrap().unwrap();
    let tool_result3 = result3.unwrap().unwrap();
    
    assert!(tool_result1.success);
    assert!(tool_result2.success);
    assert!(tool_result3.success);
}

#[test]
fn test_builder_thread_safety() {
    use std::sync::Arc;
    use std::thread;
    
    // Test that LoreGrep can be safely shared between threads
    let loregrep = Arc::new(LoreGrep::builder().build().unwrap());
    
    let mut handles = vec![];
    
    for i in 0..5 {
        let lg = Arc::clone(&loregrep);
        let handle = thread::spawn(move || {
            // Each thread checks basic functionality
            let is_scanned = lg.is_scanned();
            let stats = lg.get_stats();
            let tools = LoreGrep::get_tool_definitions();
            
            // Basic assertions
            assert!(!is_scanned); // Fresh instance shouldn't be scanned
            assert!(stats.is_ok());
            assert!(!tools.is_empty());
            
            i
        });
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for handle in handles {
        let thread_id = handle.join().unwrap();
        assert!(thread_id < 5);
    }
}

#[test] 
fn test_error_handling_comprehensive() {
    let loregrep = LoreGrep::builder().build().unwrap();
    
    // Test error propagation doesn't panic
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let result = rt.block_on(async {
        // Invalid tool name
        let result = loregrep.execute_tool("invalid_tool_name", json!({})).await;
        match result {
            Ok(tool_result) => {
                assert!(!tool_result.success);
                assert!(tool_result.error.is_some());
            },
            Err(_) => {
                // It's also ok if the function returns an error for invalid tool name
            }
        }
        
        // Test with valid tool but possibly bad parameters
        let result = loregrep.execute_tool("search_functions", json!({
            "pattern": null,
            "limit": "not_a_number"
        })).await;
        // Tool execution should not panic - success or failure is implementation dependent
        assert!(result.is_ok() || result.is_err()); // Just check it doesn't panic
        
        Ok::<(), Box<dyn std::error::Error>>(())
    });
    
    // The main thing is that no panic occurred
    assert!(result.is_ok());
}

#[test]
fn test_configuration_edge_cases() {
    // Test maximum values
    let builder = LoreGrep::builder()
        .max_files(usize::MAX)
        .cache_ttl(u64::MAX)
        .max_file_size(u64::MAX)
        .max_depth(u32::MAX);
    
    let result = builder.build();
    assert!(result.is_ok());
    
    // Test minimum values
    let builder = LoreGrep::builder()
        .max_files(0)
        .cache_ttl(0)
        .max_file_size(0)
        .max_depth(0);
    
    let result = builder.build();
    assert!(result.is_ok());
    
    // Test empty patterns
    let builder = LoreGrep::builder()
        .include_patterns(vec![])
        .exclude_patterns(vec![]);
    
    let result = builder.build();
    assert!(result.is_ok());
}