//! Structural AST comparator. Walks two JS trees in lockstep and emits
//! `Finding`s for the first (or every) divergence.

mod align;
mod callable_equivalence;
mod class_equivalence;
mod findings;
mod node_utils;

pub mod report;
pub mod tokens;
pub mod walk;

pub use walk::{compare, CompareOptions};

#[cfg(test)]
mod walk_tests;
