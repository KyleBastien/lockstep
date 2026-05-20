use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, defensive_log_guard_custom_methods_opts,
    defensive_log_guard_opts, opts_report_all,
};

#[test]
fn defensive_log_guard_absorbs_inserted_wrap() {
    let base = "function f() {
        this._logger.debug(cache, \"FOUND\");
    }";
    let head = "function f() {
        if (cache) {
            this._logger.debug(cache, \"FOUND\");
        }
    }";
    assert_equiv_raw(base, head, &defensive_log_guard_opts());
}

#[test]
fn defensive_log_guard_handles_this_property_subject() {
    let base = "function f() {
        this._logger.debug(this._cache, \"FOUND\");
    }";
    let head = "function f() {
        if (this._cache) {
            this._logger.debug(this._cache, \"FOUND\");
        }
    }";
    assert_equiv_raw(base, head, &defensive_log_guard_opts());
}

#[test]
fn defensive_log_guard_is_gated_off_by_default() {
    let base = "function f() { this._logger.debug(cache, \"x\"); }";
    let head = "function f() { if (cache) { this._logger.debug(cache, \"x\"); } }";
    assert_flagged_raw(base, head, &opts_report_all());
}

#[test]
fn defensive_log_guard_rejects_condition_mismatch() {
    let base = "function f() { this._logger.debug(cache, \"x\"); }";
    let head = "function f() {
        if (other) {
            this._logger.debug(cache, \"x\");
        }
    }";
    assert_flagged_raw(base, head, &defensive_log_guard_opts());
}

#[test]
fn defensive_log_guard_rejects_non_logger_method() {
    let base = "function f() { cache.save(); }";
    let head = "function f() {
        if (cache) {
            cache.save();
        }
    }";
    assert_flagged_raw(base, head, &defensive_log_guard_opts());
}

#[test]
fn defensive_log_guard_rejects_multi_statement_body() {
    let base = "function f() { this._logger.debug(cache, \"x\"); }";
    let head = "function f() {
        if (cache) {
            this._logger.debug(cache, \"x\");
            sideEffect();
        }
    }";
    assert_flagged_raw(base, head, &defensive_log_guard_opts());
}

#[test]
fn defensive_log_guard_rejects_zero_argument_call() {
    let base = "function f() { this._logger.flush(); }";
    let head = "function f() {
        if (cache) {
            this._logger.flush();
        }
    }";
    assert_flagged_raw(base, head, &defensive_log_guard_opts());
}

#[test]
fn defensive_log_guard_custom_method_list() {
    let base = "function f() { this._tele.telemetry(cache, \"x\"); }";
    let head = "function f() {
        if (cache) {
            this._tele.telemetry(cache, \"x\");
        }
    }";
    let opts = defensive_log_guard_custom_methods_opts(vec!["telemetry".into()]);
    assert_equiv_raw(base, head, &opts);
}

#[test]
fn defensive_log_guard_directionality_base_has_guard_head_removed_flags() {
    let base = "function f() {
        if (cache) {
            this._logger.debug(cache, \"x\");
        }
    }";
    let head = "function f() {
        this._logger.debug(cache, \"x\");
    }";
    assert_flagged_raw(base, head, &defensive_log_guard_opts());
}
