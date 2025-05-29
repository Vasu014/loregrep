use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::fs;
use std::path::Path;
use tree_sitter::{Language, Parser};

// Mock functions for now - these will be replaced with actual loregrep code
fn parse_rust_file(content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_rust::language())?;
    let tree = parser.parse(content, None).unwrap();
    black_box(tree);
    Ok(())
}

fn parse_python_file(content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_python::language())?;
    let tree = parser.parse(content, None).unwrap();
    black_box(tree);
    Ok(())
}

fn parse_typescript_file(content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_typescript::language_typescript())?;
    let tree = parser.parse(content, None).unwrap();
    black_box(tree);
    Ok(())
}

fn benchmark_rust_parsing(c: &mut Criterion) {
    let sample_rust_code = r#"
pub struct Example {
    pub id: u64,
    pub name: String,
}

impl Example {
    pub fn new(id: u64, name: String) -> Self {
        Self { id, name }
    }
    
    pub async fn process(&self) -> Result<(), std::io::Error> {
        println!("Processing {}", self.name);
        Ok(())
    }
}

pub fn main() {
    let example = Example::new(1, "test".to_string());
    println!("Created example: {:?}", example);
}
"#;

    c.bench_function("parse_rust_small", |b| {
        b.iter(|| parse_rust_file(black_box(sample_rust_code)))
    });
}

fn benchmark_python_parsing(c: &mut Criterion) {
    let sample_python_code = r#"
class Example:
    def __init__(self, id: int, name: str):
        self.id = id
        self.name = name
    
    async def process(self) -> None:
        print(f"Processing {self.name}")
    
    def __repr__(self) -> str:
        return f"Example(id={self.id}, name='{self.name}')"

def main():
    example = Example(1, "test")
    print(f"Created example: {example}")

if __name__ == "__main__":
    main()
"#;

    c.bench_function("parse_python_small", |b| {
        b.iter(|| parse_python_file(black_box(sample_python_code)))
    });
}

fn benchmark_typescript_parsing(c: &mut Criterion) {
    let sample_typescript_code = r#"
interface ExampleInterface {
    id: number;
    name: string;
}

class Example implements ExampleInterface {
    constructor(public id: number, public name: string) {}
    
    async process(): Promise<void> {
        console.log(`Processing ${this.name}`);
    }
    
    toString(): string {
        return `Example(id=${this.id}, name='${this.name}')`;
    }
}

function main(): void {
    const example = new Example(1, "test");
    console.log(`Created example: ${example}`);
}

main();
"#;

    c.bench_function("parse_typescript_small", |b| {
        b.iter(|| parse_typescript_file(black_box(sample_typescript_code)))
    });
}

fn benchmark_large_file_parsing(c: &mut Criterion) {
    // Generate a larger Rust file for performance testing
    let large_rust_code = (0..100)
        .map(|i| {
            format!(
                r#"
pub struct Generated{} {{
    pub id: u64,
    pub name: String,
    pub value: f64,
}}

impl Generated{} {{
    pub fn new(id: u64, name: String, value: f64) -> Self {{
        Self {{ id, name, value }}
    }}
    
    pub async fn process(&self) -> Result<(), std::io::Error> {{
        println!("Processing {{}} with value {{}}", self.name, self.value);
        Ok(())
    }}
    
    pub fn calculate(&self) -> f64 {{
        self.value * 2.0 + self.id as f64
    }}
}}
"#,
                i, i
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    c.bench_function("parse_rust_large", |b| {
        b.iter(|| parse_rust_file(black_box(&large_rust_code)))
    });
}

criterion_group!(
    benches,
    benchmark_rust_parsing,
    benchmark_python_parsing,
    benchmark_typescript_parsing,
    benchmark_large_file_parsing
);
criterion_main!(benches); 