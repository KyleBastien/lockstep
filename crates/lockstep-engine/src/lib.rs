//! Orchestrates: pair discovery → strip → normalize → compare → collect.

use std::path::{Path, PathBuf};

use git2::Tree;
use lockstep_compare::{compare, CompareOptions};
use lockstep_config::Config;
use lockstep_core::{Category, Finding, Report, Severity};
use lockstep_normalize::{strip_trailing_commas, var_to_let};
use lockstep_pairing::{discover_pairs, FilePair, PairKind};
use lockstep_strip::{strip, Rejection, StripOutput, TsFlavor};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("config: {0}")]
    Config(#[from] lockstep_config::ConfigError),
    #[error("pairing: {0}")]
    Pairing(#[from] lockstep_pairing::RepoError),
    #[error("strip: {0}")]
    Strip(#[from] lockstep_strip::StripError),
}

pub struct EngineOptions {
    pub repo_root: PathBuf,
    pub base_ref_override: Option<String>,
    pub explicit_paths: Vec<PathBuf>,
    pub dump_normalized_to: Option<PathBuf>,
}

pub fn run(config: &Config, opts: &EngineOptions) -> Result<Report, EngineError> {
    let repo = lockstep_pairing::repo_open(&opts.repo_root)?;
    let base_ref = opts
        .base_ref_override
        .clone()
        .unwrap_or_else(|| config.default_branch.clone());
    let ignore_set = config.ignore_set().map_err(EngineError::Config)?;
    let pairs = discover_pairs(
        &repo,
        &base_ref,
        &opts.explicit_paths,
        &ignore_set,
        opts.repo_root.as_path(),
    )?;
    let (base_tree, _base_oid) = lockstep_pairing::resolve_default_branch_tree(&repo, &base_ref)?;
    let mut findings: Vec<Finding> = Vec::new();
    let pairs_count = pairs.len() as u32;
    for pair in pairs {
        let pair_findings = check_pair(
            &repo,
            &base_tree,
            &pair,
            config,
            opts.dump_normalized_to.as_deref(),
        )?;
        findings.extend(pair_findings);
    }
    Ok(Report::from_findings(findings, pairs_count))
}

fn check_pair(
    repo: &git2::Repository,
    base_tree: &Tree<'_>,
    pair: &FilePair,
    config: &Config,
    dump_dir: Option<&Path>,
) -> Result<Vec<Finding>, EngineError> {
    let prepared = prepare_pair(repo, base_tree, pair, config)?;
    let PreparedPair {
        base,
        head,
        rejections,
    } = prepared;

    let (base_norm, head_norm) = normalize_pair(base, head, config);
    if let Some(dir) = dump_dir {
        dump_normalized(dir, &pair.head_path, &base_norm, &head_norm);
    }

    let mut out = collect_rejection_findings(&pair.head_path, &rejections, config);
    let opts = CompareOptions {
        path: pair.head_path.clone(),
        report_all: config.report_all_findings,
        allow_constructor_assigned_method_equivalence: config
            .allow_constructor_assigned_method_equivalence,
        allow_closure_cache_field_alias: config.allow_closure_cache_field_alias,
        allow_array_first_element_or_null: config.allow_array_first_element_or_null,
        allow_array_first_element_or_null_loose: config.allow_array_first_element_or_null_loose,
        allow_nullish_widening: config.allow_nullish_widening,
        allow_null_undefined_swap: config.allow_null_undefined_swap,
        allow_iife_async_wrapper: config.allow_iife_async_wrapper,
        allow_transient_cache_wrap: config.allow_transient_cache_wrap,
        allow_request_field_narrowing: config.allow_request_field_narrowing,
        allow_async_propagation: config.allow_async_propagation,
        allow_defensive_null_guard: config.allow_defensive_null_guard,
        allow_non_null_alias_local: config.allow_non_null_alias_local,
        allow_defensive_log_guard: config.allow_defensive_log_guard,
        defensive_log_guard_methods: config.defensive_log_guard_methods.clone(),
        allow_dead_defensive_optional_chain_removal: config
            .allow_dead_defensive_optional_chain_removal,
        allow_unknown_catch_narrowing: config.allow_unknown_catch_narrowing,
        allow_promise_settled_discrimination: config.allow_promise_settled_discrimination,
        allow_pure_narrowing_helper: config.allow_pure_narrowing_helper,
        narrowing_helpers: config.narrowing_helpers.clone(),
        allow_helper_call_site_substitution: config.allow_helper_call_site_substitution
            || config.allow_pure_narrowing_helper,
        allow_destructure_then_narrow: config.allow_destructure_then_narrow
            || config.allow_pure_narrowing_helper,
    };
    out.extend(compare(&base_norm, &head_norm, &opts));
    Ok(out)
}

struct PreparedPair {
    base: String,
    head: String,
    rejections: Vec<Rejection>,
}

fn prepare_pair(
    repo: &git2::Repository,
    base_tree: &Tree<'_>,
    pair: &FilePair,
    _config: &Config,
) -> Result<PreparedPair, EngineError> {
    let base_blob = lockstep_pairing::read_blob_at(repo, base_tree, &pair.base_path)?;
    let head_text = std::fs::read_to_string(pair.head_abs_path.as_path())
        .map_err(|e| EngineError::Pairing(lockstep_pairing::RepoError::Io(e.to_string())))?;

    let head_flavor = TsFlavor::from_extension(&pair.head_path);
    let StripOutput {
        output: head_stripped,
        rejections: head_rejections,
    } = strip(&head_text, head_flavor)?;

    let (base_stripped, base_rejections) = strip_base(&base_blob, pair)?;
    let mut rejections = head_rejections;
    rejections.extend(base_rejections);
    Ok(PreparedPair {
        base: base_stripped,
        head: head_stripped,
        rejections,
    })
}

fn strip_base(blob: &str, pair: &FilePair) -> Result<(String, Vec<Rejection>), EngineError> {
    match pair.kind {
        PairKind::JsCounterpart => Ok((blob.to_string(), Vec::new())),
        PairKind::TsWithSuppression => {
            let flavor = TsFlavor::from_extension(&pair.base_path);
            let out = strip(blob, flavor)?;
            Ok((out.output, out.rejections))
        }
    }
}

fn normalize_pair(mut base: String, mut head: String, config: &Config) -> (String, String) {
    if config.allow_var_to_const_let {
        base = var_to_let(&base);
        head = var_to_let(&head);
    }
    if config.allow_formatting_diff {
        base = strip_trailing_commas(&base);
        head = strip_trailing_commas(&head);
    }
    (base, head)
}

fn dump_normalized(dir: &Path, head_path: &Path, base: &str, head: &str) {
    let _ = std::fs::create_dir_all(dir);
    let stem = head_path.to_string_lossy().replace(['/', '\\'], "__");
    let _ = std::fs::write(dir.join(format!("{stem}.base.js")), base);
    let _ = std::fs::write(dir.join(format!("{stem}.head.js")), head);
}

fn collect_rejection_findings(
    head_path: &Path,
    rejections: &[Rejection],
    config: &Config,
) -> Vec<Finding> {
    rejections
        .iter()
        .filter(|r| !(r.kind == "enum_declaration" && config.allow_enum_to_iife))
        .map(|r| {
            Finding::new(
                head_path,
                Category::StrippedTsConstruct,
                format!(
                    "head uses TS construct `{}` that lockstep cannot mechanically equate to JS",
                    r.kind
                ),
            )
            .with_severity(Severity::Error)
            .with_lines(r.line, r.line)
            .with_snippets(r.snippet.clone(), r.snippet.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockstep_pairing::test_fixtures::setup_migration_repo;
    use tempfile::TempDir;

    fn default_opts(root: &Path) -> EngineOptions {
        EngineOptions {
            repo_root: root.to_path_buf(),
            base_ref_override: None,
            explicit_paths: Vec::new(),
            dump_normalized_to: None,
        }
    }

    #[test]
    fn run_approves_clean_annotation_only_migration() {
        let tmp = TempDir::new().unwrap();
        let root = setup_migration_repo(
            &tmp,
            "src/calc.js",
            "function add(a, b) { return a + b; }\n",
            "src/calc.ts",
            "function add(a: number, b: number): number { return a + b; }\n",
        );
        let report = run(&Config::default(), &default_opts(&root)).unwrap();
        assert_eq!(report.findings.len(), 0, "got: {:?}", report.findings);
        assert_eq!(report.pairs_examined, 1);
    }

    #[test]
    fn run_flags_divergent_migration() {
        let tmp = TempDir::new().unwrap();
        let root = setup_migration_repo(
            &tmp,
            "src/calc.js",
            "function add(a, b) { return a + b; }\n",
            "src/calc.ts",
            "function add(a: number, b: number): number { return a - b; }\n",
        );
        let report = run(&Config::default(), &default_opts(&root)).unwrap();
        assert!(!report.findings.is_empty());
    }

    #[test]
    fn pure_narrowing_helper_cascades_to_v0_1_14_subflags() {
        let tmp = TempDir::new().unwrap();
        let head = "function asString(value) { \
                return typeof value === \"string\" ? value : undefined; \
            }\n\
            function f(obj) {\n\
                const name = asString(obj.foo) ?? \"\";\n\
                return `name: ${name}`;\n\
            }\n";
        let root = setup_migration_repo(
            &tmp,
            "src/h.js",
            "function f(obj) { return `name: ${obj.foo}`; }\n",
            "src/h.ts",
            head,
        );
        let config = Config {
            allow_pure_narrowing_helper: true,
            narrowing_helpers: vec!["asString".to_string()],
            ..Config::default()
        };
        let report = run(&config, &default_opts(&root)).unwrap();
        assert_eq!(
            report.findings.len(),
            0,
            "v0.1.14 sub-flags should cascade on; got: {:?}",
            report.findings
        );
    }
}
