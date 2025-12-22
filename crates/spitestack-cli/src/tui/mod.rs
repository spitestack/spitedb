//! SpiteStack TUI.
//!
//! Long Island Grit - a Glassjaw-inspired terminal interface.
//! SpiteStack - Code Angry.
#![allow(dead_code, unused_imports, unused_variables, unused_assignments)]

pub mod app;
pub mod audio;
pub mod capabilities;
pub mod commands;
pub mod event;
pub mod render;
pub mod terminal;
pub mod theme;
pub mod widgets;

use std::io::Write;
use std::time::{Duration, Instant};

use crate::tui::app::App;
use crate::tui::capabilities::{CapabilityTier, SyncGuard};
use crate::tui::event::{handle_event, handle_task_result, EventHandler, EventResult};
use crate::tui::terminal::TuiContext;
use crate::tui::theme::Theme;
use crate::tui::widgets::{draw_dashboard, draw_splash};

/// Run the TUI application.
pub async fn run() -> miette::Result<()> {
    run_with_options(None).await
}

/// Run the TUI application with a graphics tier override.
pub async fn run_with_options(graphics_override: Option<&str>) -> miette::Result<()> {
    // Install panic hook
    terminal::install_panic_hook();

    // Initialize terminal with capability detection
    let mut ctx = terminal::init(graphics_override)
        .map_err(|e| miette::miette!("failed to initialize terminal: {}", e))?;

    // Get detected tier
    let tier = ctx.tier();

    // Create app state
    let mut app = App::new();
    app.splash_start = Some(Instant::now());

    // Detect project
    detect_project(&mut app);

    // Create theme
    let theme = Theme::default();

    // Run the app
    let result = run_app(&mut app, &mut ctx, &theme).await;

    // Restore terminal
    terminal::restore()
        .map_err(|e| miette::miette!("failed to restore terminal: {}", e))?;

    result
}

/// Detect if we're in a SpiteStack project.
fn detect_project(app: &mut App) {
    // Look for domain directory
    let cwd = std::env::current_dir().unwrap_or_default();

    // Check common domain paths
    let candidates = [
        cwd.join("src/domain"),
        cwd.join("domain"),
    ];

    for candidate in candidates {
        if candidate.exists() && candidate.is_dir() {
            app.project.root = Some(cwd.clone());
            app.project.domain_dir = candidate;
            app.project.output_dir = cwd.join(".spitestack");

            // Try to get project name from directory
            app.project.name = cwd
                .file_name()
                .map(|s| s.to_string_lossy().to_string());

            app.log_info(format!("project: {}", app.project.name.as_deref().unwrap_or("unknown")));
            return;
        }
    }

    // No project found
    app.log_info("no project detected. use /init to create one.");
}

/// Main application loop.
async fn run_app(app: &mut App, ctx: &mut TuiContext, theme: &Theme) -> miette::Result<()> {
    let tier = ctx.tier();
    let use_sync = ctx.use_sync_rendering();

    // Tick rate based on capability tier
    let tick_rate = Duration::from_millis(tier.frame_duration_ms());

    let (event_handler, mut event_rx) = EventHandler::new(tick_rate);
    let _event_task = event_handler.spawn();

    loop {
        // Synchronized rendering for Premium tier
        // This prevents flicker by batching all output until the frame is complete
        {
            let mut stdout = std::io::stdout();
            let _sync_guard = if use_sync {
                Some(SyncGuard::new(&mut stdout, true).map_err(|e| miette::miette!("sync error: {}", e))?)
            } else {
                None
            };

            // Render frame
            ctx.terminal.draw(|f| {
                let area = f.area();

                // Clear with void background
                let bg = ratatui::widgets::Block::default()
                    .style(ratatui::style::Style::default().bg(theme.void));
                f.render_widget(bg, area);

                if app.show_splash {
                    draw_splash(f, theme, tier, area);
                } else {
                    draw_dashboard(f, app, theme, tier, area);
                }
            })
            .map_err(|e| miette::miette!("render error: {}", e))?;
        }

        // Handle events
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match handle_event(app, event).await {
                    EventResult::Continue => {}
                    EventResult::Quit => break,
                }
            }
            Some(task_result) = app.task_rx.recv() => {
                handle_task_result(app, task_result).await;
            }
        }

        // Check quit flag
        if app.should_quit {
            break;
        }
    }

    Ok(())
}
