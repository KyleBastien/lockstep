//! TOML config loader for lockstep.
//!
//! Default location is `.lockstep/config.toml` relative to the repo root.
//! CLI flags override loaded values via the [`Config::override_with`] mutators.

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid glob `{pattern}`: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: globset::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_branch: String,
    pub allow_var_to_const_let: bool,
    pub allow_formatting_diff: bool,
    pub allow_enum_to_iife: bool,
    pub allow_constructor_assigned_method_equivalence: bool,
    pub allow_closure_cache_field_alias: bool,
    pub allow_array_first_element_or_null: bool,
    pub allow_array_first_element_or_null_loose: bool,
    pub allow_nullish_widening: bool,
    pub allow_null_undefined_swap: bool,
    pub allow_iife_async_wrapper: bool,
    pub allow_transient_cache_wrap: bool,
    pub allow_request_field_narrowing: bool,
    pub allow_async_propagation: bool,
    pub allow_defensive_null_guard: bool,
    pub allow_non_null_alias_local: bool,
    pub allow_defensive_log_guard: bool,
    pub defensive_log_guard_methods: Vec<String>,
    pub allow_dead_defensive_optional_chain_removal: bool,
    pub report_all_findings: bool,
    pub ignore: Vec<String>,
}

fn default_log_guard_methods() -> Vec<String> {
    ["debug", "info", "warn", "error", "trace", "log"]
        .into_iter()
        .map(String::from)
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_branch: "main".into(),
            allow_var_to_const_let: true,
            allow_formatting_diff: true,
            allow_enum_to_iife: false,
            allow_constructor_assigned_method_equivalence: true,
            allow_closure_cache_field_alias: false,
            allow_array_first_element_or_null: false,
            allow_array_first_element_or_null_loose: false,
            allow_nullish_widening: false,
            allow_null_undefined_swap: false,
            allow_iife_async_wrapper: false,
            allow_transient_cache_wrap: false,
            allow_request_field_narrowing: false,
            allow_async_propagation: false,
            allow_defensive_null_guard: false,
            allow_non_null_alias_local: false,
            allow_defensive_log_guard: false,
            defensive_log_guard_methods: default_log_guard_methods(),
            allow_dead_defensive_optional_chain_removal: false,
            report_all_findings: true,
            ignore: vec![
                "**/*.test.ts".into(),
                "**/*.test.tsx".into(),
                "**/*.spec.ts".into(),
                "**/*.spec.tsx".into(),
                "**/__snapshots__/**".into(),
                "**/node_modules/**".into(),
                "**/dist/**".into(),
                "**/build/**".into(),
            ],
        }
    }
}

impl Config {
    /// Load from a TOML file. If the file does not exist, returns defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let parsed: Config = toml::from_str(&raw)?;
        Ok(parsed)
    }

    /// Compile [`Config::ignore`] into a globset for fast path matching.
    pub fn ignore_set(&self) -> Result<GlobSet, ConfigError> {
        let mut b = GlobSetBuilder::new();
        for pat in &self.ignore {
            let glob = Glob::new(pat).map_err(|source| ConfigError::Glob {
                pattern: pat.clone(),
                source,
            })?;
            b.add(glob);
        }
        b.build().map_err(|source| ConfigError::Glob {
            pattern: "<built>".into(),
            source,
        })
    }

    pub fn override_default_branch(mut self, branch: Option<String>) -> Self {
        if let Some(b) = branch {
            self.default_branch = b;
        }
        self
    }

    pub fn override_report_all(mut self, on: bool) -> Self {
        if on {
            self.report_all_findings = true;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn defaults_are_sensible() {
        let c = Config::default();
        assert_eq!(c.default_branch, "main");
        assert!(c.allow_var_to_const_let);
        assert!(!c.allow_enum_to_iife);
        assert!(c.allow_constructor_assigned_method_equivalence);
        assert!(!c.allow_closure_cache_field_alias);
        assert!(!c.allow_array_first_element_or_null);
        assert!(!c.allow_array_first_element_or_null_loose);
        assert!(!c.allow_nullish_widening);
        assert!(!c.allow_null_undefined_swap);
        assert!(!c.allow_iife_async_wrapper);
        assert!(!c.allow_transient_cache_wrap);
        assert!(!c.allow_request_field_narrowing);
        assert!(!c.allow_async_propagation);
        assert!(!c.allow_defensive_null_guard);
        assert!(!c.allow_non_null_alias_local);
        assert!(!c.allow_defensive_log_guard);
        assert!(!c.allow_dead_defensive_optional_chain_removal);
        assert_eq!(
            c.defensive_log_guard_methods,
            vec!["debug", "info", "warn", "error", "trace", "log"]
        );
        assert!(c.report_all_findings);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let c = Config::load(&PathBuf::from("/tmp/__definitely_not_there.toml")).unwrap();
        assert_eq!(c.default_branch, "main");
    }

    #[test]
    fn override_default_branch_replaces_value() {
        let c = Config::default().override_default_branch(Some("master".into()));
        assert_eq!(c.default_branch, "master");
    }

    #[test]
    fn ignore_set_matches_patterns() {
        let c = Config::default();
        let set = c.ignore_set().unwrap();
        assert!(set.is_match("src/foo.test.ts"));
        assert!(set.is_match("packages/x/__snapshots__/y.snap"));
        assert!(!set.is_match("src/foo.ts"));
    }

    #[test]
    fn override_report_all_preserves_granular_default() {
        let c = Config::default().override_report_all(false);
        assert!(c.report_all_findings);
        let c2 = Config::default().override_report_all(true);
        assert!(c2.report_all_findings);
    }
}
