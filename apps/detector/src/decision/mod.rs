//! Decision engine module
//!
//! Implements sliding window voting and cooldown logic for fire/smoke detection.

mod engine;
mod cooldown;
mod window;

pub use engine::{DecisionEngine, DecisionEvent, DecisionEventType};
pub use cooldown::CooldownManager;
pub use window::SlidingWindow;
