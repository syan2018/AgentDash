//! Shared executable behavior checks for runtime and driver implementations.

pub mod session_parity;
mod trace_validator;

pub use trace_validator::{ConformanceViolation, RuntimeTraceValidator};
