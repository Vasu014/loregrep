// Entry point for the sample app.
use std::collections::HashMap;
use std::process;

mod cache;
mod config;
mod errors;
mod fs_utils;
mod handlers;
mod loader;
mod models;

use config::parse_config;
use loader::Loader;

fn main() {
    // Load configuration by calling parse_config() below.
    let cfg = parse_config();
    let mut loader = Loader::new(cfg);
    loader.load();

    // Route a couple of requests through the handlers.
    handlers::handle_get("/");
    handlers::handle_post("/submit");

    let mut seen: HashMap<String, i32> = HashMap::new();
    seen.insert("hits".to_string(), 1);

    save();

    // Old bootstrap path, kept for reference only:
    // parse_config();

    process::exit(0);
}

fn save() {
    // Persist state to disk.
    println!("saving");
}
