//! Full-screen music mode widget.
//!
//! SpiteStack Records - "Code Angry"
//!
//! Large spinning vinyl with track info and playlist links.
//! Press 'M' to enter, 'M' or Esc to exit.
//!
//! - Premium tier: Gradient-colored logo, Braille vinyl
//! - Enhanced tier: Unicode styling, cleaner borders
//! - Fallback tier: ASCII art

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::capabilities::CapabilityTier;
use crate::tui::render::gradients::blood_gradient;
use crate::tui::theme::{symbols, SymbolSet, Theme};
use crate::tui::widgets::vinyl::draw_large_vinyl_tiered;

/// ASCII art logo for SpiteStack Records (ASCII-safe).
const LOGO: [&str; 6] = [
    " ____  ____  ___ _____ _____ ____ _____  _    ____ _  __",
    "/ ___||  _ \\|_ _|_   _| ____/ ___|_   _|/ \\  / ___| |/ /",
    "\\___ \\| |_) || |  | | |  _| \\___ \\ | | / _ \\| |   | ' / ",
    " ___) |  __/ | |  | | | |___ ___) || |/ ___ \\ |___| . \\ ",
    "|____/|_|   |___| |_| |_____|____/ |_/_/   \\_\\____|_|\\_\\",
    "                    R E C O R D S                       ",
];

/// Draw the full-screen music mode with tier-appropriate rendering.
pub fn draw_music_mode(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    // Clear with void background
    let block = Block::default().style(ratatui::style::Style::default().bg(theme.void));
    f.render_widget(block, area);

    // Main vertical layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Top margin
            Constraint::Length(8),  // Logo area
            Constraint::Length(2),  // Subtitle
            Constraint::Min(12),    // Vinyl + info
            Constraint::Length(4),  // Controls
            Constraint::Length(2),  // Bottom margin
        ])
        .split(area);

    // Logo (simplified - just text if terminal too narrow)
    if area.width >= 80 {
        draw_logo_tiered(f, theme, tier, main_chunks[1]);
    } else {
        draw_simple_title_tiered(f, theme, tier, main_chunks[1]);
    }

    // Subtitle: "Code Angry."
    let subtitle = Paragraph::new(Line::from(vec![
        Span::styled("\"Code Angry.\"", theme.accent().add_modifier(Modifier::ITALIC)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(subtitle, main_chunks[2]);

    // Vinyl + info area
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Vinyl
            Constraint::Percentage(50), // Info
        ])
        .split(main_chunks[3]);

    // Draw spinning vinyl with tier-appropriate rendering
    draw_large_vinyl_tiered(f, &app.vinyl, theme, tier, content_chunks[0]);

    // Info panel
    draw_info_panel(f, app, theme, tier, content_chunks[1]);

    // Controls
    draw_controls(f, theme, tier, main_chunks[4]);
}

/// Draw the logo with tier-appropriate rendering.
/// Premium: Blood gradient across the ASCII art
/// Enhanced/Fallback: Standard styled text
fn draw_logo_tiered(f: &mut Frame, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let lines: Vec<Line> = match tier {
        CapabilityTier::Premium => {
            // Gradient-colored logo - blood gradient from left to right
            LOGO.iter()
                .map(|line| {
                    let chars: Vec<Span> = line
                        .chars()
                        .enumerate()
                        .map(|(i, c)| {
                            let t = i as f32 / line.len().max(1) as f32;
                            let color = blood_gradient(t);
                            Span::styled(c.to_string(), Style::default().fg(color))
                        })
                        .collect();
                    Line::from(chars)
                })
                .collect()
        }
        _ => {
            // Standard styling
            LOGO.iter()
                .map(|line| Line::from(Span::styled(*line, theme.text())))
                .collect()
        }
    };

    let logo = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(logo, area);
}

/// Draw simple title with tier-appropriate styling.
fn draw_simple_title_tiered(f: &mut Frame, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let (spite_style, records_style) = match tier {
        CapabilityTier::Premium => {
            // Gradient effect for Premium
            (
                Style::default().fg(theme.blood).add_modifier(Modifier::BOLD),
                Style::default().fg(theme.ember).add_modifier(Modifier::BOLD),
            )
        }
        _ => (
            theme.header().add_modifier(Modifier::BOLD),
            theme.accent().add_modifier(Modifier::BOLD),
        ),
    };

    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("SPITESTACK ", spite_style),
            Span::styled("RECORDS", records_style),
        ]),
        Line::from(""),
    ])
    .alignment(Alignment::Center);
    f.render_widget(title, area);
}

fn draw_info_panel(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let info_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // "Now Playing"
            Constraint::Length(4),  // Track info
            Constraint::Min(6),     // Playlist links
        ])
        .split(area);

    // Now playing header
    let now_playing = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Now Playing:", theme.muted()),
        ]),
    ])
    .alignment(Alignment::Center);
    f.render_widget(now_playing, info_chunks[0]);

    // Track info (what you're building)
    let project_name = app
        .project
        .name
        .as_deref()
        .unwrap_or("Your Code");

    let track_info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(project_name, theme.text().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("SpiteStack Session", theme.muted()),
        ]),
    ])
    .alignment(Alignment::Center);
    f.render_widget(track_info, info_chunks[1]);

    // Playlist links
    draw_playlist_links(f, app, theme, tier, info_chunks[2]);
}

fn draw_playlist_links(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let syms = SymbolSet::for_tier(tier);

    let links_block = Block::default()
        .title(Span::styled(" Listen While You Build ", theme.muted()))
        .borders(Borders::TOP)
        .border_style(theme.border());

    let inner = links_block.inner(area);
    f.render_widget(links_block, area);

    let selected = app.music_mode.selected_link;

    let spotify_style = if selected == 0 {
        theme.selected()
    } else {
        theme.text()
    };

    let apple_style = if selected == 1 {
        theme.selected()
    } else {
        theme.text()
    };

    let links = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(if selected == 0 { syms.arrow } else { " " }, spotify_style),
            Span::styled(" Spotify - Glassjaw: Worship and Tribute", spotify_style),
        ]),
        Line::from(vec![
            Span::styled(if selected == 1 { syms.arrow } else { " " }, apple_style),
            Span::styled(" Apple Music - Glassjaw: Worship and Tribute", apple_style),
        ]),
        Line::from(""),
    ])
    .alignment(Alignment::Left);
    f.render_widget(links, inner);
}

fn draw_controls(f: &mut Frame, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let _ = tier; // Reserved for future tier-specific control rendering

    let controls = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("[S]", theme.accent()),
            Span::styled("potify  ", theme.muted()),
            Span::styled("[A]", theme.accent()),
            Span::styled("pple Music  ", theme.muted()),
            Span::styled("[j/k]", theme.accent()),
            Span::styled(" Navigate  ", theme.muted()),
            Span::styled("[Enter]", theme.accent()),
            Span::styled(" Open  ", theme.muted()),
            Span::styled("[M/Esc]", theme.accent()),
            Span::styled(" Back", theme.muted()),
        ]),
    ])
    .alignment(Alignment::Center);
    f.render_widget(controls, area);
}
