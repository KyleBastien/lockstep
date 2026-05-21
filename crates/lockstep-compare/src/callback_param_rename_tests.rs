use crate::test_helpers::{assert_equiv_raw, assert_flagged_raw, opts_report_all};

fn assert_rename_equiv(base: &str, head: &str) {
    assert_equiv_raw(base, head, &opts_report_all());
}

fn assert_rename_flagged(base: &str, head: &str) {
    assert_flagged_raw(base, head, &opts_report_all());
}

#[test]
fn gap3_arrow_single_param_rename_in_map_callback_absorbs() {
    assert_rename_equiv(
        "function f(arr) { return arr.map((client) => client.email); }",
        "function f(arr) { return arr.map((admin) => admin.email); }",
    );
}

#[test]
fn gap3_arrow_single_param_rename_in_filter_callback_absorbs() {
    assert_rename_equiv(
        "function f(arr) { return arr.filter((row) => row.active); }",
        "function f(arr) { return arr.filter((entry) => entry.active); }",
    );
}

#[test]
fn gap3_arrow_object_literal_body_with_rename_absorbs() {
    assert_rename_equiv(
        "function f(arr) { return arr.map((client) => ({ email: client.email })); }",
        "function f(arr) { return arr.map((admin) => ({ email: admin.email })); }",
    );
}

#[test]
fn gap3_nested_callback_rename_absorbs() {
    assert_rename_equiv(
        "function f(arr) {
            return arr.map((client) => client.emails.map((e) => e.id));
        }",
        "function f(arr) {
            return arr.map((admin) => admin.emails.map((m) => m.id));
        }",
    );
}

#[test]
fn gap3_function_expression_param_rename_absorbs() {
    assert_rename_equiv(
        "function f(arr) { return arr.map(function (client) { return client.id; }); }",
        "function f(arr) { return arr.map(function (admin) { return admin.id; }); }",
    );
}

#[test]
fn gap3_rejects_when_base_body_references_head_param_name() {
    // base body uses `admin` as a closure reference; head body uses `admin`
    // as its own parameter. After naive rename they'd look equal but bind
    // differently at runtime. Reject.
    assert_rename_flagged(
        "function f(arr, admin) { return arr.map((client) => admin.id + client.id); }",
        "function f(arr, admin) { return arr.map((admin) => admin.id + admin.id); }",
    );
}

#[test]
fn gap3_rejects_when_head_body_references_base_param_name() {
    assert_rename_flagged(
        "function f(arr) { return arr.map((client) => client.id); }",
        "function f(arr, client) { return arr.map((admin) => admin.id + client.id); }",
    );
}

#[test]
fn gap3_same_param_name_still_compares_normally() {
    assert_rename_equiv(
        "function f(arr) { return arr.map((x) => x.id); }",
        "function f(arr) { return arr.map((x) => x.id); }",
    );
}

#[test]
fn gap3_multi_param_callback_skipped() {
    // (idx, item) — multi-param case is out of scope; difference flags as normal.
    assert_rename_flagged(
        "function f(arr) { return arr.map((idx, item) => item.id); }",
        "function f(arr) { return arr.map((i, entry) => entry.id); }",
    );
}

#[test]
fn gap3_destructured_param_skipped() {
    assert_rename_flagged(
        "function f(arr) { return arr.map(({ id }) => id); }",
        "function f(arr) { return arr.map(({ uuid }) => uuid); }",
    );
}
