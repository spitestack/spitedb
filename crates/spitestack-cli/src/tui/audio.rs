//! Audio playback for TUI feedback.
//!
//! Audio support is currently disabled to keep cross-platform builds simple.

/// Audio player handle (disabled).
#[derive(Debug)]
pub struct AudioPlayer;

impl AudioPlayer {
    /// Audio is disabled in this build.
    pub fn new() -> Option<Self> {
        None
    }

    /// No-op when audio is disabled.
    pub fn play_scratch(&self) {}
}
