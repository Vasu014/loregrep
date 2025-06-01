use serde::{Serialize, Deserialize};

/// Tool definition for LLM system prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl ToolSchema {
    pub fn new(name: String, description: String, input_schema: serde_json::Value) -> Self {
        Self {
            name,
            description,
            input_schema,
        }
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data,
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: serde_json::Value::Null,
            error: Some(error),
        }
    }
}

/// Result of repository scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub functions_found: usize,
    pub structs_found: usize,
    pub duration_ms: u64,
    pub languages: Vec<String>,
}

impl ScanResult {
    pub fn new(
        files_scanned: usize,
        functions_found: usize,
        structs_found: usize,
        duration_ms: u64,
        languages: Vec<String>,
    ) -> Self {
        Self {
            files_scanned,
            functions_found,
            structs_found,
            duration_ms,
            languages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_schema_creation() {
        let schema = ToolSchema::new(
            "search_functions".to_string(),
            "Search for functions".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern"
                    }
                },
                "required": ["pattern"]
            })
        );

        assert_eq!(schema.name, "search_functions");
        assert_eq!(schema.description, "Search for functions");
        assert!(!schema.input_schema.is_null());
    }

    #[test]
    fn test_tool_schema_serialization() {
        let schema = ToolSchema::new(
            "test_tool".to_string(),
            "Test tool".to_string(),
            json!({"type": "object"})
        );

        let serialized = serde_json::to_string(&schema).unwrap();
        let deserialized: ToolSchema = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(schema.name, deserialized.name);
        assert_eq!(schema.description, deserialized.description);
    }

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success(json!({"count": 5}));
        
        assert!(result.success);
        assert_eq!(result.data, json!({"count": 5}));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("Tool failed".to_string());
        
        assert!(!result.success);
        assert_eq!(result.data, serde_json::Value::Null);
        assert_eq!(result.error, Some("Tool failed".to_string()));
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult::success(json!({"test": "data"}));
        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(result.success, deserialized.success);
        assert_eq!(result.data, deserialized.data);
        assert_eq!(result.error, deserialized.error);
    }

    #[test]
    fn test_scan_result_creation() {
        let result = ScanResult::new(
            100,
            250,
            75,
            1500,
            vec!["rust".to_string(), "python".to_string()]
        );

        assert_eq!(result.files_scanned, 100);
        assert_eq!(result.functions_found, 250);
        assert_eq!(result.structs_found, 75);
        assert_eq!(result.duration_ms, 1500);
        assert_eq!(result.languages.len(), 2);
        assert!(result.languages.contains(&"rust".to_string()));
        assert!(result.languages.contains(&"python".to_string()));
    }

    #[test]
    fn test_scan_result_serialization() {
        let result = ScanResult::new(
            50,
            100,
            25,
            750,
            vec!["rust".to_string()]
        );

        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: ScanResult = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(result.files_scanned, deserialized.files_scanned);
        assert_eq!(result.functions_found, deserialized.functions_found);
        assert_eq!(result.structs_found, deserialized.structs_found);
        assert_eq!(result.duration_ms, deserialized.duration_ms);
        assert_eq!(result.languages, deserialized.languages);
    }
}