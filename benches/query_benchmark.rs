use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::collections::HashMap;

// Mock data structures for benchmarking - these will be replaced with actual loregrep types
#[derive(Clone, Debug)]
struct MockFunction {
    id: u64,
    name: String,
    file_path: String,
    signature: String,
    is_async: bool,
    visibility: String,
    parameter_count: u32,
}

#[derive(Clone, Debug)]
struct MockStruct {
    id: u64,
    name: String,
    file_path: String,
    field_count: u32,
    visibility: String,
}

// Mock search implementations
fn exact_search(functions: &[MockFunction], pattern: &str) -> Vec<&MockFunction> {
    functions
        .iter()
        .filter(|f| f.name.contains(pattern))
        .collect()
}

fn regex_search(functions: &[MockFunction], pattern: &str) -> Vec<&MockFunction> {
    let regex = regex::Regex::new(pattern).unwrap();
    functions
        .iter()
        .filter(|f| regex.is_match(&f.name))
        .collect()
}

fn fuzzy_search(functions: &[MockFunction], pattern: &str) -> Vec<(&MockFunction, i64)> {
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;
    
    let matcher = SkimMatcherV2::default();
    functions
        .iter()
        .filter_map(|f| {
            matcher.fuzzy_match(&f.name, pattern).map(|score| (f, score))
        })
        .collect()
}

fn filtered_search(
    functions: &[MockFunction], 
    pattern: &str, 
    is_async: Option<bool>,
    min_params: Option<u32>
) -> Vec<&MockFunction> {
    functions
        .iter()
        .filter(|f| f.name.contains(pattern))
        .filter(|f| is_async.map_or(true, |async_filter| f.is_async == async_filter))
        .filter(|f| min_params.map_or(true, |min| f.parameter_count >= min))
        .collect()
}

// Generate test data
fn generate_mock_functions(count: usize) -> Vec<MockFunction> {
    (0..count)
        .map(|i| MockFunction {
            id: i as u64,
            name: match i % 10 {
                0 => format!("authenticate_user_{}", i),
                1 => format!("validate_token_{}", i),
                2 => format!("process_request_{}", i),
                3 => format!("handle_error_{}", i),
                4 => format!("parse_data_{}", i),
                5 => format!("serialize_response_{}", i),
                6 => format!("connect_database_{}", i),
                7 => format!("send_notification_{}", i),
                8 => format!("log_activity_{}", i),
                _ => format!("utility_function_{}", i),
            },
            file_path: format!("src/module_{}.rs", i % 20),
            signature: format!("fn function_{}(param: &str) -> Result<(), Error>", i),
            is_async: i % 3 == 0,
            visibility: if i % 4 == 0 { "public".to_string() } else { "private".to_string() },
            parameter_count: (i % 6) as u32,
        })
        .collect()
}

fn generate_mock_structs(count: usize) -> Vec<MockStruct> {
    (0..count)
        .map(|i| MockStruct {
            id: i as u64,
            name: match i % 8 {
                0 => format!("User_{}", i),
                1 => format!("Request_{}", i),
                2 => format!("Response_{}", i),
                3 => format!("Config_{}", i),
                4 => format!("Database_{}", i),
                5 => format!("Handler_{}", i),
                6 => format!("Service_{}", i),
                _ => format!("Model_{}", i),
            },
            file_path: format!("src/models/model_{}.rs", i % 15),
            field_count: (i % 10) as u32,
            visibility: if i % 3 == 0 { "public".to_string() } else { "private".to_string() },
        })
        .collect()
}

fn benchmark_exact_search(c: &mut Criterion) {
    let functions = generate_mock_functions(10000);
    
    let mut group = c.benchmark_group("exact_search");
    
    for pattern in ["authenticate", "process", "user", "data"].iter() {
        group.bench_with_input(
            BenchmarkId::new("pattern", pattern),
            pattern,
            |b, pattern| {
                b.iter(|| exact_search(black_box(&functions), black_box(pattern)))
            },
        );
    }
    
    group.finish();
}

fn benchmark_regex_search(c: &mut Criterion) {
    let functions = generate_mock_functions(10000);
    
    let mut group = c.benchmark_group("regex_search");
    
    for pattern in ["^authenticate.*", ".*process.*", "user_[0-9]+", ".*_data$"].iter() {
        group.bench_with_input(
            BenchmarkId::new("pattern", pattern),
            pattern,
            |b, pattern| {
                b.iter(|| regex_search(black_box(&functions), black_box(pattern)))
            },
        );
    }
    
    group.finish();
}

fn benchmark_fuzzy_search(c: &mut Criterion) {
    let functions = generate_mock_functions(10000);
    
    let mut group = c.benchmark_group("fuzzy_search");
    
    for pattern in ["auth", "proc", "usr", "dat"].iter() {
        group.bench_with_input(
            BenchmarkId::new("pattern", pattern),
            pattern,
            |b, pattern| {
                b.iter(|| fuzzy_search(black_box(&functions), black_box(pattern)))
            },
        );
    }
    
    group.finish();
}

fn benchmark_filtered_search(c: &mut Criterion) {
    let functions = generate_mock_functions(10000);
    
    c.bench_function("filtered_search_async", |b| {
        b.iter(|| {
            filtered_search(
                black_box(&functions),
                black_box("process"),
                black_box(Some(true)),
                black_box(None),
            )
        })
    });
    
    c.bench_function("filtered_search_params", |b| {
        b.iter(|| {
            filtered_search(
                black_box(&functions),
                black_box("user"),
                black_box(None),
                black_box(Some(2)),
            )
        })
    });
    
    c.bench_function("filtered_search_combined", |b| {
        b.iter(|| {
            filtered_search(
                black_box(&functions),
                black_box("handle"),
                black_box(Some(false)),
                black_box(Some(1)),
            )
        })
    });
}

fn benchmark_large_dataset(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_dataset");
    
    for size in [1000, 5000, 10000, 50000].iter() {
        let functions = generate_mock_functions(*size);
        
        group.bench_with_input(
            BenchmarkId::new("exact_search", size),
            &functions,
            |b, functions| {
                b.iter(|| exact_search(black_box(functions), black_box("authenticate")))
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("fuzzy_search", size),
            &functions,
            |b, functions| {
                b.iter(|| fuzzy_search(black_box(functions), black_box("auth")))
            },
        );
    }
    
    group.finish();
}

fn benchmark_struct_search(c: &mut Criterion) {
    let structs = generate_mock_structs(5000);
    
    c.bench_function("struct_exact_search", |b| {
        b.iter(|| {
            structs
                .iter()
                .filter(|s| s.name.contains(black_box("User")))
                .collect::<Vec<_>>()
        })
    });
    
    c.bench_function("struct_filtered_search", |b| {
        b.iter(|| {
            structs
                .iter()
                .filter(|s| s.name.contains(black_box("Model")))
                .filter(|s| s.field_count >= black_box(5))
                .collect::<Vec<_>>()
        })
    });
}

criterion_group!(
    benches,
    benchmark_exact_search,
    benchmark_regex_search,
    benchmark_fuzzy_search,
    benchmark_filtered_search,
    benchmark_large_dataset,
    benchmark_struct_search
);
criterion_main!(benches); 