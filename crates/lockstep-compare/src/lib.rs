//! Structural AST comparator. Walks two JS trees in lockstep and emits
//! `Finding`s for the first (or every) divergence.

mod align;
mod array_first_equivalence;
mod callable_equivalence;
mod class_equivalence;
mod findings;
mod node_utils;
mod nullish_widening_equivalence;
mod optional_chain;

pub mod report;
pub mod tokens;
pub mod walk;

pub use walk::{compare, CompareOptions};

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod walk_tests;
#[cfg(test)]
mod widening_tests;
