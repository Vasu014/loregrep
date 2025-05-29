use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub param_type: String,
    pub default_value: Option<String>,
    pub is_mutable: bool,
}

impl Parameter {
    pub fn new(name: String, param_type: String) -> Self {
        Self {
            name,
            param_type,
            default_value: None,
            is_mutable: false,
        }
    }

    pub fn with_default(mut self, default_value: String) -> Self {
        self.default_value = Some(default_value);
        self
    }

    pub fn with_mutability(mut self, is_mutable: bool) -> Self {
        self.is_mutable = is_mutable;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionSignature {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub is_public: bool,
    pub is_async: bool,
    pub is_const: bool,
    pub is_static: bool,
    pub is_extern: bool,
    pub start_line: u32,
    pub end_line: u32,
    pub generics: Vec<String>,
}

impl FunctionSignature {
    pub fn new(name: String) -> Self {
        Self {
            name,
            parameters: Vec::new(),
            return_type: None,
            is_public: false,
            is_async: false,
            is_const: false,
            is_static: false,
            is_extern: false,
            start_line: 0,
            end_line: 0,
            generics: Vec::new(),
        }
    }

    pub fn with_parameters(mut self, parameters: Vec<Parameter>) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_return_type(mut self, return_type: String) -> Self {
        self.return_type = Some(return_type);
        self
    }

    pub fn with_visibility(mut self, is_public: bool) -> Self {
        self.is_public = is_public;
        self
    }

    pub fn with_async(mut self, is_async: bool) -> Self {
        self.is_async = is_async;
        self
    }

    pub fn with_const(mut self, is_const: bool) -> Self {
        self.is_const = is_const;
        self
    }

    pub fn with_static(mut self, is_static: bool) -> Self {
        self.is_static = is_static;
        self
    }

    pub fn with_extern(mut self, is_extern: bool) -> Self {
        self.is_extern = is_extern;
        self
    }

    pub fn with_location(mut self, start_line: u32, end_line: u32) -> Self {
        self.start_line = start_line;
        self.end_line = end_line;
        self
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    /// Format the function signature for display
    pub fn format(&self) -> String {
        let visibility = if self.is_public { "pub " } else { "" };
        let const_keyword = if self.is_const { "const " } else { "" };
        let async_keyword = if self.is_async { "async " } else { "" };
        let extern_keyword = if self.is_extern { "extern " } else { "" };
        
        let generics_str = if self.generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", self.generics.join(", "))
        };
        
        let params = self.parameters
            .iter()
            .map(|p| {
                let mutability = if p.is_mutable { "mut " } else { "" };
                format!("{}{}: {}", mutability, p.name, p.param_type)
            })
            .collect::<Vec<_>>()
            .join(", ");
        
        let return_type = self.return_type
            .as_ref()
            .map(|t| format!(" -> {}", t))
            .unwrap_or_default();
        
        format!(
            "{}{}{}{}fn {}{}({}){}",
            visibility, extern_keyword, const_keyword, async_keyword, 
            self.name, generics_str, params, return_type
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub function_name: String,
    pub file_path: String,
    pub line_number: u32,
    pub column: u32,
    pub is_method_call: bool,
    pub receiver_type: Option<String>,
}

impl FunctionCall {
    pub fn new(function_name: String, file_path: String, line_number: u32) -> Self {
        Self {
            function_name,
            file_path,
            line_number,
            column: 0,
            is_method_call: false,
            receiver_type: None,
        }
    }

    pub fn with_column(mut self, column: u32) -> Self {
        self.column = column;
        self
    }

    pub fn with_method_call(mut self, receiver_type: String) -> Self {
        self.is_method_call = true;
        self.receiver_type = Some(receiver_type);
        self
    }
} 