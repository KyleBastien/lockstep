//! Log-consumer witness for the dead-defensive optional-chain rule.
//!
//! Composable witness layered on top of
//! [`crate::dead_defensive_optional_chain`]. The witness fires when a base
//! optional-chain node lexically nests inside an argument list of a
//! recognized log-only consumer call. Configured via
//! `dead_defensive_log_consumer_methods` — a list of dotted method paths
//! matched by exact-suffix on the callee's compact text (boundary must be
//! `.` or start-of-text). Empty list (default) leaves the rule unchanged.
//!
//! **WARNING — runtime-edge divergence.** When the chained object is
//! actually nullish at runtime, base interpolates the JS string
//! `"undefined"` (or passes `undefined` directly) into the logger call;
//! head throws on the bare property access. Acceptable in the same
//! trade-off envelope as the existing if-statement deadness witness, but
//! callers must opt in by populating the allowlist.

use tree_sitter::Node;

use crate::node_utils::compact_node_text;
use crate::walk::WalkCtx;

/// True when `base` (an optional-chain `member_expression`) is inside the
/// argument list of a recognized log-consumer call. The first
/// `call_expression` ancestor walking outward must be the matching
/// logger — any intermediate non-logger call kills the match (enforces
/// "no intermediate call layer").
pub(super) fn log_consumer_witness(ctx: &WalkCtx, base: Node) -> bool {
    if ctx.dead_defensive_log_consumer_methods.is_empty() {
        return false;
    }
    let mut current = base;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression" {
            return callee_matches_log_consumer(parent, ctx);
        }
        current = parent;
    }
    false
}

fn callee_matches_log_consumer(call: Node, ctx: &WalkCtx) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    let callee_text = compact_node_text(callee, ctx.base_src);
    ctx.dead_defensive_log_consumer_methods
        .iter()
        .any(|m| callee_text == *m || callee_text.ends_with(&format!(".{m}")))
}
