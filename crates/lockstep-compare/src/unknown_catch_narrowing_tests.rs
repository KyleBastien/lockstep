use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, unknown_catch_narrowing_opts,
};

#[test]
fn inline_ternary_with_string_call_absorbs() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : String(err)); }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn inline_ternary_with_optional_to_string_absorbs() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : err?.toString()); }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn inline_ternary_with_optional_prop_or_literal_absorbs() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : err?.message ?? \"unknown\"); }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn inline_ternary_with_string_literal_fallback_absorbs() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : \"unknown\"); }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn const_extraction_absorbs_with_alias_resolution() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); other(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            log(message);
            other(message);
        }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn nested_catch_uses_inner_binding() {
    let base = "function f() {
        try {
            inner();
        } catch (outer) {
            try { boom(); } catch (inner_err) { log(inner_err.message); }
        }
    }";
    let head = "function f() {
        try {
            inner();
        } catch (outer) {
            try { boom(); } catch (inner_err) {
                log(inner_err instanceof Error ? inner_err.message : String(inner_err));
            }
        }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn rejects_arrow_callback_outside_catch() {
    let base = "function f() { return (err) => log(err.message); }";
    let head =
        "function f() { return (err) => log(err instanceof Error ? err.message : String(err)); }";
    assert_flagged_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn rejects_instanceof_with_subclass() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof TypeError ? err.message : String(err)); }
    }";
    assert_flagged_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn rejects_unrelated_fallback_call() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : other(err)); }
    }";
    assert_flagged_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn is_gated_off_by_default() {
    let base = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : String(err)); }
    }";
    assert_flagged_raw(base, head, &opts_report_all());
}
