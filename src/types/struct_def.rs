use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructField {
    pub name: String,
    pub field_type: String,
    pub is_public: bool,
}

impl StructField {
    pub fn new(name: String, field_type: String) -> Self {
        Self {
            name,
            field_type,
            is_public: false,
        }
    }

    pub fn with_visibility(mut self, is_public: bool) -> Self {
        self.is_public = is_public;
        self
    }

    pub fn format(&self) -> String {
        let visibility = if self.is_public { "pub " } else { "" };
        format!("{}{}: {}", visibility, self.name, self.field_type)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructSignature {
    pub name: String,
    pub fields: Vec<StructField>,
    pub is_public: bool,
    pub is_tuple_struct: bool,
    pub start_line: u32,
    pub end_line: u32,
    pub generics: Vec<String>,
}

impl StructSignature {
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            is_public: false,
            is_tuple_struct: false,
            start_line: 0,
            end_line: 0,
            generics: Vec::new(),
        }
    }

    pub fn with_fields(mut self, fields: Vec<StructField>) -> Self {
        self.fields = fields;
        self
    }

    pub fn with_visibility(mut self, is_public: bool) -> Self {
        self.is_public = is_public;
        self
    }

    pub fn with_tuple_struct(mut self, is_tuple_struct: bool) -> Self {
        self.is_tuple_struct = is_tuple_struct;
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

    pub fn format(&self) -> String {
        let visibility = if self.is_public { "pub " } else { "" };
        let generics_str = if self.generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", self.generics.join(", "))
        };

        if self.is_tuple_struct {
            let fields = self.fields
                .iter()
                .map(|f| {
                    let vis = if f.is_public { "pub " } else { "" };
                    format!("{}{}", vis, f.field_type)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}struct {}{}({})", visibility, self.name, generics_str, fields)
        } else {
            let fields = self.fields
                .iter()
                .map(|f| f.format())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}struct {}{} {{ {} }}", visibility, self.name, generics_str, fields)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportStatement {
    pub module_path: String,
    pub imported_items: Vec<String>,
    pub alias: Option<String>,
    pub is_glob: bool,
    pub is_external: bool,
    pub line_number: u32,
}

impl ImportStatement {
    pub fn new(module_path: String) -> Self {
        Self {
            module_path,
            imported_items: Vec::new(),
            alias: None,
            is_glob: false,
            is_external: false,
            line_number: 0,
        }
    }

    pub fn with_items(mut self, items: Vec<String>) -> Self {
        self.imported_items = items;
        self
    }

    pub fn with_alias(mut self, alias: String) -> Self {
        self.alias = Some(alias);
        self
    }

    pub fn with_glob(mut self, is_glob: bool) -> Self {
        self.is_glob = is_glob;
        self
    }

    pub fn with_external(mut self, is_external: bool) -> Self {
        self.is_external = is_external;
        self
    }

    pub fn with_line_number(mut self, line_number: u32) -> Self {
        self.line_number = line_number;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportStatement {
    pub exported_item: String,
    pub alias: Option<String>,
    pub is_public: bool,
    pub line_number: u32,
}

impl ExportStatement {
    pub fn new(exported_item: String) -> Self {
        Self {
            exported_item,
            alias: None,
            is_public: true,
            line_number: 0,
        }
    }

    pub fn with_alias(mut self, alias: String) -> Self {
        self.alias = Some(alias);
        self
    }

    pub fn with_visibility(mut self, is_public: bool) -> Self {
        self.is_public = is_public;
        self
    }

    pub fn with_line_number(mut self, line_number: u32) -> Self {
        self.line_number = line_number;
        self
    }
} 