//! Terminal setup and teardown.
//!
//! SpiteStack - Code Angry.
//!
//! Handles raw mode, alternate screen, capability detection, and cleanup.

use std::io::{self, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::tui::capabilities::{detect_capabilities, TerminalCapabilities};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Terminal context with capabilities.
///
/// Holds both the ratatui Terminal and detected capabilities.
pub struct TuiContext {
    /// The ratatui terminal instance.
    pub terminal: Tui,
    /// Detected terminal capabilities.
    pub capabilities: TerminalCapabilities,
}

impl TuiContext {
    /// Get the capability tier.
    pub fn tier(&self) -> crate::tui::capabilities::CapabilityTier {
        self.capabilities.tier
    }

    /// Check if synchronized rendering should be used.
    pub fn use_sync_rendering(&self) -> bool {
        self.capabilities.synchronized_output
    }
}

/// Initialize the terminal for TUI mode.
///
/// Detects capabilities before entering alternate screen.
/// Accepts an optional CLI override for the graphics tier.
pub fn init(cli_graphics_override: Option<&str>) -> io::Result<TuiContext> {
    // Detect capabilities BEFORE entering alternate screen
    // (some detection may query the terminal)
    let capabilities = detect_capabilities(cli_graphics_override);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    Ok(TuiContext {
        terminal,
        capabilities,
    })
}

/// Initialize the terminal with default capability detection.
pub fn init_default() -> io::Result<TuiContext> {
    init(None)
}

/// Restore the terminal to normal mode.
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    Ok(())
}

/// Setup panic hook to restore terminal on panic.
pub fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore();
        original_hook(panic_info);
    }));
}
