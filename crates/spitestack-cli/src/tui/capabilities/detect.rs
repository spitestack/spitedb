//! Terminal capability detection.
//!
//! SpiteStack - Code Angry.
//!
//! Detects terminal capabilities via environment variables.
//! Supports manual override via CLI flag or SPITESTACK_GRAPHICS env var.

use super::tier::{CapabilityTier, TerminalCapabilities, UnicodeLevel};

/// Detect terminal capabilities with optional override.
///
/// Priority order:
/// 1. CLI flag override (if provided)
/// 2. SPITESTACK_GRAPHICS environment variable
/// 3. Auto-detection from terminal environment
pub fn detect_capabilities(cli_override: Option<&str>) -> TerminalCapabilities {
    let tier = detect_tier(cli_override);
    let terminal_name = detect_terminal_name();

    TerminalCapabilities {
        tier,
        true_color: detect_true_color() || tier.supports_true_color(),
        colors_256: detect_256_colors() || tier.supports_true_color(),
        unicode_level: unicode_level_for_tier(tier),
        synchronized_output: tier.supports_sync_rendering(),
        terminal_name,
    }
}

/// Detect the capability tier with optional override.
pub fn detect_tier(cli_override: Option<&str>) -> CapabilityTier {
    // 1. CLI flag takes priority
    if let Some(tier_str) = cli_override {
        if let Some(tier) = parse_tier(tier_str) {
            return tier;
        }
    }

    // 2. Environment variable override
    if let Ok(tier_str) = std::env::var("SPITESTACK_GRAPHICS") {
        if let Some(tier) = parse_tier(&tier_str) {
            return tier;
        }
    }

    // 3. Auto-detect from terminal
    auto_detect_tier()
}

/// Parse a tier string into a CapabilityTier.
fn parse_tier(s: &str) -> Option<CapabilityTier> {
    match s.to_lowercase().as_str() {
        "premium" | "full" | "high" | "1" => Some(CapabilityTier::Premium),
        "enhanced" | "unicode" | "medium" | "2" => Some(CapabilityTier::Enhanced),
        "ascii" | "fallback" | "low" | "basic" | "3" => Some(CapabilityTier::Fallback),
        "auto" | "" => None, // Fall through to auto-detection
        _ => None,
    }
}

/// Auto-detect the capability tier from the terminal environment.
fn auto_detect_tier() -> CapabilityTier {
    let terminal_name = detect_terminal_name();

    match terminal_name.as_deref().map(|s| s.to_lowercase()) {
        // Tier 1: Premium terminals with full support
        Some(ref n) if n.contains("ghostty") => CapabilityTier::Premium,
        Some(ref n) if n.contains("kitty") => CapabilityTier::Premium,

        // Tier 2: Enhanced terminals
        Some(ref n) if n.contains("iterm") => CapabilityTier::Enhanced,
        Some(ref n) if n.contains("wezterm") => CapabilityTier::Enhanced,
        Some(ref n) if n.contains("alacritty") => CapabilityTier::Enhanced,
        Some(ref n) if n.contains("gnome-terminal") => CapabilityTier::Enhanced,
        Some(ref n) if n.contains("konsole") => CapabilityTier::Enhanced,
        Some(ref n) if n.contains("hyper") => CapabilityTier::Enhanced,

        // Tier 3: Fallback
        _ => detect_tier_from_term_env(),
    }
}

/// Detect tier from TERM and COLORTERM environment variables.
fn detect_tier_from_term_env() -> CapabilityTier {
    // Check COLORTERM for truecolor support
    if detect_true_color() {
        return CapabilityTier::Enhanced;
    }

    // Check TERM for 256 color support
    if let Ok(term) = std::env::var("TERM") {
        let term_lower = term.to_lowercase();
        if term_lower.contains("256color") || term_lower.contains("truecolor") {
            return CapabilityTier::Enhanced;
        }
        if term_lower.contains("xterm") && !term_lower.contains("xterm-color") {
            return CapabilityTier::Enhanced;
        }
    }

    CapabilityTier::Fallback
}

/// Detect the terminal name from environment variables.
fn detect_terminal_name() -> Option<String> {
    // Priority order for detection
    std::env::var("TERM_PROGRAM")
        .ok()
        .or_else(|| std::env::var("LC_TERMINAL").ok())
        .or_else(|| std::env::var("TERMINAL_EMULATOR").ok())
        .or_else(|| {
            // Try to extract from TERM if it's specific
            std::env::var("TERM").ok().and_then(|t| {
                if t.starts_with("xterm") || t.starts_with("screen") || t.starts_with("tmux") {
                    None
                } else {
                    Some(t)
                }
            })
        })
}

/// Detect TrueColor (24-bit) support.
fn detect_true_color() -> bool {
    std::env::var("COLORTERM")
        .map(|v| {
            let v_lower = v.to_lowercase();
            v_lower.contains("truecolor") || v_lower.contains("24bit")
        })
        .unwrap_or(false)
}

/// Detect 256 color support.
fn detect_256_colors() -> bool {
    if detect_true_color() {
        return true;
    }

    std::env::var("TERM")
        .map(|v| v.contains("256color"))
        .unwrap_or(false)
}

/// Get the Unicode level for a capability tier.
fn unicode_level_for_tier(tier: CapabilityTier) -> UnicodeLevel {
    match tier {
        CapabilityTier::Premium => UnicodeLevel::Full,
        CapabilityTier::Enhanced => UnicodeLevel::Basic,
        CapabilityTier::Fallback => UnicodeLevel::Ascii,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tier() {
        assert_eq!(parse_tier("premium"), Some(CapabilityTier::Premium));
        assert_eq!(parse_tier("PREMIUM"), Some(CapabilityTier::Premium));
        assert_eq!(parse_tier("full"), Some(CapabilityTier::Premium));
        assert_eq!(parse_tier("high"), Some(CapabilityTier::Premium));

        assert_eq!(parse_tier("enhanced"), Some(CapabilityTier::Enhanced));
        assert_eq!(parse_tier("unicode"), Some(CapabilityTier::Enhanced));
        assert_eq!(parse_tier("medium"), Some(CapabilityTier::Enhanced));

        assert_eq!(parse_tier("ascii"), Some(CapabilityTier::Fallback));
        assert_eq!(parse_tier("fallback"), Some(CapabilityTier::Fallback));
        assert_eq!(parse_tier("low"), Some(CapabilityTier::Fallback));

        assert_eq!(parse_tier("auto"), None);
        assert_eq!(parse_tier("invalid"), None);
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(CapabilityTier::Premium.to_string(), "premium");
        assert_eq!(CapabilityTier::Enhanced.to_string(), "enhanced");
        assert_eq!(CapabilityTier::Fallback.to_string(), "fallback");
    }

    #[test]
    fn test_tier_capabilities() {
        assert!(CapabilityTier::Premium.supports_braille());
        assert!(!CapabilityTier::Enhanced.supports_braille());
        assert!(!CapabilityTier::Fallback.supports_braille());

        assert!(CapabilityTier::Premium.supports_unicode());
        assert!(CapabilityTier::Enhanced.supports_unicode());
        assert!(!CapabilityTier::Fallback.supports_unicode());
    }
}
