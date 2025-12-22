//! Source location tracking.

use std::path::PathBuf;

/// A span in the source code.
#[derive(Debug, Clone)]
pub struct Span {
    pub file: PathBuf,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    pub fn new(file: PathBuf, start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            file,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}
