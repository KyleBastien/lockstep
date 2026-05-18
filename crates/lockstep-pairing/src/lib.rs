//! File-pair discovery between HEAD and the configured default branch.

pub mod git;
pub mod pairing;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;

pub use git::{read_blob_at, repo_open, resolve_default_branch_tree, RepoError};
pub use pairing::{discover_pairs, FilePair, PairKind};
