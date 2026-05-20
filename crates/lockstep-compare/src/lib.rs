//! Structural AST comparator. Walks two JS trees in lockstep and emits
//! `Finding`s for the first (or every) divergence.

mod align;
mod array_first_equivalence;
mod async_propagation;
mod callable_equivalence;
mod class_equivalence;
mod compare_options;
mod dead_defensive_optional_chain;
mod defensive_log_guard;
mod defensive_null_guard;
mod findings;
mod iife_async_wrapper;
mod node_utils;
mod non_null_alias_local;
mod nullish_widening_equivalence;
mod optional_chain;
mod promise_settled_discrimination;
mod pure_narrowing_helper;
mod request_field_narrowing;
mod transient_cache_wrap;
mod unknown_catch_narrowing;

pub mod report;
pub mod tokens;
pub mod walk;
mod walk_ctx;

pub use compare_options::CompareOptions;
pub use walk::compare;

#[cfg(test)]
mod async_propagation_tests;
#[cfg(test)]
mod dead_defensive_optional_chain_tests;
#[cfg(test)]
mod defensive_log_guard_tests;
#[cfg(test)]
mod defensive_null_guard_tests;
#[cfg(test)]
mod iife_async_wrapper_tests;
#[cfg(test)]
mod non_null_alias_local_tests;
#[cfg(test)]
mod promise_settled_discrimination_tests;
#[cfg(test)]
mod pure_narrowing_helper_tests;
#[cfg(test)]
mod request_field_narrowing_tests;
#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod transient_cache_wrap_tests;
#[cfg(test)]
mod unknown_catch_narrowing_tests;
#[cfg(test)]
mod walk_tests;
#[cfg(test)]
mod widening_tests;
