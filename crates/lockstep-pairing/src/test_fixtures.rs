//! Shared test fixtures for git-backed integration tests.
//!
//! Gated behind `cfg(test)` for in-crate use and the `test-fixtures` feature
//! for sibling crates' `[dev-dependencies]`. Avoids duplicating ~30 lines of
//! `git init && add && commit` plumbing across every test that needs a real
//! repo on disk.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// Run `git <args>` inside `dir`, panicking on failure.
pub fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("git binary should be on PATH for tests");
    assert!(status.success(), "git {:?} failed", args);
}

/// Write `contents` to `dir/rel`, creating parent dirs as needed.
pub fn write(dir: &Path, rel: &str, contents: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, contents).unwrap();
}

/// Build a one-commit git repo on branch `main` with a single committed file.
pub fn make_committed_repo(tmp: &TempDir, path: &str, contents: &str) -> PathBuf {
    let root = tmp.path().to_path_buf();
    run_git(&root, &["init", "-q", "-b", "main"]);
    run_git(&root, &["config", "user.email", "t@t"]);
    run_git(&root, &["config", "user.name", "t"]);
    write(&root, path, contents);
    run_git(&root, &["add", path]);
    run_git(&root, &["commit", "-q", "-m", "init"]);
    root
}

/// Build a "migration in progress" repo: commits `base_path` with `base_src`
/// on main, then removes it and stages `head_path` with `head_src`. The repo
/// is left with an unmerged head TS file vs. the base JS file in HEAD~1.
pub fn setup_migration_repo(
    tmp: &TempDir,
    base_path: &str,
    base_src: &str,
    head_path: &str,
    head_src: &str,
) -> PathBuf {
    let root = make_committed_repo(tmp, base_path, base_src);
    run_git(&root, &["rm", "-q", base_path]);
    write(&root, head_path, head_src);
    run_git(&root, &["add", head_path]);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_git_succeeds_in_initialised_repo() {
        let tmp = TempDir::new().unwrap();
        run_git(tmp.path(), &["init", "-q", "-b", "main"]);
        assert!(tmp.path().join(".git").exists());
    }

    #[test]
    fn write_creates_parent_dirs_and_file() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "a/b/c.txt", "hi");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("a/b/c.txt")).unwrap(),
            "hi"
        );
    }

    #[test]
    fn make_committed_repo_produces_a_committed_blob() {
        let tmp = TempDir::new().unwrap();
        let root = make_committed_repo(&tmp, "x.txt", "hello");
        assert!(root.join(".git").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("x.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn setup_migration_repo_stages_head_and_removes_base() {
        let tmp = TempDir::new().unwrap();
        let root = setup_migration_repo(
            &tmp,
            "src/a.js",
            "let x = 1;",
            "src/a.ts",
            "let x: number = 1;",
        );
        assert!(root.join("src/a.ts").exists());
        assert!(!root.join("src/a.js").exists());
    }
}
