//! TrueColor gradient generation.
//!
//! SpiteStack - Code Angry.
//!
//! Provides color interpolation and predefined gradients for the
//! Long Island Grit theme.

use ratatui::style::Color;

/// Linearly interpolate between two RGB colors.
///
/// `t` should be in the range [0.0, 1.0].
pub fn lerp_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);

    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            Color::Rgb(
                lerp_u8(r1, r2, t),
                lerp_u8(g1, g2, t),
                lerp_u8(b1, b2, t),
            )
        }
        // Fallback: return from color if not RGB
        _ => from,
    }
}

/// Linearly interpolate between two u8 values.
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let result = (a as f32) * (1.0 - t) + (b as f32) * t;
    result.round() as u8
}

// ============================================================================
// VU Meter Gradient
// ============================================================================

/// Standard VU meter gradient: green -> yellow -> red.
///
/// - 0.0 - 0.6: Green zone (safe)
/// - 0.6 - 0.8: Yellow zone (caution)
/// - 0.8 - 1.0: Red zone (peak)
pub fn vu_gradient(position: f32) -> Color {
    let position = position.clamp(0.0, 1.0);

    // Green zone
    let green = Color::Rgb(40, 180, 40);
    // Yellow zone
    let yellow = Color::Rgb(220, 200, 40);
    // Red zone
    let red = Color::Rgb(200, 40, 40);

    if position < 0.6 {
        // Pure green
        green
    } else if position < 0.8 {
        // Green to yellow transition
        let t = (position - 0.6) / 0.2;
        lerp_color(green, yellow, t)
    } else {
        // Yellow to red transition
        let t = (position - 0.8) / 0.2;
        lerp_color(yellow, red, t)
    }
}

// ============================================================================
// Long Island Grit Gradients
// ============================================================================

/// Blood gradient: maroon -> blood -> ember.
///
/// The signature SpiteStack gradient, evoking Glassjaw's visual aesthetic.
pub fn blood_gradient(position: f32) -> Color {
    let position = position.clamp(0.0, 1.0);

    // #4A0E0E - dried, oxidized blood
    let maroon = Color::Rgb(74, 14, 14);
    // #8B0000 - deep arterial red
    let blood = Color::Rgb(139, 0, 0);
    // #CC5500 - burning, urgent
    let ember = Color::Rgb(204, 85, 0);

    if position < 0.5 {
        // Maroon to blood
        let t = position * 2.0;
        lerp_color(maroon, blood, t)
    } else {
        // Blood to ember
        let t = (position - 0.5) * 2.0;
        lerp_color(blood, ember, t)
    }
}

/// Ash gradient: void -> charcoal -> ash -> bone.
///
/// A subtle monochrome gradient for backgrounds and muted elements.
pub fn ash_gradient(position: f32) -> Color {
    let position = position.clamp(0.0, 1.0);

    // #0A0A0A - near-black, the void
    let void = Color::Rgb(10, 10, 10);
    // #1C1C1C - worn black t-shirt
    let charcoal = Color::Rgb(28, 28, 28);
    // #696969 - cigarette ash, smoke
    let ash = Color::Rgb(105, 105, 105);
    // #D4C4A8 - aged paper, dried bone
    let bone = Color::Rgb(212, 196, 168);

    if position < 0.33 {
        let t = position / 0.33;
        lerp_color(void, charcoal, t)
    } else if position < 0.66 {
        let t = (position - 0.33) / 0.33;
        lerp_color(charcoal, ash, t)
    } else {
        let t = (position - 0.66) / 0.34;
        lerp_color(ash, bone, t)
    }
}

/// Ember pulse: cycles through ember intensities.
///
/// For animated elements like the record dot or active indicators.
pub fn ember_pulse(position: f32) -> Color {
    let position = position.clamp(0.0, 1.0);

    // Dark ember
    let dark = Color::Rgb(102, 42, 0);
    // Normal ember
    let ember = Color::Rgb(204, 85, 0);
    // Bright ember
    let bright = Color::Rgb(255, 128, 32);

    // Create a pulse effect (0->1->0)
    let pulse = if position < 0.5 {
        position * 2.0
    } else {
        (1.0 - position) * 2.0
    };

    if pulse < 0.5 {
        let t = pulse * 2.0;
        lerp_color(dark, ember, t)
    } else {
        let t = (pulse - 0.5) * 2.0;
        lerp_color(ember, bright, t)
    }
}

// ============================================================================
// Vinyl Gradient
// ============================================================================

/// Vinyl groove gradient: creates depth effect from edge to center.
pub fn vinyl_gradient(position: f32) -> Color {
    let position = position.clamp(0.0, 1.0);

    // Outer edge (lighter)
    let edge = Color::Rgb(60, 60, 60);
    // Middle (standard vinyl black)
    let vinyl = Color::Rgb(30, 30, 30);
    // Inner (slightly lighter for label area)
    let label = Color::Rgb(80, 70, 60);

    if position < 0.7 {
        // Edge to vinyl
        let t = position / 0.7;
        lerp_color(edge, vinyl, t)
    } else {
        // Vinyl to label area
        let t = (position - 0.7) / 0.3;
        lerp_color(vinyl, label, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lerp_color() {
        let black = Color::Rgb(0, 0, 0);
        let white = Color::Rgb(255, 255, 255);

        // Start
        assert!(matches!(lerp_color(black, white, 0.0), Color::Rgb(0, 0, 0)));
        // End
        assert!(matches!(lerp_color(black, white, 1.0), Color::Rgb(255, 255, 255)));
        // Middle
        if let Color::Rgb(r, g, b) = lerp_color(black, white, 0.5) {
            assert!((r as i32 - 128).abs() <= 1);
            assert!((g as i32 - 128).abs() <= 1);
            assert!((b as i32 - 128).abs() <= 1);
        }
    }

    #[test]
    fn test_vu_gradient_zones() {
        // Green zone
        let green = vu_gradient(0.3);
        assert!(matches!(green, Color::Rgb(40, 180, 40)));

        // Red zone
        let red = vu_gradient(1.0);
        assert!(matches!(red, Color::Rgb(200, 40, 40)));
    }

    #[test]
    fn test_blood_gradient() {
        // Start (maroon)
        let start = blood_gradient(0.0);
        assert!(matches!(start, Color::Rgb(74, 14, 14)));

        // End (ember)
        let end = blood_gradient(1.0);
        assert!(matches!(end, Color::Rgb(204, 85, 0)));
    }
}
