//! Walker state: configuration flags, scratch aliases, accumulator side-tables.
//!
//! Extracted from `walk.rs` so the dispatcher there stays readable and the
//! state-shape changes (new alias kinds, new flags) don't bloat the dispatch
//! module. All fields are crate-private.

use std::path::Path;

use crate::walk::CompareOptions;

#[derive(Clone)]
pub(super) struct WalkCtx<'a> {
    pub(super) base_src: &'a str,
    pub(super) head_src: &'a str,
    pub(super) path: &'a Path,
    pub(super) report_all: bool,
    pub(super) allow_constructor_assigned_method_equivalence: bool,
    pub(super) allow_closure_cache_field_alias: bool,
    pub(super) allow_array_first_element_or_null: bool,
    pub(super) allow_array_first_element_or_null_loose: bool,
    pub(super) allow_nullish_widening: bool,
    pub(super) allow_null_undefined_swap: bool,
    pub(super) allow_iife_async_wrapper: bool,
    pub(super) allow_transient_cache_wrap: bool,
    pub(super) allow_request_field_narrowing: bool,
    pub(super) allow_async_propagation: bool,
    pub(super) allow_defensive_null_guard: bool,
    pub(super) ignored_base_starts: Vec<usize>,
    pub(super) ignored_head_starts: Vec<usize>,
    pub(super) aliases: Vec<CacheAlias>,
    /// Transient head locals that should compare equal to a base member
    /// expression (`this.<property>`). Populated by `transient_cache_wrap`.
    pub(super) transient_locals: Vec<TransientLocal>,
    /// Head locals narrowed from a base member expression of the form
    /// `OBJ.PROP`. Populated by `request_field_narrowing`.
    pub(super) narrowed_request_fields: Vec<NarrowedRequestField>,
    /// Active for the body of a callable where `async_propagation` accepted
    /// a base-sync / head-async divergence. Allows `await EXPR` on head where
    /// base has bare `EXPR`.
    pub(super) async_propagation_active: bool,
}

impl<'a> WalkCtx<'a> {
    pub(super) fn from_opts(base_src: &'a str, head_src: &'a str, opts: &'a CompareOptions) -> Self {
        Self {
            base_src,
            head_src,
            path: &opts.path,
            report_all: opts.report_all,
            allow_constructor_assigned_method_equivalence: opts
                .allow_constructor_assigned_method_equivalence,
            allow_closure_cache_field_alias: opts.allow_closure_cache_field_alias,
            allow_array_first_element_or_null: opts.allow_array_first_element_or_null,
            allow_array_first_element_or_null_loose: opts.allow_array_first_element_or_null_loose,
            allow_nullish_widening: opts.allow_nullish_widening,
            allow_null_undefined_swap: opts.allow_null_undefined_swap,
            allow_iife_async_wrapper: opts.allow_iife_async_wrapper,
            allow_transient_cache_wrap: opts.allow_transient_cache_wrap,
            allow_request_field_narrowing: opts.allow_request_field_narrowing,
            allow_async_propagation: opts.allow_async_propagation,
            allow_defensive_null_guard: opts.allow_defensive_null_guard,
            ignored_base_starts: Vec::new(),
            ignored_head_starts: Vec::new(),
            aliases: Vec::new(),
            transient_locals: Vec::new(),
            narrowed_request_fields: Vec::new(),
            async_propagation_active: false,
        }
    }

    /// Clone with accumulator state cleared. Config flags and sources preserved.
    /// For sub-comparisons whose findings should not feed back into the outer pass.
    pub(super) fn scratch(&self) -> Self {
        let mut s = self.clone();
        s.ignored_base_starts.clear();
        s.ignored_head_starts.clear();
        s.aliases.clear();
        s.transient_locals.clear();
        s.narrowed_request_fields.clear();
        s.async_propagation_active = false;
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CacheAlias {
    pub(super) base_name: String,
    pub(super) head_property: String,
}

/// A head-side local identifier that should compare equal to a base
/// identifier (typically a cache name like `customerBillingCache`).
/// Registered by the transient-cache-wrap rule for the scope of the wrap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TransientLocal {
    pub(super) head_name: String,
    pub(super) base_name: String,
}

/// A head-side local that should compare equal to a base member expression
/// `OBJ.PROP`. Registered by the request-field-narrowing rule for the scope
/// of the enclosing block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NarrowedRequestField {
    pub(super) head_name: String,
    pub(super) base_object: String,
    pub(super) base_property: String,
}

#[derive(Clone, Copy)]
pub(super) enum Side {
    Base,
    Head,
}
