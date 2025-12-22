//! Runtime TypeScript modules embedded at compile time.
//!
//! These modules are written in TypeScript in the `runtime/` directory
//! and embedded into the compiler binary using `include_str!`.

/// Auth module - JWT-based authentication for Bun handlers.
pub const AUTH: &str = include_str!("../../runtime/auth.ts");
/// Telemetry helper module for auto-instrumentation.
pub const TELEMETRY: &str = include_str!("../../runtime/telemetry.ts");

/// Returns all runtime modules as (filename, content) pairs.
pub fn get_runtime_modules() -> Vec<(&'static str, &'static str)> {
    vec![
        ("runtime/auth.ts", AUTH),
        ("runtime/telemetry.ts", TELEMETRY),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_runtime_emits_without_flush() {
        assert!(TELEMETRY.contains("export function emitTelemetry"));
        assert!(TELEMETRY.contains("writeBatch"));
        assert!(!TELEMETRY.contains("flushTelemetry"));
    }
}
