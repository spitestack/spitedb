//! Output panel widget.
//!
//! Scrollable log of compiler output.
//! ✓ compiled 3 aggregates in 42ms
//! › server running on :3000
//! · watching for changes...

use ratatui::{
    layout::Rect,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, OutputLevel};
use crate::tui::capabilities::CapabilityTier;
use crate::tui::theme::{symbols, SymbolSet, Theme};

/// Draw the output panel with tier-appropriate rendering.
pub fn draw_output(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let syms = SymbolSet::for_tier(tier);

    // Use rounded borders for Premium tier
    let border_set = match tier {
        CapabilityTier::Premium => border::ROUNDED,
        _ => border::PLAIN,
    };

    let block = Block::default()
        .title(Span::styled("OUTPUT", theme.header()))
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(theme.border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.output.lines.is_empty() {
        // Empty state
        let msg = Paragraph::new(Line::from(vec![
            Span::styled(syms.dot, theme.muted()),
            Span::styled(" ready", theme.muted()),
        ]));
        f.render_widget(msg, inner);
        return;
    }

    // Calculate visible lines
    let visible_height = inner.height as usize;
    let total_lines = app.output.lines.len();
    let start = if total_lines > visible_height {
        total_lines - visible_height - app.output.scroll_offset
    } else {
        0
    };
    let end = (start + visible_height).min(total_lines);

    // Build lines with tiered symbols
    let lines: Vec<Line> = app
        .output
        .lines
        .iter()
        .skip(start)
        .take(end - start)
        .map(|line| {
            let (prefix, style) = match line.level {
                OutputLevel::Info => (syms.arrow, theme.text()),
                OutputLevel::Success => (syms.check, theme.success()),
                OutputLevel::Warning => (syms.dagger, theme.warning()),
                OutputLevel::Error => (syms.cross, theme.error()),
                OutputLevel::Debug => (syms.dot, theme.muted()),
            };

            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(" ", theme.text()),
                Span::styled(&line.content, style),
            ])
        })
        .collect();

    let content = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(content, inner);
}
