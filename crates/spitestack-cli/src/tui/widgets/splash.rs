//! Splash screen widget.
//!
//! SpiteStack - Code Angry.
//!
//! Almost nothing. Just the word. The restraint is the statement.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::capabilities::CapabilityTier;
use crate::tui::theme::Theme;

/// Draw the splash screen.
pub fn draw_splash(f: &mut Frame, theme: &Theme, tier: CapabilityTier, area: Rect) {
    // Clear with void background
    let block = ratatui::widgets::Block::default()
        .style(ratatui::style::Style::default().bg(theme.void));
    f.render_widget(block, area);

    // Calculate center-ish position (slightly off-center for that raw feel)
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(1),
            Constraint::Percentage(60),
        ])
        .split(area);

    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(10),
            Constraint::Percentage(65),
        ])
        .split(vertical_chunks[1]);

    // The word "spite." with tier-appropriate styling
    let spite_style = match tier {
        CapabilityTier::Premium => {
            // Bold for premium feel
            theme.text().add_modifier(Modifier::BOLD)
        }
        _ => theme.text(),
    };

    let dot_style = match tier {
        CapabilityTier::Premium => ratatui::style::Style::default().fg(theme.ember),
        _ => theme.muted(),
    };

    let spite = Paragraph::new(Line::from(vec![
        Span::styled("spite", spite_style),
        Span::styled(".", dot_style),
    ]));
    f.render_widget(spite, horizontal_chunks[1]);

    // Visual noise in corner (tier-appropriate)
    let noise_text = match tier {
        CapabilityTier::Premium => "─═╍─═╍─",
        CapabilityTier::Enhanced => "─═─═─",
        CapabilityTier::Fallback => "-=#=-",
    };

    let noise_width = noise_text.chars().count() as u16 + 1;
    let noise_area = Rect {
        x: area.width.saturating_sub(noise_width),
        y: area.height.saturating_sub(3),
        width: noise_width,
        height: 1,
    };

    if noise_area.x < area.width && noise_area.y < area.height {
        let noise = Paragraph::new(noise_text).style(theme.muted());
        f.render_widget(noise, noise_area);
    }
}
