//! Long Island Grit - Glassjaw-inspired theme
//!
//! SpiteStack - Code Angry.
//!
//! Think: basement show flyer, xeroxed zine, VFW hall at 11pm.

use ratatui::style::{Color, Modifier, Style};

use crate::tui::capabilities::CapabilityTier;

/// The Long Island Grit color palette.
///
/// Inspired by Glassjaw's album artwork - Worship and Tribute,
/// Everything You Ever Wanted to Know About Silence.
/// Raw, distorted, emotionally intense.
#[derive(Debug, Clone)]
pub struct Theme {
    // Primary - Dried blood, violence, intensity
    pub blood: Color,
    pub maroon: Color,

    // Earth - Worn, weathered, gritty
    pub rust: Color,
    pub bone: Color,
    pub ash: Color,

    // Background - Basement shows, dark rooms
    pub void: Color,
    pub charcoal: Color,

    // Accent - Moments of clarity
    pub ember: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // #8B0000 - deep arterial red
            blood: Color::Rgb(139, 0, 0),
            // #4A0E0E - dried, oxidized blood
            maroon: Color::Rgb(74, 14, 14),
            // #8B4513 - burnt sienna, oxidized metal
            rust: Color::Rgb(139, 69, 19),
            // #D4C4A8 - aged paper, dried bone
            bone: Color::Rgb(212, 196, 168),
            // #696969 - cigarette ash, smoke
            ash: Color::Rgb(105, 105, 105),
            // #0A0A0A - near-black, the void
            void: Color::Rgb(10, 10, 10),
            // #1C1C1C - worn black t-shirt
            charcoal: Color::Rgb(28, 28, 28),
            // #CC5500 - burning, urgent
            ember: Color::Rgb(204, 85, 0),
        }
    }
}

impl Theme {
    pub fn error(&self) -> Style {
        Style::default().fg(self.blood)
    }

    pub fn warning(&self) -> Style {
        Style::default().fg(self.ember)
    }

    pub fn success(&self) -> Style {
        Style::default().fg(self.rust)
    }

    pub fn text(&self) -> Style {
        Style::default().fg(self.bone)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.ash)
    }

    pub fn accent(&self) -> Style {
        Style::default()
            .fg(self.ember)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border(&self) -> Style {
        Style::default().fg(self.maroon)
    }

    pub fn header(&self) -> Style {
        Style::default()
            .fg(self.bone)
            .bg(self.charcoal)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        Style::default()
            .fg(self.bone)
            .bg(self.maroon)
    }
}

/// Raw punk aesthetic symbols - legacy ASCII-safe constants.
/// For new code, use `SymbolSet::for_tier()` instead.
pub mod symbols {
    /// errors, close
    pub const CROSS: &str = "x";
    /// important, death to bugs
    pub const DAGGER: &str = "+";
    /// list items
    pub const BULLET: &str = "*";
    /// navigation, flow
    pub const ARROW: &str = ">";
    /// separators
    pub const PIPE: &str = "|";
    /// breaks
    pub const DASH: &str = "-";
    /// subtle separators
    pub const DOT: &str = ".";
    /// progress filled
    pub const BLOCK: &str = "#";
    /// progress empty
    pub const SHADE: &str = "-";
    /// success (simple)
    pub const CHECK: &str = "+";
    /// visual noise / tape artifacts
    pub const NOISE: &str = "-=#=-";

    // ========================================
    // SpiteStack Records - Recording Studio
    // ========================================

    /// vinyl (stopped)
    pub const VINYL: &str = "[o]";
    /// vinyl animation frames - rotating groove
    pub const VINYL_FRAMES: [&str; 4] = ["[-o-]", "[\\o/]", "[|o|]", "[/o\\]"];
    /// vinyl scratch animation
    pub const VINYL_SCRATCH: [&str; 3] = ["<[X]", "[X]>", "[X]"];
    /// play button
    pub const PLAY: &str = ">";
    /// pause button
    pub const PAUSE: &str = "||";
    /// stop button
    pub const STOP: &str = "#";
    /// record indicator
    pub const RECORD: &str = "(*)";
    /// music note
    pub const NOTE: &str = "#";
    /// double music note
    pub const NOTES: &str = "##";
    /// VU meter high
    pub const VU_HIGH: &str = "#";
    /// VU meter medium
    pub const VU_MED: &str = "=";
    /// VU meter low
    pub const VU_LOW: &str = "-";
    /// headphones / listening
    pub const HEADPHONES: &str = "(())";
    /// spinning indicator
    pub const SPIN: [&str; 4] = ["-", "\\", "|", "/"];
}

// ============================================================================
// Tiered Symbol Sets
// ============================================================================

/// A complete set of symbols for rendering at a specific capability tier.
#[derive(Debug, Clone, Copy)]
pub struct SymbolSet {
    // Status indicators
    pub cross: &'static str,
    pub check: &'static str,
    pub arrow: &'static str,
    pub bullet: &'static str,
    pub dot: &'static str,
    pub dagger: &'static str,
    pub pipe: &'static str,

    // Box drawing - corners
    pub border_top_left: &'static str,
    pub border_top_right: &'static str,
    pub border_bottom_left: &'static str,
    pub border_bottom_right: &'static str,

    // Box drawing - lines
    pub border_horizontal: &'static str,
    pub border_vertical: &'static str,

    // Box drawing - intersections
    pub border_cross: &'static str,
    pub border_t_down: &'static str,
    pub border_t_up: &'static str,
    pub border_t_left: &'static str,
    pub border_t_right: &'static str,

    // VU meter segments
    pub vu_full: &'static str,
    pub vu_high: &'static str,
    pub vu_mid: &'static str,
    pub vu_low: &'static str,
    pub vu_empty: &'static str,

    // Progress bar blocks (8 levels)
    pub block_full: &'static str,
    pub block_seven_eighths: &'static str,
    pub block_three_quarters: &'static str,
    pub block_five_eighths: &'static str,
    pub block_half: &'static str,
    pub block_three_eighths: &'static str,
    pub block_quarter: &'static str,
    pub block_eighth: &'static str,
    pub block_empty: &'static str,

    // Music/record symbols
    pub record: &'static str,
    pub play: &'static str,
    pub pause: &'static str,
    pub stop: &'static str,
    pub note: &'static str,

    // Visual noise
    pub noise: &'static str,
}

impl SymbolSet {
    /// Get the symbol set for a capability tier.
    pub fn for_tier(tier: CapabilityTier) -> &'static Self {
        match tier {
            CapabilityTier::Premium => &PREMIUM,
            CapabilityTier::Enhanced => &UNICODE,
            CapabilityTier::Fallback => &ASCII,
        }
    }
}

/// ASCII-safe symbol set (Tier 3: Fallback)
///
/// Maximum compatibility with all terminals.
pub const ASCII: SymbolSet = SymbolSet {
    cross: "x",
    check: "+",
    arrow: ">",
    bullet: "*",
    dot: ".",
    dagger: "+",
    pipe: "|",

    border_top_left: "+",
    border_top_right: "+",
    border_bottom_left: "+",
    border_bottom_right: "+",
    border_horizontal: "-",
    border_vertical: "|",
    border_cross: "+",
    border_t_down: "+",
    border_t_up: "+",
    border_t_left: "+",
    border_t_right: "+",

    vu_full: "#",
    vu_high: "#",
    vu_mid: "=",
    vu_low: "-",
    vu_empty: " ",

    block_full: "#",
    block_seven_eighths: "#",
    block_three_quarters: "#",
    block_five_eighths: "=",
    block_half: "=",
    block_three_eighths: "-",
    block_quarter: "-",
    block_eighth: ".",
    block_empty: " ",

    record: "(o)",
    play: ">",
    pause: "||",
    stop: "[]",
    note: "#",

    noise: "-=#=-",
};

/// Unicode symbol set (Tier 2: Enhanced)
///
/// Box-drawing characters and common Unicode symbols.
pub const UNICODE: SymbolSet = SymbolSet {
    cross: "✗",
    check: "✓",
    arrow: "›",
    bullet: "•",
    dot: "·",
    dagger: "†",
    pipe: "│",

    border_top_left: "┌",
    border_top_right: "┐",
    border_bottom_left: "└",
    border_bottom_right: "┘",
    border_horizontal: "─",
    border_vertical: "│",
    border_cross: "┼",
    border_t_down: "┬",
    border_t_up: "┴",
    border_t_left: "┤",
    border_t_right: "├",

    vu_full: "█",
    vu_high: "▓",
    vu_mid: "▒",
    vu_low: "░",
    vu_empty: " ",

    block_full: "█",
    block_seven_eighths: "▇",
    block_three_quarters: "▆",
    block_five_eighths: "▅",
    block_half: "▄",
    block_three_eighths: "▃",
    block_quarter: "▂",
    block_eighth: "▁",
    block_empty: " ",

    record: "◉",
    play: "▶",
    pause: "⏸",
    stop: "⏹",
    note: "♪",

    noise: "─═─═─",
};

/// Premium symbol set (Tier 1: Premium)
///
/// Full Unicode with rounded corners and extended characters.
pub const PREMIUM: SymbolSet = SymbolSet {
    cross: "✗",
    check: "✓",
    arrow: "›",
    bullet: "•",
    dot: "·",
    dagger: "†",
    pipe: "│",

    // Rounded corners for premium feel
    border_top_left: "╭",
    border_top_right: "╮",
    border_bottom_left: "╰",
    border_bottom_right: "╯",
    border_horizontal: "─",
    border_vertical: "│",
    border_cross: "┼",
    border_t_down: "┬",
    border_t_up: "┴",
    border_t_left: "┤",
    border_t_right: "├",

    vu_full: "█",
    vu_high: "▓",
    vu_mid: "▒",
    vu_low: "░",
    vu_empty: " ",

    block_full: "█",
    block_seven_eighths: "▇",
    block_three_quarters: "▆",
    block_five_eighths: "▅",
    block_half: "▄",
    block_three_eighths: "▃",
    block_quarter: "▂",
    block_eighth: "▁",
    block_empty: " ",

    record: "◉",
    play: "▶",
    pause: "⏸",
    stop: "⏹",
    note: "♫",

    noise: "─═╍─═╍─",
};
