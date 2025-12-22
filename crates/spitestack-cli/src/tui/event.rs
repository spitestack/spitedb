//! Event handling for the TUI.
//!
//! Handles keyboard input, tick events, and async task results.

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::tui::app::{App, AppMode, CompileSnapshot, CompilerStatus, DiagnosticEntry, OutputLevel, TaskResult};
use crate::tui::commands::get_suggestions;
use spite_compiler::{Compiler, CompilerConfig};

/// Application events.
#[derive(Debug)]
pub enum AppEvent {
    /// Keyboard input
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for updates
    Tick,
}

/// Result of handling an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    Continue,
    Quit,
}

/// Event handler that polls for terminal events.
pub struct EventHandler {
    tx: mpsc::Sender<AppEvent>,
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> (Self, mpsc::Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel(100);
        (Self { tx, tick_rate }, rx)
    }

    /// Spawn the event polling task.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        let tx = self.tx;
        let tick_rate = self.tick_rate;

        tokio::spawn(async move {
            let mut last_tick = std::time::Instant::now();

            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::ZERO);

                if event::poll(timeout).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if tx.send(AppEvent::Key(key)).await.is_err() {
                                break;
                            }
                        }
                        Ok(Event::Resize(w, h)) => {
                            if tx.send(AppEvent::Resize(w, h)).await.is_err() {
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                if last_tick.elapsed() >= tick_rate {
                    if tx.send(AppEvent::Tick).await.is_err() {
                        break;
                    }
                    last_tick = std::time::Instant::now();
                }
            }
        })
    }
}

/// Handle an application event.
pub async fn handle_event(app: &mut App, event: AppEvent) -> EventResult {
    match event {
        AppEvent::Key(key) => handle_key(app, key).await,
        AppEvent::Tick => {
            // Check if splash screen should end
            if app.show_splash {
                if let Some(start) = app.splash_start {
                    if start.elapsed() >= Duration::from_millis(600) {
                        app.show_splash = false;
                    }
                }
            }

            // Advance vinyl animation (SpiteStack Records)
            app.vinyl.tick();

            // Update VU meters based on compiler status
            app.vu_meters.update_for_status(&app.compiler.status);

            EventResult::Continue
        }
        AppEvent::Resize(_, _) => EventResult::Continue,
    }
}

/// Handle a key event.
async fn handle_key(app: &mut App, key: KeyEvent) -> EventResult {
    // Skip splash on any key
    if app.show_splash {
        app.show_splash = false;
        return EventResult::Continue;
    }

    // Global shortcuts
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => return EventResult::Quit,
            KeyCode::Char('l') => {
                app.output.clear();
                return EventResult::Continue;
            }
            _ => {}
        }
    }

    // Mode-specific handling
    match app.mode {
        AppMode::Dashboard => handle_dashboard_key(app, key).await,
        AppMode::Command => handle_command_key(app, key).await,
        AppMode::FixSelection => handle_fix_key(app, key).await,
        AppMode::ErrorDetail => handle_error_detail_key(app, key).await,
        AppMode::MusicMode => handle_music_mode_key(app, key).await,
        _ => EventResult::Continue,
    }
}

/// Handle key in dashboard mode.
async fn handle_dashboard_key(app: &mut App, key: KeyEvent) -> EventResult {
    match key.code {
        KeyCode::Char('/') => {
            app.mode = AppMode::Command;
            app.input.clear();
            EventResult::Continue
        }
        KeyCode::Char('q') => EventResult::Quit,
        // 'M' - Enter music mode (SpiteStack Records)
        KeyCode::Char('m') | KeyCode::Char('M') => {
            app.mode = AppMode::MusicMode;
            EventResult::Continue
        }
        KeyCode::Char('e') => {
            // Toggle error detail view if there are errors
            if !app.errors.is_empty() {
                app.fix_context = Some(crate::tui::app::FixContext {
                    errors: app.errors.clone(),
                    selected_index: 0,
                });
                app.mode = AppMode::ErrorDetail;
            }
            EventResult::Continue
        }
        KeyCode::Up => {
            if app.output.scroll_offset < app.output.lines.len().saturating_sub(1) {
                app.output.scroll_offset += 1;
                app.output.follow_tail = false;
            }
            EventResult::Continue
        }
        KeyCode::Down => {
            if app.output.scroll_offset > 0 {
                app.output.scroll_offset -= 1;
            }
            if app.output.scroll_offset == 0 {
                app.output.follow_tail = true;
            }
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

/// Handle key in command mode.
async fn handle_command_key(app: &mut App, key: KeyEvent) -> EventResult {
    match key.code {
        KeyCode::Esc => {
            if app.input.show_suggestions {
                // First Esc hides suggestions
                app.input.show_suggestions = false;
            } else {
                // Second Esc exits command mode
                app.mode = AppMode::Dashboard;
                app.input.clear();
            }
            EventResult::Continue
        }
        KeyCode::Tab => {
            // Cycle through suggestions
            let suggestions = get_suggestions(&app.input.buffer);
            if !suggestions.is_empty() && app.input.show_suggestions {
                app.input.selected_suggestion =
                    (app.input.selected_suggestion + 1) % suggestions.len();
            }
            EventResult::Continue
        }
        KeyCode::Enter => {
            // If showing suggestions, accept the selected one
            if app.input.show_suggestions {
                let suggestions = get_suggestions(&app.input.buffer);
                if !suggestions.is_empty() {
                    let cmd = suggestions[app.input.selected_suggestion.min(suggestions.len() - 1)];
                    app.input.buffer = format!("/{}", cmd.name);
                    app.input.cursor = app.input.buffer.len();
                    app.input.show_suggestions = false;
                    app.input.selected_suggestion = 0;
                    return EventResult::Continue;
                }
            }
            // Execute the command
            let command = app.input.buffer.clone();
            app.input.push_history();
            app.input.clear();
            app.mode = AppMode::Dashboard;
            execute_command(app, &command).await;
            EventResult::Continue
        }
        KeyCode::Backspace => {
            app.input.backspace();
            update_suggestions(app);
            EventResult::Continue
        }
        KeyCode::Delete => {
            app.input.delete();
            update_suggestions(app);
            EventResult::Continue
        }
        KeyCode::Left => {
            app.input.move_left();
            EventResult::Continue
        }
        KeyCode::Right => {
            app.input.move_right();
            EventResult::Continue
        }
        KeyCode::Home => {
            app.input.move_start();
            EventResult::Continue
        }
        KeyCode::End => {
            app.input.move_end();
            EventResult::Continue
        }
        KeyCode::Up => {
            if app.input.show_suggestions {
                // Navigate suggestions
                let suggestions = get_suggestions(&app.input.buffer);
                if !suggestions.is_empty() && app.input.selected_suggestion > 0 {
                    app.input.selected_suggestion -= 1;
                }
            } else {
                app.input.history_up();
            }
            EventResult::Continue
        }
        KeyCode::Down => {
            if app.input.show_suggestions {
                // Navigate suggestions
                let suggestions = get_suggestions(&app.input.buffer);
                if !suggestions.is_empty()
                    && app.input.selected_suggestion < suggestions.len() - 1
                {
                    app.input.selected_suggestion += 1;
                }
            } else {
                app.input.history_down();
            }
            EventResult::Continue
        }
        KeyCode::Char(c) => {
            app.input.insert(c);
            update_suggestions(app);
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

/// Update autocomplete suggestions based on current input.
fn update_suggestions(app: &mut App) {
    if app.input.buffer.starts_with('/') {
        let suggestions = get_suggestions(&app.input.buffer);
        app.input.show_suggestions = !suggestions.is_empty();
        // Reset selection when suggestions change
        app.input.selected_suggestion = 0;
    } else {
        app.input.show_suggestions = false;
    }
}

/// Handle key in fix selection mode.
async fn handle_fix_key(app: &mut App, key: KeyEvent) -> EventResult {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.fix_context = None;
            app.mode = AppMode::Dashboard;
            EventResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut ctx) = app.fix_context {
                if ctx.selected_index > 0 {
                    ctx.selected_index -= 1;
                }
            }
            EventResult::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut ctx) = app.fix_context {
                if ctx.selected_index < ctx.errors.len().saturating_sub(1) {
                    ctx.selected_index += 1;
                }
            }
            EventResult::Continue
        }
        KeyCode::Enter | KeyCode::Char('f') => {
            // Apply fix for selected error
            // TODO: implement fix application
            app.log_info("fix not yet implemented");
            EventResult::Continue
        }
        KeyCode::Char('i') => {
            // Ignore this error
            if let Some(ref mut ctx) = app.fix_context {
                if !ctx.errors.is_empty() {
                    ctx.errors.remove(ctx.selected_index);
                    if ctx.selected_index >= ctx.errors.len() && ctx.selected_index > 0 {
                        ctx.selected_index -= 1;
                    }
                    if ctx.errors.is_empty() {
                        app.fix_context = None;
                        app.mode = AppMode::Dashboard;
                    }
                }
            }
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

/// Handle key in error detail mode.
async fn handle_error_detail_key(app: &mut App, key: KeyEvent) -> EventResult {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.fix_context = None;
            app.mode = AppMode::Dashboard;
            EventResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut ctx) = app.fix_context {
                if ctx.selected_index > 0 {
                    ctx.selected_index -= 1;
                }
            }
            EventResult::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut ctx) = app.fix_context {
                if ctx.selected_index < ctx.errors.len().saturating_sub(1) {
                    ctx.selected_index += 1;
                }
            }
            EventResult::Continue
        }
        KeyCode::Char('f') => {
            // Switch to fix mode
            app.mode = AppMode::FixSelection;
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

/// Handle key in music mode (SpiteStack Records).
async fn handle_music_mode_key(app: &mut App, key: KeyEvent) -> EventResult {
    match key.code {
        // Exit music mode
        KeyCode::Esc | KeyCode::Char('m') | KeyCode::Char('M') | KeyCode::Char('q') => {
            app.mode = AppMode::Dashboard;
            EventResult::Continue
        }
        // Open Spotify
        KeyCode::Char('s') | KeyCode::Char('S') => {
            let url = &app.music_mode.playlist_urls.spotify;
            let _ = open::that(url);
            EventResult::Continue
        }
        // Open Apple Music
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let url = &app.music_mode.playlist_urls.apple_music;
            let _ = open::that(url);
            EventResult::Continue
        }
        // Navigate up
        KeyCode::Up | KeyCode::Char('k') => {
            if app.music_mode.selected_link > 0 {
                app.music_mode.selected_link -= 1;
            }
            EventResult::Continue
        }
        // Navigate down
        KeyCode::Down | KeyCode::Char('j') => {
            if app.music_mode.selected_link < 1 {
                app.music_mode.selected_link += 1;
            }
            EventResult::Continue
        }
        // Open selected link
        KeyCode::Enter => {
            let url = match app.music_mode.selected_link {
                0 => &app.music_mode.playlist_urls.spotify,
                _ => &app.music_mode.playlist_urls.apple_music,
            };
            let _ = open::that(url);
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

/// Execute a slash command.
async fn execute_command(app: &mut App, command: &str) {
    // Strip leading / in case user typed /mix instead of mix
    let command = command.trim().trim_start_matches('/');

    // Parse command and args
    let parts: Vec<&str> = command.split_whitespace().collect();
    let (cmd, _args) = match parts.split_first() {
        Some((cmd, args)) => (*cmd, args),
        None => return,
    };

    // SpiteStack Records - Recording Studio Commands
    match cmd {
        // /mix - Mix your tracks (compile)
        "mix" | "compile" | "c" => {
            execute_compile(app).await;
        }
        // /play - Playback mode (dev server)
        "play" | "dev" | "d" | "p" => {
            execute_dev(app).await;
        }
        // /stop - Stop the track
        "stop" | "s" => {
            if app.watcher.active {
                app.watcher.active = false;
                app.vinyl.spinning = false;
                app.log_info("stopped.");
                app.mode = AppMode::Dashboard;
            } else {
                app.log_info("nothing to stop.");
            }
        }
        // /remix - Fix and rebuild
        "remix" | "fix" | "f" | "r" => {
            if app.errors.is_empty() {
                app.log_info("no errors to remix");
            } else {
                app.fix_context = Some(crate::tui::app::FixContext {
                    errors: app.errors.clone(),
                    selected_index: 0,
                });
                app.mode = AppMode::FixSelection;
            }
        }
        // /record - Start a new session (init)
        "record" | "rec" | "init" | "i" => {
            app.log_info("record not yet implemented");
        }
        // /master - Production build
        "master" | "prod" => {
            app.log_info("mastering not yet implemented");
        }
        "clear" => {
            app.output.clear();
        }
        "quit" | "q" => {
            app.should_quit = true;
        }
        "help" | "h" | "?" => {
            app.log_info("");
            app.log_info("SPITESTACK RECORDS - Command Reference");
            app.log_info("");
            app.log_info("recording:");
            app.log_info("  /mix      - mix your domain (compile)");
            app.log_info("  /play     - playback mode (dev server)");
            app.log_info("  /stop     - stop the track");
            app.log_info("  /remix    - fix errors");
            app.log_info("  /record   - start new session (init)");
            app.log_info("  /master   - production build");
            app.log_info("");
            app.log_info("session:");
            app.log_info("  /clear    - clear output");
            app.log_info("  /quit     - exit studio");
            app.log_info("");
            app.log_info("shortcuts:");
            app.log_info("  /       - enter command mode");
            app.log_info("  m       - music mode");
            app.log_info("  e       - view errors");
            app.log_info("  q       - quit");
        }
        _ => {
            app.log_error(format!("unknown track: {}", cmd));
        }
    }
}

/// Execute the /dev command.
async fn execute_dev(app: &mut App) {
    // Check if we have a project
    if app.project.root.is_none() {
        app.log_error("no project. use /init first.");
        return;
    }

    // Initial compile
    execute_compile(app).await;

    if !app.errors.is_empty() {
        app.log_error("fix errors before starting dev mode.");
        return;
    }

    app.mode = AppMode::DevServer;
    app.watcher.active = true;
    app.watcher.pending_changes = 0;

    app.log_info("watching for changes...");
    app.log_info("use /stop to exit dev mode.");
}

/// Execute the /compile command.
async fn execute_compile(app: &mut App) {
    // Check if we have a project
    if app.project.root.is_none() {
        app.log_error("no project. use /init first.");
        return;
    }

    app.log_info("compiling...");
    app.mode = AppMode::Compiling;
    app.compiler.status = CompilerStatus::Parsing;

    let domain_dir = app.project.domain_dir.clone();
    let output_dir = app.project.output_dir.clone();
    let project_name = app.project.name.clone().unwrap_or_else(|| "app".to_string());

    let start = Instant::now();

    let config = CompilerConfig {
        domain_dir: domain_dir.clone(),
        out_dir: output_dir.clone(),
        skip_purity_check: false,
        language: "typescript".to_string(),
    };

    let compiler = Compiler::new(config);

    match compiler.compile_project(&project_name, 3000).await {
        Ok(result) => {
            let duration = start.elapsed().as_millis();

            app.compiler.status = CompilerStatus::Complete;
            app.log_success(format!(
                "compiled {} aggregates in {}ms",
                result.aggregates, duration
            ));

            app.project.last_compile = Some(CompileSnapshot {
                timestamp: Instant::now(),
                duration_ms: duration,
                aggregates: result.aggregates,
                orchestrators: result.orchestrators,
                events: result.events,
                success: true,
            });

            app.errors.clear();
        }
        Err(e) => {
            app.compiler.status = CompilerStatus::Error;
            app.log_error(format!("{}", e));

            // SpiteStack Records: Scratch the record on error!
            app.vinyl.trigger_scratch();

            // Play scratch sound if audio enabled
            if let Some(ref player) = app.audio.player {
                player.play_scratch();
            }

            // Try to extract diagnostic info
            let diagnostic = DiagnosticEntry {
                message: format!("{}", e),
                code: None,
                file: None,
                line: None,
                column: None,
                snippet: None,
                help: None,
                fix: None,
            };
            app.errors = vec![diagnostic];
        }
    }

    app.mode = AppMode::Dashboard;
    app.compiler.status = CompilerStatus::Idle;
}

/// Handle task result from async operations.
pub async fn handle_task_result(app: &mut App, result: TaskResult) {
    match result {
        TaskResult::CompileComplete {
            success,
            duration_ms,
            aggregates,
            events,
            errors,
        } => {
            app.compiler.status = if success {
                crate::tui::app::CompilerStatus::Complete
            } else {
                crate::tui::app::CompilerStatus::Error
            };

            if success {
                app.log_success(format!(
                    "compiled {} aggregates, {} events in {}ms",
                    aggregates, events, duration_ms
                ));
                app.project.last_compile = Some(crate::tui::app::CompileSnapshot {
                    timestamp: std::time::Instant::now(),
                    duration_ms,
                    aggregates,
                    orchestrators: 0,
                    events,
                    success: true,
                });
            } else {
                app.log_error("compilation failed");
            }

            app.errors = errors;
            app.mode = AppMode::Dashboard;
            app.compiler.status = crate::tui::app::CompilerStatus::Idle;
        }
        TaskResult::FileChanged(paths) => {
            app.watcher.pending_changes += paths.len();
            app.watcher.last_event = Some(std::time::Instant::now());
            for path in paths {
                app.log_info(format!("› changed: {}", path.display()));
            }
        }
        TaskResult::DevServerStarted { port } => {
            app.log_success(format!("server running on :{}", port));
        }
        TaskResult::DevServerStopped => {
            app.log_info("server stopped");
            app.watcher.active = false;
            app.mode = AppMode::Dashboard;
        }
        TaskResult::FixApplied { file, success } => {
            if success {
                app.log_success(format!("fixed: {}", file.display()));
            } else {
                app.log_error(format!("failed to fix: {}", file.display()));
            }
        }
    }
}
