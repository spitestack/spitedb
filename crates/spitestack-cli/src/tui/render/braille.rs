//! Braille character graphics for high-resolution rendering.
//!
//! SpiteStack - Code Angry.
//!
//! Braille characters allow 2x4 "pixel" resolution per character cell.
//! Each character cell becomes a 2x4 grid of dots:
//!
//! ```text
//! ⡀⠄  (dots 0,3)
//! ⠂⠐  (dots 1,4)
//! ⠁⠈  (dots 2,5)
//! ⢀⠠  (dots 6,7)
//! ```
//!
//! Unicode Braille starts at U+2800 (blank) through U+28FF.

use std::f32::consts::{FRAC_PI_2, TAU};

/// Braille canvas for high-resolution rendering.
///
/// Provides a 2D canvas where each character cell is a 2x4 pixel grid.
/// Draw shapes using pixel coordinates, then render to Braille characters.
pub struct BrailleCanvas {
    /// Width in characters
    char_width: usize,
    /// Height in characters
    char_height: usize,
    /// Pixel data (2*width x 4*height)
    pixels: Vec<bool>,
}

impl BrailleCanvas {
    /// Create a new Braille canvas.
    ///
    /// `char_width` and `char_height` are in character cells.
    /// The actual pixel resolution is 2x width by 4x height.
    pub fn new(char_width: usize, char_height: usize) -> Self {
        let pixel_count = char_width * 2 * char_height * 4;
        Self {
            char_width,
            char_height,
            pixels: vec![false; pixel_count],
        }
    }

    /// Get the pixel width (2x character width).
    pub fn pixel_width(&self) -> usize {
        self.char_width * 2
    }

    /// Get the pixel height (4x character height).
    pub fn pixel_height(&self) -> usize {
        self.char_height * 4
    }

    /// Clear the canvas.
    pub fn clear(&mut self) {
        self.pixels.fill(false);
    }

    /// Set a pixel at the given coordinates.
    pub fn set(&mut self, x: usize, y: usize, value: bool) {
        if x < self.pixel_width() && y < self.pixel_height() {
            let idx = y * self.pixel_width() + x;
            self.pixels[idx] = value;
        }
    }

    /// Get a pixel at the given coordinates.
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x < self.pixel_width() && y < self.pixel_height() {
            self.pixels[y * self.pixel_width() + x]
        } else {
            false
        }
    }

    /// Set a pixel using floating-point coordinates.
    pub fn set_f(&mut self, x: f32, y: f32, value: bool) {
        if x >= 0.0 && y >= 0.0 {
            self.set(x as usize, y as usize, value);
        }
    }

    /// Draw a circle outline.
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32) {
        let circumference = TAU * radius;
        let steps = (circumference * 2.0).max(16.0) as usize;

        for i in 0..steps {
            let angle = (i as f32 / steps as f32) * TAU;
            let x = cx + radius * angle.cos();
            let y = cy + radius * angle.sin();
            self.set_f(x, y, true);
        }
    }

    /// Draw a filled circle.
    pub fn filled_circle(&mut self, cx: f32, cy: f32, radius: f32) {
        let r2 = radius * radius;
        let r_ceil = radius.ceil() as i32;

        for dy in -r_ceil..=r_ceil {
            for dx in -r_ceil..=r_ceil {
                if (dx * dx + dy * dy) as f32 <= r2 {
                    let x = cx + dx as f32;
                    let y = cy + dy as f32;
                    self.set_f(x, y, true);
                }
            }
        }
    }

    /// Draw an arc (partial circle).
    pub fn arc(&mut self, cx: f32, cy: f32, radius: f32, start_angle: f32, end_angle: f32) {
        let arc_length = radius * (end_angle - start_angle).abs();
        let steps = (arc_length * 2.0).max(8.0) as usize;

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let angle = start_angle + t * (end_angle - start_angle);
            let x = cx + radius * angle.cos();
            let y = cy + radius * angle.sin();
            self.set_f(x, y, true);
        }
    }

    /// Draw a line using Bresenham's algorithm.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let steps = dx.max(dy).ceil() as usize;

        if steps == 0 {
            self.set_f(x0, y0, true);
            return;
        }

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x0 + t * (x1 - x0);
            let y = y0 + t * (y1 - y0);
            self.set_f(x, y, true);
        }
    }

    /// Render the canvas to a vector of Braille character strings.
    pub fn render(&self) -> Vec<String> {
        let mut result = Vec::with_capacity(self.char_height);

        for char_y in 0..self.char_height {
            let mut line = String::with_capacity(self.char_width);
            for char_x in 0..self.char_width {
                line.push(self.char_at(char_x, char_y));
            }
            result.push(line);
        }

        result
    }

    /// Get the Braille character for a specific character cell.
    fn char_at(&self, char_x: usize, char_y: usize) -> char {
        let px = char_x * 2;
        let py = char_y * 4;

        // Braille dot pattern (Unicode standard):
        // 0 3
        // 1 4
        // 2 5
        // 6 7
        let dots = [
            self.get(px, py),         // dot 0 (top-left)
            self.get(px, py + 1),     // dot 1
            self.get(px, py + 2),     // dot 2
            self.get(px + 1, py),     // dot 3 (top-right)
            self.get(px + 1, py + 1), // dot 4
            self.get(px + 1, py + 2), // dot 5
            self.get(px, py + 3),     // dot 6 (bottom-left)
            self.get(px + 1, py + 3), // dot 7 (bottom-right)
        ];

        let mut codepoint: u32 = 0x2800; // Base Braille character (blank)
        for (i, &dot) in dots.iter().enumerate() {
            if dot {
                codepoint |= 1 << i;
            }
        }

        char::from_u32(codepoint).unwrap_or(' ')
    }
}

/// Generate a spinning vinyl record using Braille graphics.
///
/// Returns a vector of strings representing the vinyl at the given frame.
///
/// # Arguments
/// * `frame` - Current animation frame number
/// * `rotation_per_frame` - Radians to rotate per frame (use AnimationConfig::vinyl_rotation_per_frame())
/// * `char_width` - Canvas width in characters
/// * `char_height` - Canvas height in characters
/// * `include_tonearm` - Whether to draw the tonearm
/// * `spinning` - Whether the record is spinning (affects tonearm position)
pub fn generate_vinyl(
    frame: usize,
    rotation_per_frame: f32,
    char_width: usize,
    char_height: usize,
    include_tonearm: bool,
    spinning: bool,
) -> Vec<String> {
    let mut canvas = BrailleCanvas::new(char_width, char_height);

    let pixel_width = canvas.pixel_width() as f32;
    let pixel_height = canvas.pixel_height() as f32;
    let cx = pixel_width / 2.0;
    let cy = pixel_height / 2.0;
    let max_radius = cx.min(cy) - 2.0;

    // Outer edge (double line for thickness)
    canvas.circle(cx, cy, max_radius);
    canvas.circle(cx, cy, max_radius - 0.5);

    // Rotation angle based on frame - use tier-appropriate speed
    let rotation = (frame as f32 * rotation_per_frame) % TAU;

    // Draw grooves as partial arcs - more segments for smoother spin
    let groove_count = 8;
    for i in 0..groove_count {
        let radius = max_radius * 0.32 + (max_radius * 0.58) * (i as f32 / groove_count as f32);

        // Draw segmented arcs to create spinning illusion
        // More segments with longer arcs = smoother appearance
        let segment_count = 6;
        let arc_spacing = TAU / segment_count as f32;
        for j in 0..segment_count {
            let base_angle = rotation + (j as f32 * arc_spacing);
            let offset = i as f32 * 0.25; // Spiral offset for visual interest
            let start = base_angle + offset;
            let end = start + 0.7; // Longer arc length for more continuous grooves
            canvas.arc(cx, cy, radius, start, end);
        }
    }

    // Subtle outer edge highlight for vinyl sheen
    canvas.arc(cx, cy, max_radius - 1.0, rotation, rotation + 0.8);

    // Label area (inner circle)
    let label_radius = max_radius * 0.28;
    canvas.circle(cx, cy, label_radius);

    // Spindle (center hole)
    canvas.filled_circle(cx, cy, 1.5);

    // Draw tonearm if requested
    if include_tonearm {
        draw_tonearm(&mut canvas, cx, cy, max_radius, spinning);
    }

    canvas.render()
}

/// Draw a tonearm on the braille canvas.
///
/// The tonearm pivots from the top-right corner and swings onto/off the record.
/// Designed to look like a classic turntable arm with headshell, stylus, and counterweight.
fn draw_tonearm(canvas: &mut BrailleCanvas, cx: f32, cy: f32, max_radius: f32, on_record: bool) {
    // Pivot point at top-right of vinyl area (tonearm base)
    let pivot_x = cx + max_radius * 1.15;
    let pivot_y = cy - max_radius * 0.5;

    // Arm angle based on state (radians from pivot)
    // When on record, arm swings far onto the record; when lifted, clearly off to the side
    let arm_angle: f32 = if on_record {
        -2.6 // On record - stylus clearly in the groove area
    } else {
        -1.5 // Lifted - arm resting position, clearly off the record
    };

    let arm_length = max_radius * 0.95;

    // Calculate arm endpoint (where headshell attaches)
    let end_x = pivot_x + arm_length * arm_angle.cos();
    let end_y = pivot_y + arm_length * arm_angle.sin();

    // Draw the tonearm tube (main arm)
    canvas.line(pivot_x, pivot_y, end_x, end_y);
    // Draw a parallel line for arm thickness
    canvas.line(pivot_x, pivot_y + 0.5, end_x, end_y + 0.5);

    // Draw pivot base (circular mount)
    canvas.filled_circle(pivot_x, pivot_y, 2.5);
    canvas.circle(pivot_x, pivot_y, 3.5);

    // Draw headshell (angled cartridge holder)
    let headshell_angle = arm_angle - 0.3; // Slight angle for headshell
    let headshell_len = 4.0;
    let hs_end_x = end_x + headshell_len * headshell_angle.cos();
    let hs_end_y = end_y + headshell_len * headshell_angle.sin();
    canvas.line(end_x, end_y, hs_end_x, hs_end_y);
    // Headshell width
    let perp_angle = headshell_angle + FRAC_PI_2;
    canvas.line(
        end_x + 1.5 * perp_angle.cos(),
        end_y + 1.5 * perp_angle.sin(),
        end_x - 1.5 * perp_angle.cos(),
        end_y - 1.5 * perp_angle.sin(),
    );

    // Draw cartridge body (rectangle at end of headshell)
    canvas.filled_circle(hs_end_x, hs_end_y, 1.5);

    // Draw stylus/needle (small point extending down from cartridge)
    if on_record {
        let stylus_x = hs_end_x + 1.5 * headshell_angle.cos();
        let stylus_y = hs_end_y + 1.5 * headshell_angle.sin();
        canvas.line(hs_end_x, hs_end_y, stylus_x, stylus_y);
        canvas.set_f(stylus_x, stylus_y, true);
    }

    // Draw counterweight on the opposite side of pivot
    let cw_distance = 6.0;
    let cw_angle = arm_angle + std::f32::consts::PI; // Opposite direction
    let cw_x = pivot_x + cw_distance * cw_angle.cos();
    let cw_y = pivot_y + cw_distance * cw_angle.sin();
    canvas.filled_circle(cw_x, cw_y, 2.0);
    // Line connecting counterweight to pivot
    canvas.line(pivot_x, pivot_y, cw_x, cw_y);
}

/// Generate a static vinyl record (for stopped/scratched state).
///
/// # Arguments
/// * `char_width` - Canvas width in characters
/// * `char_height` - Canvas height in characters
/// * `include_tonearm` - Whether to draw the tonearm (will be in lifted position)
pub fn generate_vinyl_static(char_width: usize, char_height: usize, include_tonearm: bool) -> Vec<String> {
    let mut canvas = BrailleCanvas::new(char_width, char_height);

    let pixel_width = canvas.pixel_width() as f32;
    let pixel_height = canvas.pixel_height() as f32;
    let cx = pixel_width / 2.0;
    let cy = pixel_height / 2.0;
    let max_radius = cx.min(cy) - 2.0;

    // Outer edge
    canvas.circle(cx, cy, max_radius);
    canvas.circle(cx, cy, max_radius - 0.5);

    // Full concentric grooves (no rotation effect)
    let groove_count = 6;
    for i in 0..groove_count {
        let radius = max_radius * 0.35 + (max_radius * 0.55) * (i as f32 / groove_count as f32);
        canvas.circle(cx, cy, radius);
    }

    // Label area
    let label_radius = max_radius * 0.28;
    canvas.circle(cx, cy, label_radius);

    // Spindle
    canvas.filled_circle(cx, cy, 1.5);

    // Draw tonearm in lifted position if requested
    if include_tonearm {
        draw_tonearm(&mut canvas, cx, cy, max_radius, false);
    }

    canvas.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braille_canvas_new() {
        let canvas = BrailleCanvas::new(10, 5);
        assert_eq!(canvas.pixel_width(), 20);
        assert_eq!(canvas.pixel_height(), 20);
    }

    #[test]
    fn test_braille_empty_char() {
        let canvas = BrailleCanvas::new(1, 1);
        let result = canvas.render();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "\u{2800}"); // Empty Braille character
    }

    #[test]
    fn test_braille_full_char() {
        let mut canvas = BrailleCanvas::new(1, 1);
        // Set all 8 dots
        for x in 0..2 {
            for y in 0..4 {
                canvas.set(x, y, true);
            }
        }
        let result = canvas.render();
        assert_eq!(result[0], "\u{28FF}"); // Full Braille character
    }

    #[test]
    fn test_vinyl_generation() {
        // Use a realistic rotation speed (33.3 RPM at 60fps ≈ 0.058 rad/frame)
        let vinyl = generate_vinyl(0, 0.058, 20, 10, false, true);
        assert_eq!(vinyl.len(), 10);
        assert!(vinyl.iter().all(|line| line.chars().count() == 20));
    }

    #[test]
    fn test_vinyl_with_tonearm() {
        let vinyl = generate_vinyl(0, 0.058, 25, 12, true, true);
        assert_eq!(vinyl.len(), 12);
        // With tonearm, the canvas needs to be wider
        assert!(vinyl.iter().all(|line| line.chars().count() == 25));
    }
}
