use std::collections::HashMap;
use std::path::PathBuf;

use crate::compare_options::CompareOptions;
use crate::test_helpers::{assert_equiv_raw, assert_flagged_raw, build_opts, OptsOverrides};

fn alias_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert(
        "readCdnConfig".to_string(),
        "config.cdn_config?".to_string(),
    );
    map
}

fn composition_opts() -> CompareOptions {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        narrowing_helpers_aliases: Some(alias_map()),
        alias_helper_optional_chain_composition: true,
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

#[test]
fn composes_when_default_is_empty_string() {
    let base = "function f(config, subdomain) {
            return `${config.cdn_config?.protocol}://${subdomain}.${config.cdn_config?.host}`;
        }";
    let head = "function f(config, subdomain) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.protocol ?? \"\"}://${subdomain}.${cdnConfig.host ?? \"\"}`;
        }";
    assert_equiv_raw(base, head, &composition_opts());
}

#[test]
fn composes_with_default_zero() {
    let base = "function f(config) {
            return config.cdn_config?.port;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return cdnConfig.port ?? 0;
        }";
    assert_equiv_raw(base, head, &composition_opts());
}

#[test]
fn composes_with_default_false() {
    let base = "function f(config) {
            return config.cdn_config?.secure;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return cdnConfig.secure ?? false;
        }";
    assert_equiv_raw(base, head, &composition_opts());
}

#[test]
fn composes_with_multiple_substitutions_in_one_template() {
    let base = "function f(config, userUuid) {
            return `${config.cdn_config?.protocol}://${config.cdn_config?.host}/u/${userUuid}`;
        }";
    let head = "function f(config, userUuid) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.protocol ?? \"\"}://${cdnConfig.host ?? \"\"}/u/${userUuid}`;
        }";
    assert_equiv_raw(base, head, &composition_opts());
}

#[test]
fn rejects_when_flag_off() {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        narrowing_helpers_aliases: Some(alias_map()),
        alias_helper_optional_chain_composition: false,
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "function f(config) {
            return `${config.cdn_config?.host}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.host ?? \"\"}`;
        }";
    assert_flagged_raw(base, head, &opts);
}

#[test]
fn rejects_when_alias_table_unset() {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        alias_helper_optional_chain_composition: true,
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "function f(config) {
            return `${config.cdn_config?.host}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.host ?? \"\"}`;
        }";
    assert_flagged_raw(base, head, &opts);
}

#[test]
fn rejects_when_default_is_unsafe_expression() {
    let base = "function f(config) {
            return `${config.cdn_config?.host}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.host ?? someFn()}`;
        }";
    assert_flagged_raw(base, head, &composition_opts());
}

#[test]
fn rejects_when_property_diverges() {
    let base = "function f(config) {
            return `${config.cdn_config?.protocol}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.host ?? \"\"}`;
        }";
    assert_flagged_raw(base, head, &composition_opts());
}

#[test]
fn rejects_when_lhs_is_unrelated_local() {
    let base = "function f(config) {
            return `${config.cdn_config?.host}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            const other = config.other;
            return `${other ?? \"\"}`;
        }";
    assert_flagged_raw(base, head, &composition_opts());
}
