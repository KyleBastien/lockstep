//! Source-level rewrites applied to BOTH base and head before comparison:
//! `var` → `let` and trailing-comma elision.

pub mod trailing_comma;
pub mod var_keyword;

pub use trailing_comma::strip_trailing_commas;
pub use var_keyword::var_to_let;
