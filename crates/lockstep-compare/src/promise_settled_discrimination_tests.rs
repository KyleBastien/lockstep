use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all,
    promise_settled_discrimination_opts, promise_settled_plus_non_null_alias_opts,
};

#[test]
fn single_binding_fulfilled_guard_absorbs() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.value.x;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r.status !== \"fulfilled\") return null;
        return r.value.x;
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn destructured_tuple_binding_absorbs() {
    let base = "async function f() {
        const [a, b] = await Promise.allSettled([P, Q]);
        log(a.value.x);
        return b.reason;
    }";
    let head = "async function f() {
        const [a, b] = await Promise.allSettled([P, Q]);
        if (a.status !== \"fulfilled\") return null;
        log(a.value.x);
        if (b.status !== \"rejected\") return null;
        return b.reason;
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn negated_eq_form_absorbs_with_reason_witness() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.reason.message;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r.status === \"fulfilled\") return null;
        return r.reason.message;
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn throw_terminator_absorbs() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.value.x;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r.status !== \"fulfilled\") throw new Error(\"x\");
        return r.value.x;
    }";
    assert_equiv_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn rejects_without_witness_access() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r.status !== \"fulfilled\") return null;
        return r;
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn rejects_when_name_reassigned() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.value.x;
    }";
    let head = "async function f() {
        let r = await Promise.allSettled(work());
        if (r.status !== \"fulfilled\") return null;
        r = other();
        return r.value.x;
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn rejects_when_name_not_bound_to_allsettled() {
    let base = "async function f() {
        const r = someOther();
        return r.value.x;
    }";
    let head = "async function f() {
        const r = someOther();
        if (r.status !== \"fulfilled\") return null;
        return r.value.x;
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn rejects_guard_subject_not_status() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.value.x;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r !== \"fulfilled\") return null;
        return r.value.x;
    }";
    assert_flagged_raw(base, head, &promise_settled_discrimination_opts());
}

#[test]
fn composes_with_non_null_alias_local() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.value.x;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r.status !== \"fulfilled\") return null;
        return r.value.x;
    }";
    assert_equiv_raw(base, head, &promise_settled_plus_non_null_alias_opts());
}

#[test]
fn is_gated_off_by_default() {
    let base = "async function f() {
        const r = await Promise.allSettled(work());
        return r.value.x;
    }";
    let head = "async function f() {
        const r = await Promise.allSettled(work());
        if (r.status !== \"fulfilled\") return null;
        return r.value.x;
    }";
    assert_flagged_raw(base, head, &opts_report_all());
}
