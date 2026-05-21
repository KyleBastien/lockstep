use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, unknown_catch_narrowing_opts,
};

const BASE_INLINE: &str = "function f() {
        try { work(); } catch (err) { log(err.message); }
    }";

fn head_inline(fallback: &str) -> String {
    format!(
        "function f() {{
        try {{ work(); }} catch (err) {{ log(err instanceof Error ? err.message : {fallback}); }}
    }}"
    )
}

fn assert_inline_equiv(fallback: &str) {
    assert_equiv_raw(
        BASE_INLINE,
        &head_inline(fallback),
        &unknown_catch_narrowing_opts(),
    );
}

fn assert_inline_flagged(fallback: &str) {
    assert_flagged_raw(
        BASE_INLINE,
        &head_inline(fallback),
        &unknown_catch_narrowing_opts(),
    );
}

#[test]
fn inline_ternary_with_string_call_absorbs() {
    assert_inline_equiv("String(err)");
}

#[test]
fn inline_ternary_with_optional_to_string_absorbs() {
    assert_inline_equiv("err?.toString()");
}

#[test]
fn inline_ternary_with_optional_prop_or_literal_absorbs() {
    assert_inline_equiv("err?.message ?? \"unknown\"");
}

#[test]
fn inline_ternary_with_string_literal_fallback_absorbs() {
    assert_inline_equiv("\"unknown\"");
}

#[test]
fn inline_ternary_with_typeof_string_ternary_fallback_absorbs() {
    assert_inline_equiv("(typeof err === \"string\" ? err : \"unknown\")");
}

#[test]
fn inline_ternary_with_string_call_nullish_fallback_absorbs() {
    assert_inline_equiv("String(err ?? \"unknown\")");
}

#[test]
fn inline_ternary_with_chained_optional_prop_or_literal_absorbs() {
    assert_inline_equiv("err?.message ?? err?.detail ?? \"unknown\"");
}

#[test]
fn rejects_typeof_ternary_with_wrong_type() {
    assert_inline_flagged("(typeof err === \"number\" ? err : \"unknown\")");
}

#[test]
fn rejects_string_call_with_non_err_nullish() {
    assert_inline_flagged("String(other ?? \"unknown\")");
}

#[test]
fn rejects_chained_nullish_with_non_string_tail() {
    assert_inline_flagged("err?.message ?? err?.detail ?? other()");
}

#[test]
fn rejects_instanceof_with_subclass() {
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof TypeError ? err.message : String(err)); }
    }";
    assert_flagged_raw(BASE_INLINE, head, &unknown_catch_narrowing_opts());
}

#[test]
fn rejects_unrelated_fallback_call() {
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : other(err)); }
    }";
    assert_flagged_raw(BASE_INLINE, head, &unknown_catch_narrowing_opts());
}

#[test]
fn is_gated_off_by_default() {
    let head = "function f() {
        try { work(); } catch (err) { log(err instanceof Error ? err.message : String(err)); }
    }";
    assert_flagged_raw(BASE_INLINE, head, &opts_report_all());
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
fn const_extraction_resolves_inside_template_literal() {
    let base = "function f() {
        try { work(); } catch (err) { logger.error(`error: ${err.message}`); }
    }";
    let head = "function f() {
        try { work(); } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            logger.error(`error: ${message}`);
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
fn const_between_other_catch_stmts_absorbs() {
    let base = "function f() {
        try { work(); } catch (err) {
            audit(err);
            log(err.message);
            cleanup();
        }
    }";
    let head = "function f() {
        try { work(); } catch (err) {
            audit(err);
            const message = err instanceof Error ? err.message : String(err);
            log(message);
            cleanup();
        }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn const_used_across_three_templates_and_bare_arg_absorbs() {
    let base = "function f() {
        try { work(); } catch (err) {
            logger.error(`first: ${err.message}`);
            logger.warn(`second: ${err.message}`);
            logger.info(`third: ${err.message}`);
            forward(err.message);
        }
    }";
    let head = "function f() {
        try { work(); } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            logger.error(`first: ${message}`);
            logger.warn(`second: ${message}`);
            logger.info(`third: ${message}`);
            forward(message);
        }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn gap1_promise_catch_arrow_callback_const_extract_absorbs() {
    let base = "function f(p) {
        return p.catch((err) => {
            this.logger.error(`x ${err.message}`);
            return null;
        });
    }";
    let head = "function f(p) {
        return p.catch((err) => {
            const message = err instanceof Error ? err.message : String(err);
            this.logger.error(`x ${message}`);
            return null;
        });
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn gap1_promise_catch_arrow_callback_inline_ternary_absorbs() {
    let base = "function f(p) {
        return p.catch((err) => {
            this.logger.error(`x ${err.message}`);
        });
    }";
    let head = "function f(p) {
        return p.catch((err) => {
            this.logger.error(`x ${err instanceof Error ? err.message : String(err)}`);
        });
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn gap1_rejects_arbitrary_arrow_callback_not_in_catch() {
    // Same shape but the arrow isn't a `.catch` callback. Must stay flagged.
    let base = "function f(arr) { return arr.map((err) => err.message); }";
    let head = "function f(arr) {
        return arr.map((err) => {
            const message = err instanceof Error ? err.message : String(err);
            return message;
        });
    }";
    assert_flagged_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn gap1_real_world_logger_error_template_with_message_absorbs() {
    let base = "async function fetchClientData(client) {
        try {
            return await get(client);
        } catch (err) {
            this.logger.error(`Couldn't fetch client data - ${client} - ${err.message}`);
            return null;
        }
    }";
    let head = "async function fetchClientData(client) {
        try {
            return await get(client);
        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            this.logger.error(`Couldn't fetch client data - ${client} - ${message}`);
            return null;
        }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}

#[test]
fn two_catches_same_try_with_different_bindings_absorbs() {
    let base = "function f() {
        try {
            try { work(); } catch (inner) { log(inner.message); }
        } catch (outer) {
            log(outer.message);
        }
    }";
    let head = "function f() {
        try {
            try { work(); } catch (inner) {
                const innerMessage = inner instanceof Error ? inner.message : String(inner);
                log(innerMessage);
            }
        } catch (outer) {
            const outerMessage = outer instanceof Error ? outer.message : String(outer);
            log(outerMessage);
        }
    }";
    assert_equiv_raw(base, head, &unknown_catch_narrowing_opts());
}
