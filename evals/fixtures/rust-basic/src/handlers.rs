//! HTTP-ish request handlers.

pub fn handle_get(path: &str) {
    println!("GET {}", path);
}

pub fn handle_post(path: &str) {
    println!("POST {}", path);
}

pub fn handle_delete(path: &str) {
    println!("DELETE {}", path);
}

// Not a handler; its name must NOT match the ^handle_.* pattern.
pub fn dispatch(path: &str) {
    handle_get(path);
}
