use std::collections::HashMap;
use std::path::PathBuf;

use crate::compare_options::CompareOptions;
use crate::test_helpers::{assert_equiv_raw, assert_flagged_raw, build_opts, OptsOverrides};

fn zero_arg_opts() -> CompareOptions {
    let mut map = HashMap::new();
    map.insert(
        "readCdnConfig".to_string(),
        "config.cdn_config?".to_string(),
    );
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
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
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

#[test]
fn gap2b_zero_arg_helper_alias_substitutes_member_access() {
    let base = "function f(config) {
            return config.cdn_config?.host;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return cdnConfig.host;
        }";
    assert_equiv_raw(base, head, &zero_arg_opts());
}

#[test]
fn gap2b_zero_arg_helper_alias_substitutes_multiple_uses() {
    let base = "function f(config, subdomain) {
            return `${config.cdn_config?.protocol}://${subdomain}.${config.cdn_config?.host}/`;
        }";
    let head = "function f(config, subdomain) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.protocol}://${subdomain}.${cdnConfig.host}/`;
        }";
    assert_equiv_raw(base, head, &zero_arg_opts());
}

#[test]
fn gap2b_rejects_when_aliases_table_unset() {
    let base = "function f(config) {
            return config.cdn_config?.host;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return cdnConfig.host;
        }";
    assert_flagged_raw(base, head, &opts_without_aliases_table());
}

#[test]
fn gap2b_rejects_when_helper_takes_arguments() {
    // Zero-arg rule must not fire for `HELPER(x)`.
    let mut map = HashMap::new();
    map.insert(
        "readCdnConfig".to_string(),
        "config.cdn_config?".to_string(),
    );
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        narrowing_helpers_aliases: Some(map),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "function f(config) {
            return config.cdn_config?.host;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig(config);
            return cdnConfig.host;
        }";
    assert_flagged_raw(base, head, &opts);
}

#[test]
fn gap2b_rejects_when_property_chain_diverges() {
    // Head reads .other; base reads .host. Substitution would not match.
    let base = "function f(config) {
            return config.cdn_config?.host;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return cdnConfig.other;
        }";
    assert_flagged_raw(base, head, &zero_arg_opts());
}

#[test]
fn alias_path_without_trailing_optional_chain_still_substitutes() {
    // v0.1.17: alias path lacks trailing `?` but matcher inserts one at the
    // alias/property-accessor boundary when needed.
    let mut map = HashMap::new();
    map.insert("readCdnConfig".to_string(), "config.cdn_config".to_string());
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        narrowing_helpers_aliases: Some(map),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "function f(config) {
            return config.cdn_config?.host;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return cdnConfig.host;
        }";
    assert_equiv_raw(base, head, &opts);
}
