//! Terminal capability detection and management.
//!
//! SpiteStack - Code Angry.
//!
//! This module provides:
//! - Terminal capability detection (tier.rs)
//! - Environment-based detection with manual override (detect.rs)
//! - Synchronized rendering for flicker-free animation (sync.rs)

mod detect;
mod sync;
mod tier;

pub use detect::{detect_capabilities, detect_tier};
pub use sync::{begin_sync_update, end_sync_update, with_sync, SyncGuard};
pub use tier::{CapabilityTier, TerminalCapabilities, UnicodeLevel};
