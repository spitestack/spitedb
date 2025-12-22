//! Frame timing and animation utilities.
//!
//! SpiteStack - Code Angry.
//!
//! Provides animation timing configuration per capability tier
//! and smooth interpolation utilities.

use std::time::{Duration, Instant};

use crate::tui::capabilities::CapabilityTier;

/// Animation timing configuration.
#[derive(Debug, Clone)]
pub struct AnimationConfig {
    /// Duration between frames.
    pub frame_duration: Duration,
    /// Vinyl rotation speed (RPM).
    pub vinyl_rpm: f32,
    /// VU meter decay rate per frame.
    pub vu_decay_rate: f32,
    /// Peak hold duration in frames.
    pub peak_hold_frames: usize,
}

impl AnimationConfig {
    /// Get the animation configuration for a capability tier.
    pub fn for_tier(tier: CapabilityTier) -> Self {
        match tier {
            CapabilityTier::Premium => Self {
                frame_duration: Duration::from_millis(16), // ~60fps
                vinyl_rpm: 33.3,
                vu_decay_rate: 0.08,  // Slower decay for smoother animation
                peak_hold_frames: 30, // Hold peak for ~0.5s
            },
            CapabilityTier::Enhanced => Self {
                frame_duration: Duration::from_millis(33), // ~30fps
                vinyl_rpm: 33.3,
                vu_decay_rate: 0.12,
                peak_hold_frames: 15,
            },
            CapabilityTier::Fallback => Self {
                frame_duration: Duration::from_millis(50), // ~20fps
                vinyl_rpm: 33.3,
                vu_decay_rate: 0.20, // Faster decay for fewer frames
                peak_hold_frames: 10,
            },
        }
    }

    /// Get frames per second.
    pub fn fps(&self) -> f32 {
        1000.0 / self.frame_duration.as_millis() as f32
    }

    /// Calculate vinyl rotation per frame (in radians).
    pub fn vinyl_rotation_per_frame(&self) -> f32 {
        // RPM to radians per frame
        // RPM * (2π / 60) / FPS
        self.vinyl_rpm * std::f32::consts::TAU / 60.0 / self.fps()
    }
}

/// Frame timer for smooth animation.
///
/// Tracks elapsed time and returns the number of frames to advance.
pub struct FrameTimer {
    last_tick: Instant,
    accumulated: Duration,
    config: AnimationConfig,
    frame_count: u64,
}

impl FrameTimer {
    /// Create a new frame timer for the given capability tier.
    pub fn new(tier: CapabilityTier) -> Self {
        Self {
            last_tick: Instant::now(),
            accumulated: Duration::ZERO,
            config: AnimationConfig::for_tier(tier),
            frame_count: 0,
        }
    }

    /// Create a frame timer with custom configuration.
    pub fn with_config(config: AnimationConfig) -> Self {
        Self {
            last_tick: Instant::now(),
            accumulated: Duration::ZERO,
            config,
            frame_count: 0,
        }
    }

    /// Tick the timer and return the number of frames to advance.
    pub fn tick(&mut self) -> usize {
        let now = Instant::now();
        self.accumulated += now - self.last_tick;
        self.last_tick = now;

        let frame_nanos = self.config.frame_duration.as_nanos();
        let frames = (self.accumulated.as_nanos() / frame_nanos) as usize;

        if frames > 0 {
            self.accumulated -= self.config.frame_duration * frames as u32;
            self.frame_count += frames as u64;
        }

        frames
    }

    /// Get the total frame count.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get the current frame as a looping index.
    pub fn frame_index(&self, loop_length: u64) -> usize {
        (self.frame_count % loop_length) as usize
    }

    /// Get the configuration.
    pub fn config(&self) -> &AnimationConfig {
        &self.config
    }
}

// ============================================================================
// Easing Functions
// ============================================================================

/// Easing functions for smooth animations.
pub mod easing {
    /// Linear interpolation (no easing).
    pub fn linear(t: f32) -> f32 {
        t
    }

    /// Ease-in quadratic.
    pub fn ease_in_quad(t: f32) -> f32 {
        t * t
    }

    /// Ease-out quadratic.
    pub fn ease_out_quad(t: f32) -> f32 {
        1.0 - (1.0 - t) * (1.0 - t)
    }

    /// Ease-in-out quadratic.
    pub fn ease_in_out_quad(t: f32) -> f32 {
        if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    }

    /// Ease-in cubic.
    pub fn ease_in_cubic(t: f32) -> f32 {
        t * t * t
    }

    /// Ease-out cubic.
    pub fn ease_out_cubic(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(3)
    }

    /// Ease-in-out cubic.
    pub fn ease_in_out_cubic(t: f32) -> f32 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    /// Elastic ease-out (bouncy).
    pub fn ease_out_elastic(t: f32) -> f32 {
        if t == 0.0 || t == 1.0 {
            return t;
        }

        let c4 = std::f32::consts::TAU / 3.0;
        2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
    }

    /// Bounce ease-out.
    pub fn ease_out_bounce(t: f32) -> f32 {
        let n1 = 7.5625;
        let d1 = 2.75;

        if t < 1.0 / d1 {
            n1 * t * t
        } else if t < 2.0 / d1 {
            let t = t - 1.5 / d1;
            n1 * t * t + 0.75
        } else if t < 2.5 / d1 {
            let t = t - 2.25 / d1;
            n1 * t * t + 0.9375
        } else {
            let t = t - 2.625 / d1;
            n1 * t * t + 0.984375
        }
    }
}

// ============================================================================
// VU Meter Animation State
// ============================================================================

/// VU meter level with peak hold and decay.
#[derive(Debug, Clone)]
pub struct VuLevel {
    /// Current level (0.0 - 1.0).
    pub level: f32,
    /// Peak level (0.0 - 1.0).
    pub peak: f32,
    /// Frames since peak was set.
    peak_age: usize,
}

impl Default for VuLevel {
    fn default() -> Self {
        Self {
            level: 0.0,
            peak: 0.0,
            peak_age: 0,
        }
    }
}

impl VuLevel {
    /// Create a new VU level.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a new level and update peak.
    pub fn set_level(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);

        if self.level >= self.peak {
            self.peak = self.level;
            self.peak_age = 0;
        }
    }

    /// Update for a new frame (apply decay).
    pub fn tick(&mut self, config: &AnimationConfig) {
        // Decay the level
        self.level = (self.level - config.vu_decay_rate).max(0.0);

        // Age the peak
        self.peak_age += 1;

        // Decay peak after hold period
        if self.peak_age > config.peak_hold_frames {
            self.peak = (self.peak - config.vu_decay_rate * 0.5).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_config_for_tier() {
        let premium = AnimationConfig::for_tier(CapabilityTier::Premium);
        let fallback = AnimationConfig::for_tier(CapabilityTier::Fallback);

        // Premium should have faster frame rate
        assert!(premium.frame_duration < fallback.frame_duration);
        // Premium should have slower decay (smoother)
        assert!(premium.vu_decay_rate < fallback.vu_decay_rate);
    }

    #[test]
    fn test_frame_timer() {
        let mut timer = FrameTimer::new(CapabilityTier::Fallback);
        assert_eq!(timer.frame_count(), 0);

        // Simulate some time passing
        std::thread::sleep(Duration::from_millis(60));
        let frames = timer.tick();

        // Should have advanced at least 1 frame
        assert!(frames >= 1);
        assert!(timer.frame_count() >= 1);
    }

    #[test]
    fn test_easing_bounds() {
        use easing::*;

        for &f in &[
            linear as fn(f32) -> f32,
            ease_in_quad,
            ease_out_quad,
            ease_in_out_quad,
        ] {
            assert!((f(0.0) - 0.0).abs() < 0.01);
            assert!((f(1.0) - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_vu_level_decay() {
        let config = AnimationConfig::for_tier(CapabilityTier::Fallback);
        let mut vu = VuLevel::new();

        vu.set_level(1.0);
        assert_eq!(vu.level, 1.0);

        vu.tick(&config);
        assert!(vu.level < 1.0);
    }
}
