//! Status bar widget.
//!
//! SpiteStack Records - "Code Angry"
//!
//! ◉ SPITESTACK RECORDS                  project | aggregates | state

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::capabilities::CapabilityTier;
use crate::tui::theme::{symbols, SymbolSet, Theme};

/// Draw the status bar with tier-appropriate rendering.
pub fn draw_status(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let syms = SymbolSet::for_tier(tier);
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(theme.border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into left and right sections
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Left: Vinyl + SPITESTACK RECORDS
    // Get vinyl animation frame - use tiered symbols
    let vinyl_char = if app.vinyl.scratching {
        symbols::VINYL_SCRATCH[app.vinyl.scratch_frames % 3]
    } else if app.vinyl.spinning {
        if tier.supports_unicode() {
            syms.record
        } else {
            symbols::VINYL_FRAMES[app.vinyl.frame % 4]
        }
    } else {
        if tier.supports_unicode() {
            syms.record
        } else {
            symbols::VINYL
        }
    };

    let vinyl_style = if app.vinyl.scratching {
        theme.error()
    } else if app.vinyl.spinning {
        theme.accent()
    } else {
        theme.muted()
    };

    let title = Paragraph::new(Line::from(vec![
        Span::styled(vinyl_char, vinyl_style),
        Span::styled(" ", theme.text()),
        Span::styled("SPITESTACK ", theme.header()),
        Span::styled("RECORDS", theme.accent()),
    ]));
    f.render_widget(title, chunks[0]);

    // Right: project | aggregates | state
    let mut status_parts = Vec::new();

    // Project name
    if let Some(ref name) = app.project.name {
        status_parts.push(Span::styled(name.as_str(), theme.text()));
    } else {
        status_parts.push(Span::styled("no project", theme.muted()));
    }

    status_parts.push(Span::styled(format!(" {} ", syms.pipe), theme.muted()));

    // Aggregate count
    if let Some(ref snapshot) = app.project.last_compile {
        status_parts.push(Span::styled(
            format!("{} agg", snapshot.aggregates),
            theme.text(),
        ));
    } else {
        status_parts.push(Span::styled("- agg", theme.muted()));
    }

    status_parts.push(Span::styled(format!(" {} ", syms.pipe), theme.muted()));

    // Compiler state
    let state_style = match app.compiler.status {
        crate::tui::app::CompilerStatus::Idle => theme.muted(),
        crate::tui::app::CompilerStatus::Error => theme.error(),
        crate::tui::app::CompilerStatus::Complete => theme.success(),
        _ => theme.warning(),
    };
    status_parts.push(Span::styled(app.compiler.status.as_str(), state_style));

    // Watcher indicator
    if app.watcher.active {
        status_parts.push(Span::styled(format!(" {} ", syms.pipe), theme.muted()));
        status_parts.push(Span::styled(syms.dot, theme.success()));
    }

    let status = Paragraph::new(Line::from(status_parts))
        .alignment(ratatui::layout::Alignment::Right);
    f.render_widget(status, chunks[1]);
}
