//! Optional-chain pre-empt for the dual walker.
//!
//! `EXPR.foo` rewritten as `EXPR?.foo` (more defensive on head) is allowed:
//! the head adds a runtime null-guard but does not change the meaning when
//! `EXPR` is non-null. The reverse direction drops a guard the JS baseline
//! carried, so it is flagged as `less defensive` via `ArityMismatch`.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::dead_defensive_optional_chain::is_dead_defensive_chain;
use crate::findings::less_defensive_optional_chain;
use crate::walk::{
    walk_optional_chain_less_defensive, walk_optional_chain_more_defensive, WalkCtx,
};

pub(super) enum OptionalChainOutcome {
    Same,
    LessDefensive,
    MoreDefensive,
}

pub(super) fn is_optional_chain_capable(kind: &str) -> bool {
    matches!(
        kind,
        "member_expression" | "subscript_expression" | "call_expression"
    )
}

pub(super) fn optional_chain_outcome(base: Node, head: Node) -> OptionalChainOutcome {
    let base_opt = base.child_by_field_name("optional_chain").is_some();
    let head_opt = head.child_by_field_name("optional_chain").is_some();
    match (base_opt, head_opt) {
        (true, false) => OptionalChainOutcome::LessDefensive,
        (false, true) => OptionalChainOutcome::MoreDefensive,
        _ => OptionalChainOutcome::Same,
    }
}

/// Returns `true` if the optional-chain pre-empt consumed the pair and the
/// caller should not fall through to the regular child walk.
pub(super) fn handle_optional_chain(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !is_optional_chain_capable(base.kind()) {
        return false;
    }
    match optional_chain_outcome(base, head) {
        OptionalChainOutcome::LessDefensive => {
            if is_dead_defensive_chain(ctx, base, head) {
                walk_optional_chain_less_defensive(ctx, base, head, findings);
                return true;
            }
            findings.push(less_defensive_optional_chain(ctx, base, head));
            true
        }
        OptionalChainOutcome::MoreDefensive => {
            walk_optional_chain_more_defensive(ctx, base, head, findings);
            true
        }
        OptionalChainOutcome::Same => false,
    }
}
