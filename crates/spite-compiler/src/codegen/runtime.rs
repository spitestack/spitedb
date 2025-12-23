//! Runtime TypeScript modules embedded at compile time.
//!
//! These modules are written in TypeScript in the `runtime/` directory
//! and embedded into the compiler binary using `include_str!`.

/// Auth module - JWT-based authentication for Bun handlers.
pub const AUTH: &str = include_str!("../../runtime/auth.ts");
/// Telemetry helper module for auto-instrumentation.
pub const TELEMETRY: &str = include_str!("../../runtime/telemetry.ts");
/// Client SDK base module.
pub const CLIENT: &str = include_str!("../../runtime/client.ts");
/// Identity/Auth system module.
pub const IDENTITY: &str = include_str!("../../runtime/identity.ts");
/// Tenant management module.
pub const TENANT: &str = include_str!("../../runtime/tenant.ts");
/// Email abstraction module.
pub const EMAIL: &str = include_str!("../../runtime/email.ts");
/// Zero-dependency TOTP module.
pub const TOTP: &str = include_str!("../../runtime/totp.ts");
/// SMS abstraction module.
pub const SMS: &str = include_str!("../../runtime/sms.ts");
/// Social auth module.
pub const SOCIAL: &str = include_str!("../../runtime/social.ts");
/// App registration module for access configuration.
pub const APP: &str = include_str!("../../runtime/app.ts");
/// Admin dashboard API handlers.
pub const ADMIN: &str = include_str!("../../runtime/admin.ts");
/// Admin dashboard WebSocket handler for real-time streaming.
pub const ADMIN_WS: &str = include_str!("../../runtime/admin-ws.ts");
/// Security headers module.
pub const SECURITY_HEADERS: &str = include_str!("../../runtime/security-headers.ts");
/// Rate limiting module.
pub const RATE_LIMIT: &str = include_str!("../../runtime/rate-limit.ts");
/// Password policy module.
pub const PASSWORD_POLICY: &str = include_str!("../../runtime/password-policy.ts");

/// Returns all runtime modules as (filename, content) pairs.
pub fn get_runtime_modules() -> Vec<(&'static str, &'static str)> {
    vec![
        ("runtime/auth.ts", AUTH),
        ("runtime/telemetry.ts", TELEMETRY),
        ("runtime/client.ts", CLIENT),
        ("runtime/identity.ts", IDENTITY),
        ("runtime/tenant.ts", TENANT),
        ("runtime/email.ts", EMAIL),
        ("runtime/totp.ts", TOTP),
        ("runtime/sms.ts", SMS),
        ("runtime/social.ts", SOCIAL),
        ("runtime/app.ts", APP),
        ("runtime/admin.ts", ADMIN),
        ("runtime/admin-ws.ts", ADMIN_WS),
        ("runtime/security-headers.ts", SECURITY_HEADERS),
        ("runtime/rate-limit.ts", RATE_LIMIT),
        ("runtime/password-policy.ts", PASSWORD_POLICY),
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
