//! Match HEAD-side touched `.ts`/`.tsx` files to a default-branch baseline.
//!
//! Pairing predicate (per user-confirmed plan decision):
//!   (a) HEAD path `src/foo.ts` ↔ default-branch path `src/foo.js`/.jsx/.mjs/.cjs.
//!   (b) Same HEAD path on default branch when its blob contains a TS
//!       suppression marker (`@ts-ignore` or `@ts-nocheck`).
//!
//! Files not matching either are skipped — they were authored fresh as TS and
//! have no JS counterpart to compare against.

use std::path::{Path, PathBuf};

use git2::{Oid, Repository, Tree};
use globset::GlobSet;

use crate::git::{
    blob_contains, path_exists_in_tree, paths_changed_vs_tree, resolve_default_branch_tree,
    RepoError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    /// Base file is a `.js`/`.jsx`/`.mjs`/`.cjs` at the matching stem.
    JsCounterpart,
    /// Base file is the same `.ts`/`.tsx` path containing a TS suppression
    /// marker (`@ts-ignore` or `@ts-nocheck`).
    TsWithSuppression,
}

const SUPPRESSION_MARKERS: &[&str] = &["@ts-ignore", "@ts-nocheck"];

#[derive(Debug, Clone)]
pub struct FilePair {
    pub head_path: PathBuf,
    pub head_abs_path: PathBuf,
    pub base_path: PathBuf,
    pub base_ref_tree: Oid,
    pub kind: PairKind,
}

pub fn discover_pairs(
    repo: &Repository,
    base_ref: &str,
    explicit_paths: &[PathBuf],
    ignore: &GlobSet,
    workdir: &Path,
) -> Result<Vec<FilePair>, RepoError> {
    let (base_tree, _base_oid) = resolve_default_branch_tree(repo, base_ref)?;

    let candidates: Vec<PathBuf> = if explicit_paths.is_empty() {
        paths_changed_vs_tree(repo, &base_tree)?
    } else {
        explicit_paths.to_vec()
    };

    let mut pairs = Vec::new();
    for path in candidates {
        let rel = make_relative(&path, workdir);
        if !is_ts_extension(&rel) {
            continue;
        }
        if ignore.is_match(&rel) {
            continue;
        }
        if let Some(p) = try_pair(repo, &base_tree, &rel, workdir)? {
            pairs.push(p);
        }
    }
    Ok(pairs)
}

fn try_pair(
    repo: &Repository,
    base_tree: &Tree,
    rel: &Path,
    workdir: &Path,
) -> Result<Option<FilePair>, RepoError> {
    let head_abs = workdir.join(rel);
    let tree_oid = base_tree.id();

    // (a) Try JS counterparts at the same stem.
    for ext in JS_EXTENSIONS {
        let cand = swap_extension(rel, ext);
        if path_exists_in_tree(base_tree, &cand) {
            return Ok(Some(FilePair {
                head_path: rel.to_path_buf(),
                head_abs_path: head_abs,
                base_path: cand,
                base_ref_tree: tree_oid,
                kind: PairKind::JsCounterpart,
            }));
        }
    }

    // (b) Same TS path on the base branch with a TS suppression marker
    //     (@ts-ignore or @ts-nocheck).
    if path_exists_in_tree(base_tree, rel) && blob_has_suppression(repo, base_tree, rel)? {
        return Ok(Some(FilePair {
            head_path: rel.to_path_buf(),
            head_abs_path: head_abs,
            base_path: rel.to_path_buf(),
            base_ref_tree: tree_oid,
            kind: PairKind::TsWithSuppression,
        }));
    }

    Ok(None)
}

const JS_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs"];

fn blob_has_suppression(
    repo: &Repository,
    base_tree: &Tree,
    rel: &Path,
) -> Result<bool, RepoError> {
    for marker in SUPPRESSION_MARKERS {
        if blob_contains(repo, base_tree, rel, marker)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_ts_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx")
    )
}

fn swap_extension(path: &Path, new_ext: &str) -> PathBuf {
    let mut p = path.to_path_buf();
    p.set_extension(new_ext);
    p
}

fn make_relative(path: &Path, workdir: &Path) -> PathBuf {
    path.strip_prefix(workdir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_extension_basic() {
        assert_eq!(
            swap_extension(Path::new("src/foo.ts"), "js"),
            PathBuf::from("src/foo.js")
        );
        assert_eq!(
            swap_extension(Path::new("a/b/c.tsx"), "jsx"),
            PathBuf::from("a/b/c.jsx")
        );
    }

    #[test]
    fn ts_extension_detect() {
        assert!(is_ts_extension(Path::new("a.ts")));
        assert!(is_ts_extension(Path::new("a.tsx")));
        assert!(!is_ts_extension(Path::new("a.js")));
        assert!(!is_ts_extension(Path::new("a")));
    }

    mod integration {
        use super::super::*;
        use crate::git::repo_open;
        use crate::test_fixtures::setup_migration_repo;
        use globset::GlobSetBuilder;
        use tempfile::TempDir;

        #[test]
        fn discover_pairs_finds_js_counterpart_migration() {
            let tmp = TempDir::new().unwrap();
            let root = setup_migration_repo(
                &tmp,
                "src/x.js",
                "let x = 1;\n",
                "src/x.ts",
                "let x: number = 1;\n",
            );
            let repo = repo_open(&root).unwrap();
            let pairs = discover_pairs(
                &repo,
                "main",
                &[],
                &GlobSetBuilder::new().build().unwrap(),
                &root,
            )
            .unwrap();
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].kind, PairKind::JsCounterpart);
            assert_eq!(pairs[0].head_path, PathBuf::from("src/x.ts"));
        }

        fn discover_pairs_with_base_marker(base_src: &str) -> Vec<FilePair> {
            use crate::test_fixtures::{make_committed_repo, write};
            let tmp = TempDir::new().unwrap();
            let root = make_committed_repo(&tmp, "src/x.ts", base_src);
            // Modify the same TS file on HEAD so it shows up as changed.
            write(&root, "src/x.ts", "let x: number = 2;\n");
            let repo = repo_open(&root).unwrap();
            discover_pairs(
                &repo,
                "main",
                &[],
                &GlobSetBuilder::new().build().unwrap(),
                &root,
            )
            .unwrap()
        }

        #[test]
        fn discover_pairs_matches_ts_ignore_on_base() {
            let pairs = discover_pairs_with_base_marker("// @ts-ignore\nlet x: number = 1;\n");
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].kind, PairKind::TsWithSuppression);
            assert_eq!(pairs[0].head_path, PathBuf::from("src/x.ts"));
        }

        #[test]
        fn discover_pairs_matches_ts_nocheck_on_base() {
            let pairs = discover_pairs_with_base_marker("// @ts-nocheck\nlet x: number = 1;\n");
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].kind, PairKind::TsWithSuppression);
            assert_eq!(pairs[0].head_path, PathBuf::from("src/x.ts"));
        }
    }
}
