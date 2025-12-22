//! TUI widgets.
//!
//! SpiteStack Records - "Code Angry"

mod dashboard;
mod errors;
mod input;
mod music;
mod output;
mod splash;
mod status;
mod vinyl;
mod vu_meter;

pub use dashboard::draw_dashboard;
pub use errors::draw_errors;
pub use input::draw_input;
pub use music::draw_music_mode;
pub use output::draw_output;
pub use splash::draw_splash;
pub use status::draw_status;
pub use vinyl::{
    draw_large_vinyl, draw_large_vinyl_tiered, draw_mini_vinyl, draw_mini_vinyl_tiered,
    draw_tone_arm, draw_vinyl_label,
};
pub use vu_meter::{draw_progress_bar, draw_progress_bar_tiered, draw_vu_meters, draw_vu_meters_tiered};
