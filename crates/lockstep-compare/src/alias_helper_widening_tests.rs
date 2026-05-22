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

fn alias_map_without_trailing_chain() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("readCdnConfig".to_string(), "config.cdn_config".to_string());
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

fn composition_opts_without_trailing_chain() -> CompareOptions {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string()]),
        narrowing_helpers_aliases: Some(alias_map_without_trailing_chain()),
        alias_helper_optional_chain_composition: true,
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

fn mixed_rule_opts_without_trailing_chain() -> CompareOptions {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readCdnConfig".to_string(), "asString".to_string()]),
        narrowing_helpers_aliases: Some(alias_map_without_trailing_chain()),
        alias_helper_optional_chain_composition: true,
        helper_call_site_substitution: true,
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

// --- v0.1.17 Gap 1 — alias path tolerates omitted trailing `?` ---

#[test]
fn composes_in_template_with_alias_path_lacking_trailing_optional_chain() {
    let base = "function f(config) {
            return `${config.cdn_config?.protocol}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.protocol ?? \"\"}`;
        }";
    assert_equiv_raw(base, head, &composition_opts_without_trailing_chain());
}

#[test]
fn composes_sibling_alias_helper_slots_without_trailing_chain_marker() {
    let base = "function f(config) {
            return `${config.cdn_config?.protocol}://${config.cdn_config?.host}`;
        }";
    let head = "function f(config) {
            const cdnConfig = readCdnConfig();
            return `${cdnConfig.protocol ?? \"\"}://${cdnConfig.host ?? \"\"}`;
        }";
    assert_equiv_raw(base, head, &composition_opts_without_trailing_chain());
}

#[test]
fn composes_with_multi_segment_alias_path_lacking_trailing_chain() {
    let mut map = HashMap::new();
    map.insert("readDeep".to_string(), "a.b.c".to_string());
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readDeep".to_string()]),
        narrowing_helpers_aliases: Some(map),
        alias_helper_optional_chain_composition: true,
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "function f(a) {
            return `${a.b.c?.x}`;
        }";
    let head = "function f(a) {
            const cAlias = readDeep();
            return `${cAlias.x ?? \"\"}`;
        }";
    assert_equiv_raw(base, head, &opts);
}

#[test]
fn composes_with_mixed_rule_siblings_in_one_template() {
    let base = "function f(config, clientData) {
            return `${config.cdn_config?.protocol}://${clientData.subdomain}`;
        }";
    let head = "function asString(v) { return typeof v === \"string\" ? v : undefined; }
        function f(config, clientData) {
            const cdnConfig = readCdnConfig();
            const subdomain = asString(clientData?.subdomain) ?? \"\";
            return `${cdnConfig.protocol ?? \"\"}://${subdomain}`;
        }";
    assert_equiv_raw(base, head, &mixed_rule_opts_without_trailing_chain());
}

#[test]
fn full_canonical_template_with_mixed_rules_and_no_chain_marker_in_alias() {
    let base = "function f(config, clientData, userUuid) {
            return `${config.cdn_config?.protocol}://${clientData.subdomain}.${config.cdn_config?.host}/u/${userUuid}`;
        }";
    let head = "function asString(v) { return typeof v === \"string\" ? v : undefined; }
        function f(config, clientData, userUuid) {
            const cdnConfig = readCdnConfig();
            const subdomain = asString(clientData?.subdomain) ?? \"\";
            return `${cdnConfig.protocol ?? \"\"}://${subdomain}.${cdnConfig.host ?? \"\"}/u/${userUuid}`;
        }";
    assert_equiv_raw(base, head, &mixed_rule_opts_without_trailing_chain());
}
