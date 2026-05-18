//! TS → JS-equivalent source rewrite.
//!
//! Strategy: walk the TS tree, collect byte ranges of TS-only constructs,
//! splice them out, then re-parse the result with `tree-sitter-javascript`.

pub mod elide;
pub mod fields;
pub mod imports;
pub mod ts_nodes;

#[cfg(test)]
mod elide_tests;

pub use elide::{strip, Rejection, StripError, StripOutput, TsFlavor};
