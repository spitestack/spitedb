//! Synchronized rendering for flicker-free animation.
//!
//! SpiteStack - Code Angry.
//!
//! Uses DCS (Device Control String) sequences to batch frame updates.
//! Supported by Ghostty, Kitty, and other modern terminals.
//!
//! This prevents the "tearing" effect where partial frames are visible.

use std::io::{self, Write};

/// DCS sequence to begin synchronized update.
/// CSI ? 2026 h
const BEGIN_SYNC: &[u8] = b"\x1b[?2026h";

/// DCS sequence to end synchronized update.
/// CSI ? 2026 l
const END_SYNC: &[u8] = b"\x1b[?2026l";

/// Begin a synchronized update.
///
/// All output after this call will be buffered until `end_sync_update` is called.
/// The terminal will then render the entire buffer atomically.
pub fn begin_sync_update<W: Write>(writer: &mut W) -> io::Result<()> {
    writer.write_all(BEGIN_SYNC)
}

/// End a synchronized update.
///
/// The terminal will now render all buffered output atomically.
pub fn end_sync_update<W: Write>(writer: &mut W) -> io::Result<()> {
    writer.write_all(END_SYNC)?;
    writer.flush()
}

/// RAII guard for synchronized updates.
///
/// Automatically ends the synchronized update when dropped.
///
/// # Example
///
/// ```ignore
/// let mut stdout = io::stdout();
/// {
///     let _sync = SyncGuard::new(&mut stdout, true)?;
///     // All rendering here is batched
///     write!(stdout, "...")?;
/// } // Sync ends here, frame is rendered atomically
/// ```
pub struct SyncGuard<'a, W: Write> {
    writer: &'a mut W,
    enabled: bool,
}

impl<'a, W: Write> SyncGuard<'a, W> {
    /// Create a new sync guard.
    ///
    /// If `enabled` is false, this is a no-op (for terminals that don't support sync).
    pub fn new(writer: &'a mut W, enabled: bool) -> io::Result<Self> {
        if enabled {
            begin_sync_update(writer)?;
        }
        Ok(Self { writer, enabled })
    }

    /// Check if synchronization is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl<W: Write> Drop for SyncGuard<'_, W> {
    fn drop(&mut self) {
        if self.enabled {
            // Best effort - ignore errors during drop
            let _ = end_sync_update(self.writer);
        }
    }
}

/// Wrapper that provides synchronized rendering around a closure.
///
/// # Example
///
/// ```ignore
/// with_sync(&mut stdout, supports_sync, || {
///     // Render frame here
/// })?;
/// ```
pub fn with_sync<W, F, R>(writer: &mut W, enabled: bool, f: F) -> io::Result<R>
where
    W: Write,
    F: FnOnce() -> R,
{
    let _guard = SyncGuard::new(writer, enabled)?;
    Ok(f())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_sequences() {
        let mut buf = Vec::new();

        begin_sync_update(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[?2026h");

        buf.clear();
        end_sync_update(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[?2026l");
    }

    #[test]
    fn test_sync_guard_enabled() {
        let mut buf = Vec::new();

        {
            let _guard = SyncGuard::new(&mut buf, true).unwrap();
            assert!(buf.starts_with(b"\x1b[?2026h"));
        }
        // Guard dropped, should have end sequence
        assert!(buf.ends_with(b"\x1b[?2026l"));
    }

    #[test]
    fn test_sync_guard_disabled() {
        let mut buf = Vec::new();

        {
            let _guard = SyncGuard::new(&mut buf, false).unwrap();
        }
        // Nothing should be written when disabled
        assert!(buf.is_empty());
    }
}
