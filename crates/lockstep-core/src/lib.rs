//! Shared types: `Finding`, `Severity`, `Category`, `Verdict`, `Report`.
//!
//! No dependencies on parsing/git/config — pure data carrier so every other
//! crate can speak the same vocabulary.

pub mod finding;
pub mod report;
pub mod verdict;

pub use finding::{Category, Finding, Severity};
pub use report::{Report, SeverityCounts};
pub use verdict::{Verdict, VerdictKind};
