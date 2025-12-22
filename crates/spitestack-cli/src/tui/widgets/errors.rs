//! Error list widget.
//!
//! × ForbiddenCall
//!   Todo/aggregate.ts:24
//!   † fix available

use ratatui::{
    layout::Rect,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::app::{App, DiagnosticEntry};
use crate::tui::capabilities::CapabilityTier;
use crate::tui::theme::{symbols, SymbolSet, Theme};

/// Draw the error list panel with tier-appropriate rendering.
pub fn draw_errors(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let syms = SymbolSet::for_tier(tier);

    // Use rounded borders for Premium tier
    let border_set = match tier {
        CapabilityTier::Premium => border::ROUNDED,
        _ => border::PLAIN,
    };
    let block = Block::default()
        .title(Span::styled("ERRORS", theme.header()))
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(theme.border());

    if app.errors.is_empty() {
        // No errors - show empty state
        let inner = block.inner(area);
        f.render_widget(block, area);

        let msg = Paragraph::new(Line::from(vec![
            Span::styled(syms.dot, theme.muted()),
            Span::styled(" clean", theme.muted()),
        ]));
        f.render_widget(msg, inner);
    } else {
        // Show error list with tiered symbols
        let items: Vec<ListItem> = app
            .errors
            .iter()
            .flat_map(|e| error_to_list_items(e, theme, &syms))
            .collect();

        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}

/// Draw the error detail view with tier-appropriate rendering.
pub fn draw_error_detail(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let syms = SymbolSet::for_tier(tier);

    let ctx = match &app.fix_context {
        Some(ctx) => ctx,
        None => return,
    };

    let error = match ctx.errors.get(ctx.selected_index) {
        Some(e) => e,
        None => return,
    };

    // Use rounded borders for Premium tier
    let border_set = match tier {
        CapabilityTier::Premium => border::ROUNDED,
        _ => border::PLAIN,
    };

    let block = Block::default()
        .title(Span::styled(
            format!("{} {}", syms.cross, error.code.as_deref().unwrap_or("ERROR")),
            theme.error(),
        ))
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(theme.border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build content
    let mut lines = Vec::new();

    // Empty line
    lines.push(Line::from(""));

    // Code snippet
    if let Some(ref snippet) = error.snippet {
        lines.push(Line::from(vec![
            Span::styled("   ", theme.text()),
            Span::styled(snippet.as_str(), theme.text()),
        ]));
        // Underline
        let underline = "~".repeat(snippet.len().min(40));
        lines.push(Line::from(vec![
            Span::styled("   ", theme.text()),
            Span::styled(underline, theme.error()),
        ]));
    }

    // Empty line
    lines.push(Line::from(""));

    // Location
    if let Some(ref file) = error.file {
        let location = format!(
            "{}:{}:{}",
            file.display(),
            error.line.unwrap_or(0),
            error.column.unwrap_or(0)
        );
        lines.push(Line::from(vec![
            Span::styled("   ", theme.text()),
            Span::styled(location, theme.muted()),
        ]));
    }

    // Empty line
    lines.push(Line::from(""));

    // Help text
    if let Some(ref help) = error.help {
        lines.push(Line::from(vec![
            Span::styled("   ", theme.text()),
            Span::styled(help.as_str(), theme.text()),
        ]));
    }

    // Empty line
    lines.push(Line::from(""));

    // Actions
    let has_fix = error.fix.is_some();
    let mut actions = Vec::new();
    actions.push(Span::styled("   ", theme.text()));
    if has_fix {
        actions.push(Span::styled("[f]", theme.accent()));
        actions.push(Span::styled("ix  ", theme.muted()));
    }
    actions.push(Span::styled("[i]", theme.accent()));
    actions.push(Span::styled("gnore  ", theme.muted()));
    actions.push(Span::styled("[q]", theme.accent()));
    actions.push(Span::styled("uit", theme.muted()));
    lines.push(Line::from(actions));

    let content = Paragraph::new(lines);
    f.render_widget(content, inner);
}

/// Convert a diagnostic entry to list items with tiered symbols.
fn error_to_list_items<'a>(error: &'a DiagnosticEntry, theme: &'a Theme, syms: &SymbolSet) -> Vec<ListItem<'a>> {
    let mut items = Vec::new();

    // Error type line
    let error_line = Line::from(vec![
        Span::styled(syms.cross, theme.error()),
        Span::styled(" ", theme.text()),
        Span::styled(&error.message, theme.text()),
    ]);
    items.push(ListItem::new(error_line));

    // Location line
    if let Some(ref file) = error.file {
        let location = format!(
            "  {}:{}",
            file.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            error.line.unwrap_or(0)
        );
        items.push(ListItem::new(Line::from(vec![Span::styled(
            location,
            theme.muted(),
        )])));
    }

    // Fix available indicator
    if error.fix.is_some() {
        let fix_line = Line::from(vec![
            Span::styled("  ", theme.text()),
            Span::styled(syms.dagger, theme.warning()),
            Span::styled(" fix available", theme.muted()),
        ]);
        items.push(ListItem::new(fix_line));
    }

    // Empty line for spacing
    items.push(ListItem::new(Line::from("")));

    items
}
