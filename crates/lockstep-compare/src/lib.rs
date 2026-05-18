//! Structural AST comparator. Walks two JS trees in lockstep and emits
//! `Finding`s for the first (or every) divergence.

pub mod report;
pub mod tokens;
pub mod walk;

pub use walk::{compare, CompareOptions};
