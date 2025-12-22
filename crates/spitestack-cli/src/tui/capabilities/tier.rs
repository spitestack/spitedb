//! Terminal capability tiers.
//!
//! SpiteStack - Code Angry.
//!
//! Three tiers of terminal rendering:
//! - Premium: Ghostty, Kitty (Braille, TrueColor, sync rendering)
//! - Enhanced: iTerm2, WezTerm (Unicode, 256 colors)
//! - Fallback: Terminal.app, basic xterm (ASCII only)

/// Terminal capability tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum CapabilityTier {
    /// Tier 3: ASCII fallback (Terminal.app, basic xterm, SSH)
    #[default]
    Fallback = 0,
    /// Tier 2: 256 colors, basic Unicode (iTerm2, WezTerm, Alacritty)
    Enhanced = 1,
    /// Tier 1: Full 24-bit, Braille, sync rendering (Ghostty, Kitty)
    Premium = 2,
}

impl CapabilityTier {
    /// Check if this tier supports TrueColor (24-bit).
    pub fn supports_true_color(&self) -> bool {
        matches!(self, Self::Premium | Self::Enhanced)
    }

    /// Check if this tier supports Braille characters.
    pub fn supports_braille(&self) -> bool {
        matches!(self, Self::Premium)
    }

    /// Check if this tier supports synchronized rendering.
    pub fn supports_sync_rendering(&self) -> bool {
        matches!(self, Self::Premium)
    }

    /// Check if this tier supports Unicode box-drawing characters.
    pub fn supports_unicode(&self) -> bool {
        matches!(self, Self::Premium | Self::Enhanced)
    }

    /// Get the recommended frame duration for this tier.
    pub fn frame_duration_ms(&self) -> u64 {
        match self {
            Self::Premium => 16,   // ~60fps
            Self::Enhanced => 33,  // ~30fps
            Self::Fallback => 50,  // ~20fps
        }
    }
}

impl std::fmt::Display for CapabilityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Premium => write!(f, "premium"),
            Self::Enhanced => write!(f, "enhanced"),
            Self::Fallback => write!(f, "fallback"),
        }
    }
}

/// Unicode support level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeLevel {
    /// ASCII only (0x20-0x7E)
    Ascii,
    /// Basic Unicode: box-drawing, common symbols
    Basic,
    /// Full Unicode: Braille, all blocks, special characters
    Full,
}

/// Full terminal capabilities.
#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    /// The detected or overridden capability tier.
    pub tier: CapabilityTier,
    /// Whether TrueColor (24-bit) is supported.
    pub true_color: bool,
    /// Whether 256 colors are supported.
    pub colors_256: bool,
    /// The Unicode support level.
    pub unicode_level: UnicodeLevel,
    /// Whether synchronized output is supported (DCS).
    pub synchronized_output: bool,
    /// The detected terminal name (if any).
    pub terminal_name: Option<String>,
}

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self {
            tier: CapabilityTier::Fallback,
            true_color: false,
            colors_256: false,
            unicode_level: UnicodeLevel::Ascii,
            synchronized_output: false,
            terminal_name: None,
        }
    }
}

impl TerminalCapabilities {
    /// Create capabilities for a specific tier.
    pub fn for_tier(tier: CapabilityTier) -> Self {
        match tier {
            CapabilityTier::Premium => Self {
                tier,
                true_color: true,
                colors_256: true,
                unicode_level: UnicodeLevel::Full,
                synchronized_output: true,
                terminal_name: None,
            },
            CapabilityTier::Enhanced => Self {
                tier,
                true_color: true,
                colors_256: true,
                unicode_level: UnicodeLevel::Basic,
                synchronized_output: false,
                terminal_name: None,
            },
            CapabilityTier::Fallback => Self::default(),
        }
    }
}
