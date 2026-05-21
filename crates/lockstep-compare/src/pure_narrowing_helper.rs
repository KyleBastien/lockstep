//! Equivalence rule for pure type-narrowing helpers.
//!
//! Gated on `allow_pure_narrowing_helper`. TypeScript migrations frequently
//! introduce small helpers like `asString` / `asNumber` / `isPlainObject`
//! that wrap a `typeof` check around a value and return `undefined` (or
//! `false`) on type mismatch. Use sites then look like:
//!
//! ```text
//! // base
//! const x = obj.foo;
//! // head
//! const x = asString(obj.foo) ?? "";
//! ```
//!
//! Two mechanisms compose:
//!
//! 1. **Top-level declaration filter.** Head `function HELPER(...) { ... }`
//!    declarations whose name is listed in `narrowing_helpers` are dropped
//!    from the head program's comparable children. The base program has no
//!    corresponding declaration, so this avoids an arity mismatch.
//!
//! 2. **Call-site equivalence.** A head `binary_expression` of shape
//!    `HELPER(EXPR) ?? DEFAULT` (where `HELPER` was filtered in step 1) is
//!    treated as equivalent to a base expression that matches `EXPR`. A
//!    scratch sub-walk verifies the `EXPR` shape against the base node.
//!
//! **WARNING — observable behavior change on type mismatch.** When `EXPR`
//! evaluates to a value of the wrong runtime type (e.g. `asString(123)`),
//! the head substitutes `DEFAULT` while base interpolates / propagates the
//! raw value. The two flags (`allow_pure_narrowing_helper` and a non-empty
//! `narrowing_helpers`) are both required, and the helper name must also
//! be declared in head — these stack to keep the rule narrowly opt-in.
//!
//! V1 requirements at the call site:
//! - Helper name ∈ `narrowing_helpers` config.
//! - Helper declaration found in head program top-level.
//! - Call shape is one of:
//!   - `HELPER(EXPR) ?? DEFAULT` — binary `??` with call on the left, classic
//!     "narrow-to-value-or-undefined" helpers (`asString`, `asNumber`).
//!   - `HELPER(EXPR) ? EXPR : DEFAULT` — ternary with the call on the
//!     condition, the same `EXPR` on the consequence, and a safe-default
//!     literal on the alternative. Covers type-predicate helpers
//!     (`isPlainObject`, `isAdmin`, …) whose use site preserves the value
//!     when the predicate holds and substitutes a literal otherwise.
//!
//! Bare `HELPER(EXPR)` without either guard is out of scope: the migration
//! would have to know the helper's specific narrowing target to be
//! equivalence-preserving, and we don't carry that information in config.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::walk::{walk, WalkCtx};

/// One-time top-level scan: registers each `function HELPER(...) { ... }`
/// declaration whose name is configured. Pushes its byte start onto
/// `ignored_head_starts` so the structural compare won't see the extra
/// declaration, and records the name in `recognized_narrowing_helpers` so
/// call-site matching can verify the helper is locally declared.
pub(super) fn register_narrowing_helper_declarations(ctx: &mut WalkCtx, head_root: Node) {
    if !ctx.allow_pure_narrowing_helper || ctx.narrowing_helpers.is_empty() {
        return;
    }
    let mut cursor = head_root.walk();
    for child in head_root.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        let Some(name) = declared_function_name(child, ctx.head_src) else {
            continue;
        };
        if !ctx.narrowing_helpers.iter().any(|h| h == &name) {
            continue;
        }
        ctx.ignored_head_starts.push(child.start_byte());
        if !ctx.recognized_narrowing_helpers.contains(&name) {
            ctx.recognized_narrowing_helpers.push(name);
        }
    }
}

fn declared_function_name(node: Node, src: &str) -> Option<String> {
    if node.kind() != "function_declaration" {
        return None;
    }
    let name = node.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    Some(node_text(name, src))
}

/// Pair pre-empt for `HELPER(EXPR) ?? DEFAULT` ↔ base `EXPR`.
///
/// Returns `true` when the head shape matches a registered helper call and
/// the inner `EXPR` sub-walks to no findings against `base`.
pub(super) fn is_pure_narrowing_helper_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if !ctx.allow_pure_narrowing_helper {
        return false;
    }
    if ctx.recognized_narrowing_helpers.is_empty() {
        return false;
    }
    let Some(inner) = extract_helper_call_inner(ctx, head) else {
        return false;
    };
    sub_walk_clean(ctx, base, inner)
}

fn extract_helper_call_inner<'a>(ctx: &WalkCtx, head: Node<'a>) -> Option<Node<'a>> {
    if let Some(inner) = extract_from_nullish_default(ctx, head) {
        return Some(inner);
    }
    extract_from_type_predicate_ternary(ctx, head)
}

fn extract_from_nullish_default<'a>(ctx: &WalkCtx, head: Node<'a>) -> Option<Node<'a>> {
    let call = unwrap_nullish_default_call(head, ctx.head_src)?;
    let _helper = recognized_helper_name(ctx, call)?;
    sole_call_argument(call)
}

/// `HELPER(EXPR) ? EXPR : DEFAULT` — the condition is the registered helper
/// call, the consequence textually matches the helper's sole argument, and
/// the alternative is a literal safe default. Returns the call argument so
/// the outer sub-walk can verify base ~ `EXPR`.
fn extract_from_type_predicate_ternary<'a>(ctx: &WalkCtx, head: Node<'a>) -> Option<Node<'a>> {
    let (call, consequence, alternative) = ternary_parts(head)?;
    if call.kind() != "call_expression" {
        return None;
    }
    let _helper = recognized_helper_name(ctx, call)?;
    let arg = sole_call_argument(call)?;
    if compact_node_text(arg, ctx.head_src) != compact_node_text(consequence, ctx.head_src) {
        return None;
    }
    if !is_safe_default(alternative, ctx.head_src) {
        return None;
    }
    Some(arg)
}

fn ternary_parts<'a>(node: Node<'a>) -> Option<(Node<'a>, Node<'a>, Node<'a>)> {
    if node.kind() != "ternary_expression" {
        return None;
    }
    Some((
        unwrap_parens(node.child_by_field_name("condition")?),
        unwrap_parens(node.child_by_field_name("consequence")?),
        unwrap_parens(node.child_by_field_name("alternative")?),
    ))
}

/// Literal "safe defaults" the migration may substitute when the type
/// predicate is false: empty container, scalar literal, or the bare
/// `undefined` identifier.
fn is_safe_default(node: Node, src: &str) -> bool {
    match node.kind() {
        "string" | "number" | "null" | "true" | "false" => true,
        "undefined" => true,
        "object" => {
            raw_comparable_children(node)
                .into_iter()
                .filter(|n| n.is_named())
                .count()
                == 0
        }
        "array" => {
            raw_comparable_children(node)
                .into_iter()
                .filter(|n| n.is_named())
                .count()
                == 0
        }
        "identifier" => node_text(node, src) == "undefined",
        _ => false,
    }
}

/// Returns the call-expression on the left of a `?? DEFAULT` binary
/// expression. Both the operator check and the left-side `call_expression`
/// kind check live here so the caller stays narrow.
fn unwrap_nullish_default_call<'a>(head: Node<'a>, src: &str) -> Option<Node<'a>> {
    if head.kind() != "binary_expression" {
        return None;
    }
    if node_text(head.child_by_field_name("operator")?, src) != "??" {
        return None;
    }
    let left = unwrap_parens(head.child_by_field_name("left")?);
    if left.kind() != "call_expression" {
        return None;
    }
    Some(left)
}

fn recognized_helper_name(ctx: &WalkCtx, call: Node) -> Option<String> {
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "identifier" {
        return None;
    }
    let name = node_text(callee, ctx.head_src);
    ctx.recognized_narrowing_helpers
        .iter()
        .find(|h| *h == &name)
        .map(|_| name)
}

fn sole_call_argument(call: Node) -> Option<Node> {
    let arguments = call.child_by_field_name("arguments")?;
    let args: Vec<Node> = raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if args.len() != 1 {
        return None;
    }
    Some(unwrap_parens(args[0]))
}

fn sub_walk_clean(ctx: &WalkCtx, base: Node, head_inner: Node) -> bool {
    let scratch = ctx.scratch();
    let mut findings: Vec<Finding> = Vec::new();
    walk(&scratch, base, head_inner, &mut findings);
    findings.is_empty()
}

fn unwrap_parens(mut node: Node) -> Node {
    while node.kind() == "parenthesized_expression" {
        match first_named_child(node) {
            Some(child) => node = child,
            None => break,
        }
    }
    node
}
