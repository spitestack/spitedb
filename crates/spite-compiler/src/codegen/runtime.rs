//! Runtime TypeScript modules embedded at compile time.
//!
//! These modules are written in TypeScript in the `runtime/` directory
//! and embedded into the compiler binary using `include_str!`.

/// Auth module - JWT-based authentication for Bun handlers.
pub const AUTH: &str = include_str!("../../runtime/auth.ts");

/// Returns all runtime modules as (filename, content) pairs.
pub fn get_runtime_modules() -> Vec<(&'static str, &'static str)> {
    vec![
        ("runtime/auth.ts", AUTH),
    ]
}
