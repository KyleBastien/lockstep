use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, request_narrowing_opts,
};

const BASE_DIRECT: &str = "function f() {
    if (!!this._request.uuid) {
        useDirectly(this._request.uuid);
    }
}";

const HEAD_NARROWED: &str = "function f() {
    const requestUuid = \"uuid\" in this._request && typeof this._request.uuid === \"string\" ? this._request.uuid : undefined;
    if (!!requestUuid) {
        useDirectly(requestUuid);
    }
}";

#[test]
fn request_field_narrowing_absorbs_extraction() {
    assert_equiv_raw(BASE_DIRECT, HEAD_NARROWED, &request_narrowing_opts());
}

#[test]
fn request_field_narrowing_is_gated_off_by_default() {
    assert_flagged_raw(BASE_DIRECT, HEAD_NARROWED, &opts_report_all());
}

#[test]
fn request_field_narrowing_rejects_property_mismatch() {
    let base = "function f() { useDirectly(this._request.uuid); }";
    let head = "function f() {
        const requestUuid = \"name\" in this._request && typeof this._request.name === \"string\" ? this._request.name : undefined;
        useDirectly(requestUuid);
    }";
    assert_flagged_raw(base, head, &request_narrowing_opts());
}

#[test]
fn request_field_narrowing_rejects_unused_local() {
    let base = "function f() { useDirectly(); }";
    let head = "function f() {
        const requestUuid = \"uuid\" in this._request && typeof this._request.uuid === \"string\" ? this._request.uuid : undefined;
        useDirectly();
    }";
    assert_flagged_raw(base, head, &request_narrowing_opts());
}

#[test]
fn request_field_narrowing_rejects_wrong_alternative() {
    let base = "function f() { useDirectly(this._request.uuid); }";
    let head = "function f() {
        const requestUuid = \"uuid\" in this._request && typeof this._request.uuid === \"string\" ? this._request.uuid : null;
        useDirectly(requestUuid);
    }";
    assert_flagged_raw(base, head, &request_narrowing_opts());
}
