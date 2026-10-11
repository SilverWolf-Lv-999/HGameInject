use std::f32::consts::{E, PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Constants {
    Pi,
    Tau,
    Euler,
}

impl Constants {
    pub const fn value(self) -> f32 {
        match self {
            Constants::Pi => PI,
            Constants::Tau => TAU,
            Constants::Euler => E,
        }
    }
}

pub struct MathUtility;

impl MathUtility {
    pub fn degrees_to_radians(degrees: f32) -> f32 {
        degrees * (Constants::Pi.value() / 180.0)
    }

    pub fn radians_to_degrees(radians: f32) -> f32 {
        radians * (180.0 / Constants::Pi.value())
    }

    pub fn clamp(value: f32, minimum: f32, maximum: f32) -> f32 {
        value.max(minimum).min(maximum)
    }

    pub fn lerp(start: f32, end: f32, delta: f32) -> f32 {
        start + (end - start) * delta
    }

    pub fn wrap_angle(degrees: f32) -> f32 {
        (degrees % 360.0 + 360.0) % 360.0
    }

    pub fn random_range(minimum: f32, maximum: f32) -> f32 {
        let span = maximum - minimum;
        minimum + span * MathUtility::next_unit()
    }

    pub fn next_unit() -> f32 {
        let state = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mixed = state.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (((mixed >> 33) as u32) as f32) / ((u32::MAX >> 1) as f32 + 1.0)
    }
}

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x2545_F491);
