//! Command input widget with autocomplete.
//!
//! SpiteStack - Code Angry.
//!
//! Clear, human-centered command input area:
//! - Full border container for visual clarity
//! - "COMMAND" title to identify purpose
//! - Highlighted hint showing how to activate
//! - Focus indicator with border color change
//! - Autocomplete dropdown when typing "/"
//!
//! ```text
//! ╭──────────────────────────────────────╮
//! │ /mix      mix your domain (compile)  │  ← Dropdown
//! │ /master   production build           │
//! ╰──────────────────────────────────────╯
//! ╭─ COMMAND ────────────────────────────╮
//! │ › /m█                                │  ← Input
//! ╰──────────────────────────────────────╯
//! ```

use ratatui::{
    layout::Rect,
    style::Style,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::app::{App, AppMode};
use crate::tui::capabilities::CapabilityTier;
use crate::tui::commands::get_suggestions;
use crate::tui::theme::{SymbolSet, Theme};

/// Draw the command input with tier-appropriate rendering.
///
/// Human-centered design improvements:
/// - Full border container (not just top line) for clear visual separation
/// - "COMMAND" title to clarify the widget's purpose
/// - Border color changes when focused (accent vs muted)
/// - Better hint text with highlighted key
/// - Premium tier gets subtle background highlight on focus
pub fn draw_input(f: &mut Frame, app: &App, theme: &Theme, tier: CapabilityTier, area: Rect) {
    let syms = SymbolSet::for_tier(tier);
    let is_active = matches!(app.mode, AppMode::Command);

    // Use rounded borders for Premium tier
    let border_set = match tier {
        CapabilityTier::Premium => border::ROUNDED,
        _ => border::PLAIN,
    };

    // Build the container block
    let mut block = Block::default()
        .title(Span::styled(
            " COMMAND ",
            if is_active { theme.accent() } else { theme.muted() },
        ))
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(if is_active { theme.accent() } else { theme.border() });

    // Premium tier: subtle background highlight when active
    if is_active && tier == CapabilityTier::Premium {
        block = block.style(Style::default().bg(theme.charcoal));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build input line
    let mut spans = Vec::new();

    // Prompt - different for active vs inactive
    let prompt = if is_active {
        Span::styled(syms.arrow, theme.accent()) // Active: bright arrow
    } else {
        Span::styled("/", theme.muted()) // Inactive: subtle slash
    };
    spans.push(prompt);
    spans.push(Span::styled(" ", theme.text()));

    // Input text with cursor
    if is_active {
        let (before, after) = app.input.buffer.split_at(app.input.cursor);
        spans.push(Span::styled(before, theme.text()));

        // Cursor - block cursor
        if after.is_empty() {
            spans.push(Span::styled("█", theme.text()));
        } else {
            let mut chars = after.chars();
            let cursor_char = chars.next().unwrap();
            spans.push(Span::styled(
                cursor_char.to_string(),
                Style::default().fg(theme.void).bg(theme.bone),
            ));
            spans.push(Span::styled(chars.as_str(), theme.text()));
        }
    } else {
        // Inactive - show helpful hint with highlighted key
        if app.input.buffer.is_empty() {
            spans.push(Span::styled("press ", theme.muted()));
            spans.push(Span::styled("/", theme.accent())); // Highlight the key
            spans.push(Span::styled(" to enter command", theme.muted()));
        } else {
            spans.push(Span::styled(&app.input.buffer, theme.text()));
        }
    }

    let input = Paragraph::new(Line::from(spans));
    f.render_widget(input, inner);

    // Draw autocomplete dropdown if showing suggestions
    if is_active && app.input.show_suggestions {
        let suggestions = get_suggestions(&app.input.buffer);
        if !suggestions.is_empty() {
            draw_suggestions_dropdown(f, &suggestions, app.input.selected_suggestion, theme, tier, area);
        }
    }
}

/// Draw the autocomplete suggestions dropdown above the input.
fn draw_suggestions_dropdown(
    f: &mut Frame,
    suggestions: &[&crate::tui::commands::CommandDef],
    selected: usize,
    theme: &Theme,
    tier: CapabilityTier,
    input_area: Rect,
) {
    // Calculate dropdown dimensions
    let max_items = 6;
    let visible_count = suggestions.len().min(max_items);
    let dropdown_height = (visible_count as u16) + 2; // +2 for borders

    // Position dropdown above the input
    let dropdown_area = Rect {
        x: input_area.x,
        y: input_area.y.saturating_sub(dropdown_height),
        width: input_area.width.min(50),
        height: dropdown_height,
    };

    // Use tier-appropriate borders
    let border_set = match tier {
        CapabilityTier::Premium => border::ROUNDED,
        _ => border::PLAIN,
    };

    // Clear the area first (overlay effect)
    f.render_widget(Clear, dropdown_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(theme.border())
        .style(Style::default().bg(theme.void));

    let inner = block.inner(dropdown_area);
    f.render_widget(block, dropdown_area);

    // Build list items
    let items: Vec<ListItem> = suggestions
        .iter()
        .take(max_items)
        .enumerate()
        .map(|(i, cmd)| {
            let is_selected = i == selected;

            let style = if is_selected {
                theme.selected()
            } else {
                theme.text()
            };

            let desc_style = if is_selected {
                Style::default().fg(theme.ash).bg(theme.maroon)
            } else {
                theme.muted()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("/{:<10}", cmd.name), style),
                Span::styled(cmd.description, desc_style),
            ]))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}
