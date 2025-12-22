//! Dashboard layout widget.
//!
//! SpiteStack - Code Angry.
//!
//! ┌──────────────────────────────────────────────────────────────────┐
//! │ ◉ SPITESTACK                         todo-app | 3 agg | idle    │
//! ├───────────────────────────┬──────────────────────────────────────┤
//! │ ERRORS / MIXING DESK      │ OUTPUT                               │
//! │                           │                                      │
//! │ × ForbiddenCall           │ ✓ mixed 3 aggregates in 42ms         │
//! │   Todo/aggregate.ts:24    │ › server running on :3000            │
//! │   † fix available         │                                      │
//! │                           │ · watching for changes...            │
//! │                           │                                      │
//! ├───────────────────────────┴──────────────────────────────────────┤
//! │ / _                                                              │
//! └──────────────────────────────────────────────────────────────────┘

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::tui::app::{App, AppMode};
use crate::tui::capabilities::CapabilityTier;
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    draw_errors, draw_input, draw_music_mode, draw_output, draw_status, draw_vu_meters_tiered,
};
use crate::tui::widgets::errors::draw_error_detail;

/// Draw the main dashboard layout with tier-appropriate rendering.
pub fn draw_dashboard(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    // Check for music mode first - full screen takeover
    if matches!(app.mode, AppMode::MusicMode) {
        draw_music_mode(f, app, theme, tier, area);
        return;
    }

    // Main vertical split
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Status bar
            Constraint::Min(8),     // Main content (reduced for more input space)
            Constraint::Length(3),  // Command input (increased for better UX)
        ])
        .split(area);

    // Status bar
    draw_status(f, app, theme, tier, chunks[0]);

    // Main content area
    match app.mode {
        AppMode::ErrorDetail | AppMode::FixSelection => {
            // Full screen error detail
            draw_error_detail(f, app, theme, tier, chunks[1]);
        }
        AppMode::Compiling => {
            // Show VU meters during compilation (mixing desk view)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(35),  // VU Meters (Mixing Desk)
                    Constraint::Percentage(65),  // Output
                ])
                .split(chunks[1]);

            // Use tiered VU meters with gradient coloring
            draw_vu_meters_tiered(f, &app.vu_meters, theme, tier, main_chunks[0]);
            draw_output(f, app, theme, tier, main_chunks[1]);
        }
        _ => {
            // Normal dashboard: errors + output
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(35),  // Errors
                    Constraint::Percentage(65),  // Output
                ])
                .split(chunks[1]);

            draw_errors(f, app, theme, tier, main_chunks[0]);
            draw_output(f, app, theme, tier, main_chunks[1]);
        }
    }

    // Command input
    draw_input(f, app, theme, tier, chunks[2]);
}
