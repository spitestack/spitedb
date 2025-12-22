//! Vinyl record widget.
//!
//! SpiteStack - Code Angry.
//!
//! Spinning vinyl with animation frames.
//! - Premium tier: High-resolution Braille graphics
//! - Enhanced tier: Unicode symbols
//! - Fallback tier: ASCII art
//!
//! - Mini vinyl: status bar corner (always visible)
//! - Large vinyl: full-screen music mode

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::VinylState;
use crate::tui::capabilities::CapabilityTier;
use crate::tui::render::animation::AnimationConfig;
use crate::tui::render::braille::{generate_vinyl, generate_vinyl_static};

use crate::tui::theme::{symbols, SymbolSet, Theme};

/// Large vinyl ASCII art for full-screen music mode.
/// Each frame shows the grooves at different positions to simulate rotation.
/// All lines MUST be exactly 34 characters wide for proper alignment.
const VINYL_LARGE: [[&str; 11]; 4] = [
    // Frame 0 - grooves horizontal
    [
        "        .-------------.          ",
        "      .'- - - - - - - -'.        ",
        "    .' - - - - - - - - - '.      ",
        "   / - - .--------. - - - \\     ",
        "  | - - |          | - - - |     ",
        "  |- - -|    [-]   |- - - -|     ",
        "  | - - |          | - - - |     ",
        "   \\ - - '--------' - - - /     ",
        "    '. - - - - - - - - - .'      ",
        "      '.- - - - - - - -.'        ",
        "        '-------------'          ",
    ],
    // Frame 1 - grooves diagonal /
    [
        "        .-------------.          ",
        "      .'/ / / / / / / /'.        ",
        "    .' / / / / / / / / / '.      ",
        "   / / / .--------. / / / \\     ",
        "  | / / |          | / / / |     ",
        "  |/ / /|    [\\]   |/ / / /|     ",
        "  | / / |          | / / / |     ",
        "   \\ / / '--------' / / / /     ",
        "    '. / / / / / / / / / .'      ",
        "      './ / / / / / / /.'        ",
        "        '-------------'          ",
    ],
    // Frame 2 - grooves vertical
    [
        "        .-------------.          ",
        "      .'| | | | | | | |'.        ",
        "    .' | | | | | | | | | '.      ",
        "   / | | .--------. | | | \\     ",
        "  | | | |          | | | | |     ",
        "  || | ||    [|]   || | | ||     ",
        "  | | | |          | | | | |     ",
        "   \\ | | '--------' | | | /     ",
        "    '. | | | | | | | | | .'      ",
        "      '.| | | | | | | |.'        ",
        "        '-------------'          ",
    ],
    // Frame 3 - grooves diagonal \
    [
        "        .-------------.          ",
        "      .'.\\ \\ \\ \\ \\ \\ \\ \\'.        ",
        "    .' \\ \\ \\ \\ \\ \\ \\ \\ \\ '.      ",
        "   / \\ \\ .--------. \\ \\ \\ \\     ",
        "  | \\ \\ |          | \\ \\ \\ |     ",
        "  |\\ \\ \\|    [/]   |\\ \\ \\ \\|     ",
        "  | \\ \\ |          | \\ \\ \\ |     ",
        "   \\ \\ \\ '--------' \\ \\ \\ /     ",
        "    '. \\ \\ \\ \\ \\ \\ \\ \\ \\ .'      ",
        "      '.\\ \\ \\ \\ \\ \\ \\ \\.'        ",
        "        '-------------'          ",
    ],
];

/// Large vinyl with "SPITESTACK RECORDS" label (for scratch/stopped state).
/// All lines MUST be exactly 34 characters wide.
const VINYL_LABEL: [&str; 11] = [
    "        .-------------.          ",
    "      .'               '.        ",
    "    .'                   '.      ",
    "   /     .---------.      \\     ",
    "  |     | SPITESTACK |     |     ",
    "  |     |  RECORDS   |     |     ",
    "  |     |    [X]     |     |     ",
    "   \\     '---------'      /     ",
    "    '.                   .'      ",
    "      '.               .'        ",
    "        '-------------'          ",
];

// ============================================================================
// Tiered Vinyl Rendering
// ============================================================================

/// Draw the large vinyl with tier-appropriate rendering.
pub fn draw_large_vinyl_tiered(
    f: &mut Frame,
    vinyl: &VinylState,
    theme: &Theme,
    tier: CapabilityTier,
    area: Rect,
) {
    match tier {
        CapabilityTier::Premium => draw_braille_vinyl(f, vinyl, theme, area),
        CapabilityTier::Enhanced => draw_unicode_vinyl(f, vinyl, theme, area),
        CapabilityTier::Fallback => draw_large_vinyl(f, vinyl, theme, area),
    }
}

/// Draw the mini vinyl with tier-appropriate symbols.
pub fn draw_mini_vinyl_tiered(
    f: &mut Frame,
    vinyl: &VinylState,
    theme: &Theme,
    tier: CapabilityTier,
    area: Rect,
) {
    let syms = SymbolSet::for_tier(tier);

    let frame_char = if vinyl.scratching {
        symbols::VINYL_SCRATCH[vinyl.scratch_frames % 3]
    } else if vinyl.spinning {
        // Use Unicode record symbol for Enhanced/Premium
        if tier.supports_unicode() {
            syms.record
        } else {
            symbols::VINYL_FRAMES[vinyl.ascii_frame()]
        }
    } else {
        if tier.supports_unicode() {
            syms.record
        } else {
            symbols::VINYL
        }
    };

    let style = if vinyl.scratching {
        theme.error()
    } else if vinyl.spinning {
        theme.accent()
    } else {
        theme.muted()
    };

    let vinyl_widget = Paragraph::new(frame_char)
        .style(style)
        .alignment(Alignment::Center);

    f.render_widget(vinyl_widget, area);
}

// ============================================================================
// Premium Tier: Braille Graphics
// ============================================================================

/// Draw high-resolution Braille vinyl (Premium tier).
fn draw_braille_vinyl(f: &mut Frame, vinyl: &VinylState, theme: &Theme, area: Rect) {
    // Calculate canvas size based on area
    // Each Braille character is 2 pixels wide and 4 pixels tall
    // Make canvas wider to accommodate tonearm on the right
    let char_width = (area.width as usize).min(45);
    let char_height = (area.height as usize).min(20);

    // Get the tier-appropriate rotation speed for realistic 33⅓ RPM
    let config = AnimationConfig::for_tier(CapabilityTier::Premium);
    let rotation_per_frame = config.vinyl_rotation_per_frame();

    // Generate the vinyl graphics with tonearm
    let lines = if vinyl.scratching {
        // Stopped/scratched - static vinyl with lifted tonearm
        generate_vinyl_static(char_width, char_height, true)
    } else {
        // Spinning - animated vinyl with tonearm on record
        generate_vinyl(
            vinyl.frame,
            rotation_per_frame,
            char_width,
            char_height,
            true,  // include_tonearm
            vinyl.spinning,
        )
    };

    // Style with uniform vinyl color (no vertical gradient which creates fadeout effect)
    let vinyl_color = Color::Rgb(55, 55, 55); // Dark charcoal vinyl color
    let styled_lines: Vec<Line> = lines
        .iter()
        .map(|line| {
            let color = if vinyl.scratching {
                // Red tint when scratching
                theme.blood
            } else {
                vinyl_color
            };

            Line::from(Span::styled(line.clone(), Style::default().fg(color)))
        })
        .collect();

    let widget = Paragraph::new(styled_lines).alignment(Alignment::Center);
    f.render_widget(widget, area);
}

// ============================================================================
// Enhanced Tier: Unicode Symbols
// ============================================================================

/// Draw Unicode vinyl (Enhanced tier).
///
/// Uses box-drawing characters for cleaner lines than ASCII.
fn draw_unicode_vinyl(f: &mut Frame, vinyl: &VinylState, theme: &Theme, area: Rect) {
    // Unicode vinyl art with cleaner characters
    let frame_art = if vinyl.scratching {
        VINYL_UNICODE_LABEL.to_vec()
    } else {
        VINYL_UNICODE[vinyl.ascii_frame()].to_vec()
    };

    let style = if vinyl.scratching {
        theme.error()
    } else {
        theme.text()
    };

    let lines: Vec<Line> = frame_art
        .iter()
        .map(|line| Line::from(Span::styled(*line, style)))
        .collect();

    let vinyl_widget = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(vinyl_widget, area);
}

/// Unicode vinyl frames with box-drawing characters.
const VINYL_UNICODE: [[&str; 11]; 4] = [
    // Frame 0 - grooves horizontal
    [
        "        ╭─────────────╮          ",
        "      ╭─┼─┼─┼─┼─┼─┼─┼─╮        ",
        "    ╭─┼─┼─┼─┼─┼─┼─┼─┼─┼─╮      ",
        "   ╭─┼─╭─────────╮─┼─┼─╮     ",
        "  │─┼─│           │─┼─┼─│     ",
        "  │─┼─│    ◉     │─┼─┼─│     ",
        "  │─┼─│           │─┼─┼─│     ",
        "   ╰─┼─╰─────────╯─┼─┼─╯     ",
        "    ╰─┼─┼─┼─┼─┼─┼─┼─┼─┼─╯      ",
        "      ╰─┼─┼─┼─┼─┼─┼─┼─╯        ",
        "        ╰─────────────╯          ",
    ],
    // Frame 1 - grooves diagonal /
    [
        "        ╭─────────────╮          ",
        "      ╭╱╱╱╱╱╱╱╱╱╱╱╱╱╱╮        ",
        "    ╭╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╮      ",
        "   ╭╱╱╭─────────╮╱╱╱╱╮     ",
        "  │╱╱│           │╱╱╱╱│     ",
        "  │╱╱│    ◉     │╱╱╱╱│     ",
        "  │╱╱│           │╱╱╱╱│     ",
        "   ╰╱╱╰─────────╯╱╱╱╱╯     ",
        "    ╰╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╯      ",
        "      ╰╱╱╱╱╱╱╱╱╱╱╱╱╱╱╯        ",
        "        ╰─────────────╯          ",
    ],
    // Frame 2 - grooves vertical
    [
        "        ╭─────────────╮          ",
        "      ╭│││││││││││││││╮        ",
        "    ╭│││││││││││││││││││╮      ",
        "   ╭│││╭─────────╮│││││╮     ",
        "  │││││           │││││││     ",
        "  │││││    ◉     │││││││     ",
        "  │││││           │││││││     ",
        "   ╰│││╰─────────╯│││││╯     ",
        "    ╰│││││││││││││││││││╯      ",
        "      ╰│││││││││││││││╯        ",
        "        ╰─────────────╯          ",
    ],
    // Frame 3 - grooves diagonal \
    [
        "        ╭─────────────╮          ",
        "      ╭╲╲╲╲╲╲╲╲╲╲╲╲╲╲╮        ",
        "    ╭╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╮      ",
        "   ╭╲╲╭─────────╮╲╲╲╲╮     ",
        "  │╲╲│           │╲╲╲╲│     ",
        "  │╲╲│    ◉     │╲╲╲╲│     ",
        "  │╲╲│           │╲╲╲╲│     ",
        "   ╰╲╲╰─────────╯╲╲╲╲╯     ",
        "    ╰╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╯      ",
        "      ╰╲╲╲╲╲╲╲╲╲╲╲╲╲╲╯        ",
        "        ╰─────────────╯          ",
    ],
];

/// Unicode vinyl label (stopped state).
const VINYL_UNICODE_LABEL: [&str; 11] = [
    "        ╭─────────────╮          ",
    "      ╭               ╮        ",
    "    ╭                   ╮      ",
    "   ╭     ╭─────────╮      ╮     ",
    "  │     │ SPITESTACK │     │     ",
    "  │     │  RECORDS   │     │     ",
    "  │     │    ◉      │     │     ",
    "   ╰     ╰─────────╯      ╯     ",
    "    ╰                   ╯      ",
    "      ╰               ╯        ",
    "        ╰─────────────╯          ",
];

// ============================================================================
// Fallback Tier: ASCII Art (Original)
// ============================================================================

/// Draw the mini vinyl widget for the status bar corner.
pub fn draw_mini_vinyl(f: &mut Frame, vinyl: &VinylState, theme: &Theme, area: Rect) {
    let frame_char = if vinyl.scratching {
        symbols::VINYL_SCRATCH[vinyl.scratch_frames % 3]
    } else if vinyl.spinning {
        symbols::VINYL_FRAMES[vinyl.ascii_frame()]
    } else {
        symbols::VINYL
    };

    let style = if vinyl.scratching {
        theme.error()
    } else if vinyl.spinning {
        theme.accent()
    } else {
        theme.muted()
    };

    let vinyl_widget = Paragraph::new(frame_char)
        .style(style)
        .alignment(Alignment::Center);

    f.render_widget(vinyl_widget, area);
}

/// Draw the large vinyl for full-screen music mode (ASCII fallback).
pub fn draw_large_vinyl(f: &mut Frame, vinyl: &VinylState, theme: &Theme, area: Rect) {
    // Use the labeled vinyl or animated frames
    let frame_art: &[&str; 11] = if vinyl.scratching {
        // Show label when scratching (stopped)
        &VINYL_LABEL
    } else {
        &VINYL_LARGE[vinyl.ascii_frame()]
    };

    let style = if vinyl.scratching {
        theme.error()
    } else {
        theme.text()
    };

    let lines: Vec<Line> = frame_art
        .iter()
        .map(|line| Line::from(Span::styled(*line, style)))
        .collect();

    let vinyl_widget = Paragraph::new(lines).alignment(Alignment::Center);

    f.render_widget(vinyl_widget, area);
}

/// Draw the vinyl with the SpiteStack Records label (static, for music mode header).
pub fn draw_vinyl_label(f: &mut Frame, theme: &Theme, area: Rect) {
    let lines: Vec<Line> = VINYL_LABEL
        .iter()
        .map(|line| Line::from(Span::styled(*line, theme.text())))
        .collect();

    let vinyl_widget = Paragraph::new(lines).alignment(Alignment::Center);

    f.render_widget(vinyl_widget, area);
}

/// Draw the tone arm (for music mode, positioned next to vinyl).
pub fn draw_tone_arm(f: &mut Frame, vinyl: &VinylState, theme: &Theme, area: Rect) {
    let arm = if vinyl.spinning {
        // Arm on record
        [
            "     |",
            "    / ",
            "   /  ",
            "  O   ",
        ]
    } else {
        // Arm lifted
        [
            "     |",
            "     |",
            "    / ",
            "  O   ",
        ]
    };

    let style = theme.muted();
    let lines: Vec<Line> = arm
        .iter()
        .map(|line| Line::from(Span::styled(*line, style)))
        .collect();

    let widget = Paragraph::new(lines);
    f.render_widget(widget, area);
}
