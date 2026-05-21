use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, promise_settled_discrimination_opts,
};

#[test]
fn arrow_body_expansion_with_string_call_fallback_absorbs() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = r.status === \"rejected\"
                ? r.reason instanceof Error
                    ? r.reason.message
                    : String(r.reason)
                : \"\";
            log(`failed: ${reason}`);
        });
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_with_status_not_fulfilled_form_absorbs() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = r.status !== \"fulfilled\"
                ? r.reason instanceof Error
                    ? r.reason.message
                    : String(r.reason)
                : \"\";
            log(`failed: ${reason}`);
        });
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_with_optional_to_string_fallback_absorbs() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = r.status === \"rejected\"
                ? r.reason instanceof Error
                    ? r.reason.message
                    : r.reason?.toString()
                : \"\";
            log(`failed: ${reason}`);
        });
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_rejects_work_beyond_extract_and_use() {
    // Trailing body does more than just consume LOCAL — should still flag.
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = r.status === \"rejected\"
                ? r.reason instanceof Error
                    ? r.reason.message
                    : String(r.reason)
                : \"\";
            doExtraWork();
            log(`failed: ${reason}`);
        });
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_rejects_unrelated_const_value() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = otherSource();
            log(`failed: ${reason}`);
        });
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_rejects_status_check_for_different_result() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = q.status === \"rejected\"
                ? q.reason instanceof Error
                    ? q.reason.message
                    : String(q.reason)
                : \"\";
            log(`failed: ${reason}`);
        });
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_rejects_non_safe_default() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = r.status === \"rejected\"
                ? r.reason instanceof Error
                    ? r.reason.message
                    : String(r.reason)
                : someComputation();
            log(`failed: ${reason}`);
        });
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn arrow_body_expansion_is_gated_off_by_default() {
    let base = "function f(r) {
        handleResult(r, () => log(`failed: ${r.reason.message}`));
    }";
    let head = "function f(r) {
        handleResult(r, () => {
            const reason = r.status === \"rejected\"
                ? r.reason instanceof Error
                    ? r.reason.message
                    : String(r.reason)
                : \"\";
            log(`failed: ${reason}`);
        });
    }";
    assert_flagged_raw(base, head, &opts_report_all());
}
