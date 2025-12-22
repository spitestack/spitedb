//! Rendering primitives for tiered terminal graphics.
//!
//! SpiteStack - Code Angry.
//!
//! This module provides:
//! - Braille graphics for high-resolution rendering (braille.rs)
//! - TrueColor gradient generation (gradients.rs)
//! - Animation timing and easing (animation.rs)

pub mod animation;
pub mod braille;
pub mod gradients;

pub use animation::{AnimationConfig, FrameTimer, VuLevel};
pub use braille::{generate_vinyl, generate_vinyl_static, BrailleCanvas};
pub use gradients::{ash_gradient, blood_gradient, ember_pulse, lerp_color, vinyl_gradient, vu_gradient};
