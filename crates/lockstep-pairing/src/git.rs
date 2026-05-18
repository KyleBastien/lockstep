//! Thin wrappers around `git2` for reading files at refs and diffing trees.

use std::path::{Path, PathBuf};

use git2::{Oid, Repository, Tree};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("io: {0}")]
    Io(String),
    #[error("ref `{0}` did not resolve to a commit")]
    NotACommit(String),
    #[error("path `{0}` not found in tree")]
    PathNotInTree(PathBuf),
    #[error("blob at `{0}` is not valid UTF-8")]
    NotUtf8(PathBuf),
}

pub fn repo_open(root: &Path) -> Result<Repository, RepoError> {
    Repository::discover(root).map_err(RepoError::from)
}

/// Resolve `<ref>` to a Tree object. Accepts:
///   * a short branch name (`main`),
///   * a fully-qualified ref (`refs/heads/main`),
///   * a remote-tracking branch (`origin/main`),
///   * a commit SHA.
pub fn resolve_default_branch_tree<'r>(
    repo: &'r Repository,
    name: &str,
) -> Result<(Tree<'r>, Oid), RepoError> {
    let candidates = [
        name.to_string(),
        format!("refs/heads/{name}"),
        format!("refs/remotes/origin/{name}"),
        format!("origin/{name}"),
    ];
    for cand in &candidates {
        if let Ok(obj) = repo.revparse_single(cand) {
            if let Ok(commit) = obj.peel_to_commit() {
                let tree = commit.tree()?;
                return Ok((tree, commit.id()));
            }
        }
    }
    Err(RepoError::NotACommit(name.into()))
}

/// Read the blob at `path` from `tree` as UTF-8.
pub fn read_blob_at(repo: &Repository, tree: &Tree, path: &Path) -> Result<String, RepoError> {
    let entry = tree
        .get_path(path)
        .map_err(|_| RepoError::PathNotInTree(path.to_path_buf()))?;
    let obj = entry.to_object(repo)?;
    let blob = obj
        .as_blob()
        .ok_or_else(|| RepoError::PathNotInTree(path.to_path_buf()))?;
    let content =
        std::str::from_utf8(blob.content()).map_err(|_| RepoError::NotUtf8(path.to_path_buf()))?;
    Ok(content.to_string())
}

/// Probe whether `path` exists in `tree` without reading its full content.
pub fn path_exists_in_tree(tree: &Tree, path: &Path) -> bool {
    tree.get_path(path).is_ok()
}

/// True if the blob at `path` in `tree` contains the substring `needle`.
pub fn blob_contains(
    repo: &Repository,
    tree: &Tree,
    path: &Path,
    needle: &str,
) -> Result<bool, RepoError> {
    let entry = match tree.get_path(path) {
        Ok(e) => e,
        Err(_) => return Ok(false),
    };
    let obj = entry.to_object(repo)?;
    let blob = match obj.as_blob() {
        Some(b) => b,
        None => return Ok(false),
    };
    let bytes = blob.content();
    Ok(bytes.windows(needle.len()).any(|w| w == needle.as_bytes()))
}

/// List paths that differ between `base_tree` and the working tree (with index
/// applied). Returns paths relative to the repo workdir.
pub fn paths_changed_vs_tree(
    repo: &Repository,
    base_tree: &Tree,
) -> Result<Vec<PathBuf>, RepoError> {
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let diff = repo.diff_tree_to_workdir_with_index(Some(base_tree), Some(&mut opts))?;
    let mut out = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(p) = delta.new_file().path() {
                out.push(p.to_path_buf());
            } else if let Some(p) = delta.old_file().path() {
                out.push(p.to_path_buf());
            }
            true
        },
        None,
        None,
        None,
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{make_committed_repo, write};
    use tempfile::TempDir;

    #[test]
    fn repo_open_finds_initialised_repo() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "a.txt", "hi");
        let repo = repo_open(&root).unwrap();
        assert!(repo.workdir().is_some());
    }

    #[test]
    fn resolve_default_branch_tree_finds_main() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "a.txt", "hi");
        let repo = repo_open(&root).unwrap();
        let (_tree, oid) = resolve_default_branch_tree(&repo, "main").unwrap();
        assert!(!oid.is_zero());
    }

    #[test]
    fn read_blob_at_returns_committed_contents() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "a.txt", "hello\n");
        let repo = repo_open(&root).unwrap();
        let (tree, _) = resolve_default_branch_tree(&repo, "main").unwrap();
        let text = read_blob_at(&repo, &tree, Path::new("a.txt")).unwrap();
        assert_eq!(text, "hello\n");
    }

    #[test]
    fn path_exists_in_tree_distinguishes_present_and_missing() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "a.txt", "hi");
        let repo = repo_open(&root).unwrap();
        let (tree, _) = resolve_default_branch_tree(&repo, "main").unwrap();
        assert!(path_exists_in_tree(&tree, Path::new("a.txt")));
        assert!(!path_exists_in_tree(&tree, Path::new("missing.txt")));
    }

    #[test]
    fn blob_contains_finds_substring() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "a.ts", "// @ts-ignore\nlet x = 1;\n");
        let repo = repo_open(&root).unwrap();
        let (tree, _) = resolve_default_branch_tree(&repo, "main").unwrap();
        assert!(blob_contains(&repo, &tree, Path::new("a.ts"), "@ts-ignore").unwrap());
        assert!(!blob_contains(&repo, &tree, Path::new("a.ts"), "nope").unwrap());
        assert!(!blob_contains(&repo, &tree, Path::new("missing"), "x").unwrap());
    }

    #[test]
    fn paths_changed_vs_tree_lists_modified_paths() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "a.ts", "let x = 1;\n");
        write(&root, "a.ts", "let x = 2;\n");
        let repo = repo_open(&root).unwrap();
        let (tree, _) = resolve_default_branch_tree(&repo, "main").unwrap();
        let changed = paths_changed_vs_tree(&repo, &tree).unwrap();
        assert!(changed.iter().any(|p| p == Path::new("a.ts")));
    }
}
