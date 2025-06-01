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