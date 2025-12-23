//! SpiteStack CLI UI primitives.
//!
//! Punk rock, space-age terminal experience.
#![allow(dead_code)]

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};
use std::time::Duration;

/// Neon Void color palette
pub mod colors {
    use console::Color;

    pub const CYAN: Color = Color::Color256(51);       // Electric cyan
    pub const MAGENTA: Color = Color::Color256(201);   // Hot magenta
    pub const VIOLET: Color = Color::Color256(135);    // Soft violet
    pub const NEON_GREEN: Color = Color::Color256(82); // Neon green
    pub const DIM: Color = Color::Color256(240);       // Dim gray
}

/// Space-age symbols
pub mod symbols {
    pub const DIAMOND: &str = "\u{25C6}";          // ◆
    pub const DIAMOND_OUTLINE: &str = "\u{25C7}";  // ◇
    pub const TARGET_FILLED: &str = "\u{25C9}";    // ◉
    pub const TARGET_EMPTY: &str = "\u{25CE}";     // ◎
    pub const TRIANGLE: &str = "\u{25B8}";         // ▸
    pub const PROGRESS_FILLED: &str = "\u{25B0}";  // ▰
    pub const PROGRESS_EMPTY: &str = "\u{25B1}";   // ▱
    pub const STAR: &str = "\u{2726}";             // ✦
    pub const DOT: &str = "\u{00B7}";              // ·
    pub const ARROW: &str = "\u{2500}\u{25B8}";    // ─▸
}

/// Taglines - randomly selected
const TAGLINES: &[&str] = &[
    "Event Sourcing. No Bullshit.",
    "Built with spite.",
    "Your ORM wishes it was this cool.",
    "Stop fighting your database.",
];

/// Success messages - randomly selected
const SUCCESS_MESSAGES: &[&str] = &[
    "LET'S GO",
    "NAILED IT",
    "SHIP IT",
    "PURE FIRE",
];

/// Get a random tagline
pub fn random_tagline() -> &'static str {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize;
    TAGLINES[seed % TAGLINES.len()]
}

/// Get a random success message
pub fn random_success_message() -> &'static str {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize;
    SUCCESS_MESSAGES[seed % SUCCESS_MESSAGES.len()]
}

/// Create a clickable file link (OSC 8 hyperlink)
/// Works in iTerm2, Windows Terminal, Hyper, kitty, Alacritty, WezTerm, VS Code terminal
pub fn file_link(path: &str, line: u32) -> String {
    let abs_path = std::fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string());
    let uri = format!("file://{}#{}", abs_path, line);
    let display = format!("{}:{}", path, line);
    format!("\x1b]8;;{}\x07{}\x1b]8;;\x07", uri, display)
}

/// HSL to RGB conversion for gradients
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Create a gradient across text (cyan -> magenta)
pub fn gradient_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let len = chars.len() as f32;
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if c.is_whitespace() {
                c.to_string()
            } else {
                let t = i as f32 / len;
                // Interpolate hue from cyan (180) to magenta (300)
                let hue = 180.0 + (t * 120.0);
                let (r, g, b) = hsl_to_rgb(hue, 1.0, 0.6);
                format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, c)
            }
        })
        .collect()
}

/// Print the SpiteStack logo banner with gradient
pub fn print_banner() {
    let logo = r#"
       ╔═══════════════════════════════════════════════════════╗
       ║                                                       ║
       ║    ▄▄▄▄▄  ▄▄▄▄▄  ▄▄▄▄▄  ▄▄▄▄▄  ▄▄▄▄▄                 ║
       ║    █▀▀▀▀  █▀▀▀█  ▀▀█▀▀  ▀▀█▀▀  █▀▀▀▀                 ║
       ║    ▀▀▀▀█  █▀▀▀▀    █      █    █▀▀▀                  ║
       ║    ▀▀▀▀▀  ▀        ▀      ▀    ▀▀▀▀▀                 ║
       ║                 S · T · A · C · K                     ║
       ║                                                       ║
       ║    ═══════════════════════════════════════════════    ║"#;

    // Print logo with gradient
    for line in logo.lines() {
        println!("{}", gradient_text(line));
    }

    // Print tagline
    let tagline = random_tagline();
    let tagline_line = format!(
        "       ║    {} {:<38} {}    ║",
        symbols::DIAMOND_OUTLINE,
        tagline,
        symbols::DIAMOND_OUTLINE
    );
    println!("{}", gradient_text(&tagline_line));

    println!(
        "{}",
        gradient_text("       ║                                                       ║")
    );
    println!(
        "{}",
        gradient_text("       ╚═══════════════════════════════════════════════════════╝")
    );
    println!();
}

/// Print compact version header
pub fn print_compact_header(version: &str) {
    println!(
        "  {} {} {}",
        style(symbols::DIAMOND).fg(colors::CYAN),
        style("spitestack").fg(colors::CYAN).bold(),
        style(version).dim()
    );
}

/// Print a success message
pub fn success(msg: &str) {
    println!(
        "  {} {}",
        style(symbols::TARGET_FILLED).fg(colors::NEON_GREEN),
        msg
    );
}

/// Print an error message
pub fn error(msg: &str) {
    println!(
        "  {} {}",
        style(symbols::DIAMOND).fg(colors::MAGENTA),
        style(msg).fg(colors::MAGENTA)
    );
}

/// Print an info message
pub fn info(msg: &str) {
    println!(
        "  {} {}",
        style(symbols::DIAMOND_OUTLINE).fg(colors::CYAN),
        msg
    );
}

/// Print a dim/secondary message
pub fn dim(msg: &str) {
    println!("  {}", style(msg).fg(colors::DIM));
}

/// Create a spinner with punk rock styling
pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("\u{25CE}\u{25C9}\u{25CE}\u{25C9}") // ◎◉◎◉
            .template("  {spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(150));
    pb
}

/// Create a progress bar with space-age styling
pub fn progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("     {bar:32.cyan/dim} {percent:>3}%")
            .unwrap()
            .progress_chars("\u{25B0}\u{25B0}\u{25B1}"), // ▰▰▱
    );
    pb
}

/// Print a divider line
pub fn divider() {
    println!();
    let line = "\u{254C}".repeat(53); // ╌
    println!("  {}", style(line).fg(colors::DIM));
    println!();
}

/// Print a box header
pub fn box_header(title: &str) {
    let width = 55;
    let title_padded = format!(" {} ", title);
    let title_len = title_padded.chars().count();
    let dashes = width - title_len - 4;

    println!(
        "  {}{}{}{}",
        style("\u{256D}\u{2500}").fg(colors::CYAN), // ╭─
        style(title_padded).fg(colors::CYAN).bold(),
        style("\u{2500}".repeat(dashes)).fg(colors::CYAN),
        style("\u{256E}").fg(colors::CYAN) // ╮
    );
}

/// Print a box line
pub fn box_line(content: &str) {
    let width: usize = 53;
    let content_len = content.chars().count();
    let padding = width.saturating_sub(content_len);
    println!(
        "  {} {}{}{}",
        style("\u{2502}").fg(colors::CYAN), // │
        content,
        " ".repeat(padding),
        style("\u{2502}").fg(colors::CYAN)
    );
}

/// Print a box footer
pub fn box_footer() {
    let width = 55;
    println!(
        "  {}{}{}",
        style("\u{2570}").fg(colors::CYAN), // ╰
        style("\u{2500}".repeat(width - 2)).fg(colors::CYAN),
        style("\u{256F}").fg(colors::CYAN) // ╯
    );
}

/// Print the sparkle animation for success celebration
pub fn sparkle_animation() {
    let width = 51;
    let mut stdout = io::stdout();

    // Build sparkle pattern
    let mut line: Vec<char> = vec![' '; width];
    for i in (0..width).step_by(4) {
        line[i] = if i % 8 == 0 { '\u{2726}' } else { '\u{00B7}' }; // ✦ or ·
    }

    // Animate: cycle brightness 3 times
    for _ in 0..3 {
        print!("\r  ");
        for c in &line {
            if *c == '\u{2726}' {
                print!("{}", style(c).fg(colors::CYAN).bold());
            } else if *c == '\u{00B7}' {
                print!("{}", style(c).dim());
            } else {
                print!("{}", c);
            }
        }
        let _ = stdout.flush();
        std::thread::sleep(Duration::from_millis(100));

        print!("\r  ");
        for c in &line {
            if *c == '\u{2726}' {
                print!("{}", style(c).dim());
            } else if *c == '\u{00B7}' {
                print!("{}", style(c).fg(colors::VIOLET));
            } else {
                print!("{}", c);
            }
        }
        let _ = stdout.flush();
        std::thread::sleep(Duration::from_millis(100));
    }
    println!();
}

/// Print success celebration banner
pub fn success_banner() {
    sparkle_animation();

    let line = "\u{2550}".repeat(51); // ═
    println!("  {}", style(&line).fg(colors::CYAN));
    println!();

    let msg = random_success_message();
    let spaced_msg: String = msg.chars().map(|c| format!("{} ", c)).collect();
    let spaced_msg = spaced_msg.trim();
    let padding = (51 - spaced_msg.len() - 4) / 2;

    println!(
        "  {}{}  {}  {}{}",
        " ".repeat(padding),
        style(symbols::DIAMOND).fg(colors::MAGENTA).bold(),
        gradient_text(spaced_msg),
        style(symbols::DIAMOND).fg(colors::MAGENTA).bold(),
        " ".repeat(padding)
    );

    println!();
    println!("  {}", style(&line).fg(colors::CYAN));
    sparkle_animation();
    println!();
}

/// Print file tree item
pub fn tree_item(prefix: &str, name: &str, description: Option<&str>, is_last: bool) {
    let connector = if is_last {
        "\u{2570}\u{2500}\u{2500}" // ╰──
    } else {
        "\u{251C}\u{2500}\u{2500}" // ├──
    };

    if let Some(desc) = description {
        println!(
            "  {}{}  {}   {}",
            style(prefix).fg(colors::DIM),
            style(connector).fg(colors::DIM),
            style(name).fg(colors::CYAN),
            style(desc).dim()
        );
    } else {
        println!(
            "  {}{}  {}",
            style(prefix).fg(colors::DIM),
            style(connector).fg(colors::DIM),
            style(name).fg(colors::CYAN)
        );
    }
}

/// Print directory in tree
pub fn tree_dir(prefix: &str, name: &str) {
    println!(
        "  {}{} {}/",
        style(prefix).fg(colors::DIM),
        style(symbols::TRIANGLE).fg(colors::CYAN),
        style(name).fg(colors::CYAN).bold()
    );
}

/// Print aggregate summary line
pub fn aggregate_line(name: &str, commands: usize, events: usize, total_possible: usize) {
    let filled = (events * 8) / total_possible.max(1);
    let bar: String = format!(
        "{}{}",
        symbols::PROGRESS_FILLED.repeat(filled.min(8)),
        symbols::PROGRESS_EMPTY.repeat(8 - filled.min(8))
    );

    println!(
        "  {}   {:12} {} commands   {} events   {}",
        style(symbols::TRIANGLE).fg(colors::CYAN),
        style(name).bold(),
        commands,
        events,
        style(bar).fg(colors::VIOLET)
    );
}

/// Print timing information
pub fn timing(label: &str, duration_ms: u128) {
    println!(
        "  {} {} in {}ms{}",
        style(symbols::DIAMOND_OUTLINE).fg(colors::CYAN),
        label,
        duration_ms,
        if duration_ms < 100 { ". Go ship something." } else { "" }
    );
}

/// Print "Hold up" error header
pub fn error_header() {
    println!();
    println!(
        "  {} {}",
        style(symbols::DIAMOND).fg(colors::MAGENTA).bold(),
        style("Hold up.").fg(colors::MAGENTA).bold()
    );
    println!();
}

/// Print "Nope" error header (for check failures)
pub fn nope_header() {
    println!();
    println!(
        "  {} {}",
        style(symbols::DIAMOND).fg(colors::MAGENTA).bold(),
        style("Nope.").fg(colors::MAGENTA).bold()
    );
    println!();
}

/// Print "Looking good" success for check
pub fn looking_good() {
    println!(
        "  {} {}",
        style(symbols::TARGET_FILLED).fg(colors::NEON_GREEN),
        style("Looking good.").bold()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsl_to_rgb() {
        // Cyan (180 degrees)
        let (r, g, b) = hsl_to_rgb(180.0, 1.0, 0.5);
        assert_eq!(r, 0);
        assert!(g > 200);
        assert!(b > 200);
    }

    #[test]
    fn test_gradient_text_empty() {
        assert_eq!(gradient_text(""), "");
    }

    #[test]
    fn test_file_link_format() {
        let link = file_link("test.ts", 42);
        assert!(link.contains("test.ts"));
        assert!(link.contains("\x1b]8;;"));
    }
}
