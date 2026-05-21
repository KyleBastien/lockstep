use std::collections::HashMap;
use std::path::PathBuf;

use crate::compare_options::CompareOptions;
use crate::test_helpers::{assert_equiv_raw, assert_flagged_raw, build_opts, OptsOverrides};

fn unwrap_opts() -> CompareOptions {
    let mut map = HashMap::new();
    map.insert("readAdminRows".to_string(), "data".to_string());
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readAdminRows".to_string()]),
        helper_call_site_substitution: true,
        narrowing_helpers_unwrap: Some(map),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

fn opts_without_unwrap_table() -> CompareOptions {
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readAdminRows".to_string()]),
        helper_call_site_substitution: true,
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    opts
}

#[test]
fn gap2a_two_stmt_pattern_with_await_source_absorbs() {
    let base = "async function f(client) {
            const clientAdmins = await this.clientRepository.getAdmins(client);
            return clientAdmins.data;
        }";
    let head = "async function f(client) {
            const clientAdminsRaw = await this.clientRepository.getAdmins(client);
            const adminRows = readAdminRows(clientAdminsRaw);
            return adminRows;
        }";
    assert_equiv_raw(base, head, &unwrap_opts());
}

#[test]
fn gap2a_two_stmt_pattern_with_iter_var_rename_in_map_absorbs() {
    let base = "async function f(client) {
            const clientAdmins = await this.clientRepository.getAdmins(client);
            const emailsTo = clientAdmins.data.map((client) => ({ email: client.email }));
            return emailsTo;
        }";
    let head = "async function f(client) {
            const clientAdminsRaw = await this.clientRepository.getAdmins(client);
            const adminRows = readAdminRows(clientAdminsRaw);
            const emailsTo = adminRows.map((admin) => ({ email: admin.email }));
            return emailsTo;
        }";
    assert_equiv_raw(base, head, &unwrap_opts());
}

#[test]
fn gap2a_one_stmt_inline_source_absorbs() {
    let base = "async function f(client) {
            const clientAdmins = await this.clientRepository.getAdmins(client);
            return clientAdmins.data;
        }";
    let head = "async function f(client) {
            const adminRows = readAdminRows(await this.clientRepository.getAdmins(client));
            return adminRows;
        }";
    assert_equiv_raw(base, head, &unwrap_opts());
}

#[test]
fn gap2a_rejects_when_unwrap_table_unset() {
    let base = "async function f(client) {
            const clientAdmins = await this.clientRepository.getAdmins(client);
            return clientAdmins.data;
        }";
    let head = "async function f(client) {
            const clientAdminsRaw = await this.clientRepository.getAdmins(client);
            const adminRows = readAdminRows(clientAdminsRaw);
            return adminRows;
        }";
    assert_flagged_raw(base, head, &opts_without_unwrap_table());
}

#[test]
fn gap2a_rejects_when_base_source_differs() {
    let base = "async function f(client) {
            const clientAdmins = await this.clientRepository.getOther(client);
            return clientAdmins.data;
        }";
    let head = "async function f(client) {
            const clientAdminsRaw = await this.clientRepository.getAdmins(client);
            const adminRows = readAdminRows(clientAdminsRaw);
            return adminRows;
        }";
    assert_flagged_raw(base, head, &unwrap_opts());
}

#[test]
fn gap2a_rejects_when_helper_not_in_unwrap_table() {
    let mut map = HashMap::new();
    map.insert("otherHelper".to_string(), "data".to_string());
    let mut opts = build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(vec!["readAdminRows".to_string()]),
        helper_call_site_substitution: true,
        narrowing_helpers_unwrap: Some(map),
        ..OptsOverrides::default()
    });
    opts.path = PathBuf::from("test.ts");
    let base = "async function f(client) {
            const clientAdmins = await this.clientRepository.getAdmins(client);
            return clientAdmins.data;
        }";
    let head = "async function f(client) {
            const clientAdminsRaw = await this.clientRepository.getAdmins(client);
            const adminRows = readAdminRows(clientAdminsRaw);
            return adminRows;
        }";
    assert_flagged_raw(base, head, &opts);
}
