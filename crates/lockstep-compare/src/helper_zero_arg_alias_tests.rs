use std::collections::HashMap;
use std::path::PathBuf;

use crate::compare_options::CompareOptions;
use crate::test_helpers::{assert_equiv_raw, assert_flagged_raw, build_opts, OptsOverrides};

fn zero_arg_opts() -> CompareOptions {
    let mut map = HashMap::new();
    map.insert("readPpConfig".to_string(), "config.pp_config?".to_string());
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readPpConfig".to_string()]),
        narrowing_helpers_aliases: Some(map),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

fn opts_without_aliases_table() -> CompareOptions {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readPpConfig".to_string()]),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

#[test]
fn gap2b_zero_arg_helper_alias_substitutes_member_access() {
    let base = "function f(config) {
            return config.pp_config?.host;
        }";
    let head = "function f(config) {
            const ppConfig = readPpConfig();
            return ppConfig.host;
        }";
    assert_equiv_raw(base, head, &zero_arg_opts());
}

#[test]
fn gap2b_zero_arg_helper_alias_substitutes_multiple_uses() {
    let base = "function f(config, subdomain) {
            return `${config.pp_config?.protocol}://${subdomain}.${config.pp_config?.host}/`;
        }";
    let head = "function f(config, subdomain) {
            const ppConfig = readPpConfig();
            return `${ppConfig.protocol}://${subdomain}.${ppConfig.host}/`;
        }";
    assert_equiv_raw(base, head, &zero_arg_opts());
}

#[test]
fn gap2b_rejects_when_aliases_table_unset() {
    let base = "function f(config) {
            return config.pp_config?.host;
        }";
    let head = "function f(config) {
            const ppConfig = readPpConfig();
            return ppConfig.host;
        }";
    assert_flagged_raw(base, head, &opts_without_aliases_table());
}

#[test]
fn gap2b_rejects_when_helper_takes_arguments() {
    // Zero-arg rule must not fire for `HELPER(x)`.
    let mut map = HashMap::new();
    map.insert("readPpConfig".to_string(), "config.pp_config?".to_string());
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readPpConfig".to_string()]),
        narrowing_helpers_aliases: Some(map),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "function f(config) {
            return config.pp_config?.host;
        }";
    let head = "function f(config) {
            const ppConfig = readPpConfig(config);
            return ppConfig.host;
        }";
    assert_flagged_raw(base, head, &opts);
}

#[test]
fn gap2b_rejects_when_property_chain_diverges() {
    // Head reads .other; base reads .host. Substitution would not match.
    let base = "function f(config) {
            return config.pp_config?.host;
        }";
    let head = "function f(config) {
            const ppConfig = readPpConfig();
            return ppConfig.other;
        }";
    assert_flagged_raw(base, head, &zero_arg_opts());
}
