//! VU meter progress widget for compilation phases.
//!
//! SpiteStack - Code Angry.
//!
//! Displays compilation phases as "tracks" with VU-meter-style progress bars.
//! - Premium tier: Gradient-colored bars with Unicode blocks
//! - Enhanced tier: Unicode block characters
//! - Fallback tier: ASCII characters
//!
//! ```text
//! ┌─ MIXING DESK ──────────────────────────────────┐
//! │ TRACK 1: Parsing    [████████████░░░░] 100%    │
//! │ TRACK 2: IR Conv    [████████░░░░░░░░]  50%    │
//! │ TRACK 3: Validate   [░░░░░░░░░░░░░░░░]   0%    │
//! │ TRACK 4: CodeGen    [░░░░░░░░░░░░░░░░]   0%    │
//! └────────────────────────────────────────────────┘
//! ```

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::VuMeterState;
use crate::tui::capabilities::CapabilityTier;
use crate::tui::render::gradients::vu_gradient;
use crate::tui::theme::{SymbolSet, Theme};

/// VU meter bar width in characters.
const VU_WIDTH: usize = 20;

// ============================================================================
// Tiered VU Meter Rendering
// ============================================================================

/// Draw VU meters with tier-appropriate rendering.
pub fn draw_vu_meters_tiered(
    f: &mut Frame,
    state: &VuMeterState,
    theme: &Theme,
    tier: CapabilityTier,
    area: Rect,
) {
    let syms = SymbolSet::for_tier(tier);

    // Use rounded borders for Premium tier
    let border_set = match tier {
        CapabilityTier::Premium => border::ROUNDED,
        _ => border::PLAIN,
    };

    let block = Block::default()
        .title(Span::styled(
            " MIXING DESK ",
            theme.header().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(theme.border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Need at least 4 lines for the tracks
    if inner.height < 4 {
        return;
    }

    // Layout for 4 tracks
    let track_height = inner.height / 4;
    let constraints: Vec<Constraint> = (0..4)
        .map(|i| {
            if i == 3 {
                Constraint::Min(1)
            } else {
                Constraint::Length(track_height)
            }
        })
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, track) in state.tracks.iter().enumerate() {
        if i < chunks.len() {
            draw_single_track_tiered(
                f,
                i + 1,
                track.name,
                track.progress,
                track.peak,
                i == state.active_track,
                track.complete,
                theme,
                tier,
                chunks[i],
            );
        }
    }
}

fn draw_single_track_tiered(
    f: &mut Frame,
    track_num: usize,
    name: &str,
    progress: f32,
    peak: f32,
    active: bool,
    complete: bool,
    theme: &Theme,
    tier: CapabilityTier,
    area: Rect,
) {
    let syms = SymbolSet::for_tier(tier);

    // Build the bar based on tier
    let bar_spans: Vec<Span> = match tier {
        CapabilityTier::Premium => build_gradient_bar(progress, peak, theme),
        CapabilityTier::Enhanced => build_unicode_bar(progress, peak, syms, theme, active, complete),
        CapabilityTier::Fallback => build_ascii_bar(progress, peak, theme, active, complete),
    };

    // Label styles
    let label_style = if active {
        theme.accent()
    } else if complete {
        theme.success()
    } else {
        theme.muted()
    };

    let percent = (progress * 100.0) as u8;
    let percent_str = format!("{:>3}%", percent);

    // Track indicator
    let track_indicator = if active { syms.arrow } else { " " };

    // Build the line
    let mut spans = vec![
        Span::styled(track_indicator, label_style),
        Span::styled(format!("TRACK {}: ", track_num), label_style),
        Span::styled(format!("{:<10}", name), theme.text()),
        Span::styled(syms.border_vertical, theme.muted()),
    ];
    spans.extend(bar_spans);
    spans.push(Span::styled(syms.border_vertical, theme.muted()));
    spans.push(Span::styled(format!(" {}", percent_str), label_style));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}

/// Build gradient-colored VU bar (Premium tier).
fn build_gradient_bar(progress: f32, peak: f32, theme: &Theme) -> Vec<Span<'static>> {
    let filled = (progress * VU_WIDTH as f32) as usize;
    let peak_pos = (peak * VU_WIDTH as f32) as usize;

    (0..VU_WIDTH)
        .map(|i| {
            let pos = i as f32 / VU_WIDTH as f32;
            let is_filled = i < filled;
            let is_peak = i == peak_pos && peak > progress;

            let char = if is_filled {
                "█"
            } else if is_peak {
                "▓"
            } else {
                " "
            };

            let color = if is_filled || is_peak {
                vu_gradient(pos)
            } else {
                theme.charcoal
            };

            Span::styled(char.to_string(), Style::default().fg(color))
        })
        .collect()
}

/// Build Unicode block VU bar (Enhanced tier).
fn build_unicode_bar(
    progress: f32,
    peak: f32,
    syms: &SymbolSet,
    theme: &Theme,
    active: bool,
    complete: bool,
) -> Vec<Span<'static>> {
    let filled = (progress * VU_WIDTH as f32) as usize;
    let peak_pos = (peak * VU_WIDTH as f32) as usize;

    let bar_style = if complete {
        theme.success()
    } else if active {
        theme.warning()
    } else {
        theme.muted()
    };

    (0..VU_WIDTH)
        .map(|i| {
            let char = if i < filled {
                syms.vu_full
            } else if i == peak_pos && peak > progress {
                syms.vu_high
            } else {
                syms.vu_empty
            };

            let style = if i < filled || (i == peak_pos && peak > progress) {
                bar_style
            } else {
                theme.muted()
            };

            Span::styled(char.to_string(), style)
        })
        .collect()
}

/// Build ASCII VU bar (Fallback tier).
fn build_ascii_bar(
    progress: f32,
    peak: f32,
    theme: &Theme,
    active: bool,
    complete: bool,
) -> Vec<Span<'static>> {
    let filled = (progress * VU_WIDTH as f32) as usize;
    let peak_pos = (peak * VU_WIDTH as f32) as usize;

    let bar_style = if complete {
        theme.success()
    } else if active {
        theme.warning()
    } else {
        theme.muted()
    };

    let mut bar = String::new();
    for i in 0..VU_WIDTH {
        if i < filled {
            bar.push('#');
        } else if i == peak_pos && peak > progress {
            bar.push('=');
        } else {
            bar.push('-');
        }
    }

    vec![Span::styled(bar, bar_style)]
}

// ============================================================================
// Original Functions (Fallback)
// ============================================================================

/// Draw VU meters for compilation progress (ASCII fallback).
pub fn draw_vu_meters(f: &mut Frame, state: &VuMeterState, theme: &Theme, area: Rect) {
    draw_vu_meters_tiered(f, state, theme, CapabilityTier::Fallback, area)
}

/// Draw a simple progress bar (single line, no track styling).
pub fn draw_progress_bar(f: &mut Frame, label: &str, progress: f32, theme: &Theme, area: Rect) {
    draw_progress_bar_tiered(f, label, progress, theme, CapabilityTier::Fallback, area)
}

/// Draw a simple progress bar with tier-appropriate rendering.
pub fn draw_progress_bar_tiered(
    f: &mut Frame,
    label: &str,
    progress: f32,
    theme: &Theme,
    tier: CapabilityTier,
    area: Rect,
) {
    let syms = SymbolSet::for_tier(tier);
    let filled = (progress * VU_WIDTH as f32) as usize;

    let bar_spans: Vec<Span> = match tier {
        CapabilityTier::Premium => {
            (0..VU_WIDTH)
                .map(|i| {
                    let pos = i as f32 / VU_WIDTH as f32;
                    let is_filled = i < filled;
                    let char = if is_filled { "█" } else { " " };
                    let color = if is_filled { vu_gradient(pos) } else { theme.charcoal };
                    Span::styled(char.to_string(), Style::default().fg(color))
                })
                .collect()
        }
        CapabilityTier::Enhanced => {
            (0..VU_WIDTH)
                .map(|i| {
                    let char = if i < filled { syms.vu_full } else { syms.vu_empty };
                    Span::styled(char.to_string(), theme.accent())
                })
                .collect()
        }
        CapabilityTier::Fallback => {
            let mut bar = String::new();
            for i in 0..VU_WIDTH {
                bar.push(if i < filled { '#' } else { '-' });
            }
            vec![Span::styled(bar, theme.accent())]
        }
    };

    let percent = (progress * 100.0) as u8;

    let mut spans = vec![
        Span::styled(format!("{}: ", label), theme.text()),
        Span::styled(syms.border_vertical, theme.muted()),
    ];
    spans.extend(bar_spans);
    spans.push(Span::styled(syms.border_vertical, theme.muted()));
    spans.push(Span::styled(format!(" {}%", percent), theme.muted()));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}
