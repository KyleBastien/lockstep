//! Equivalence rule for the "dead defensive optional chain" pattern.
//!
//! Gated on `allow_dead_defensive_optional_chain_removal`. Treats a head-side
//! removal of an optional chain (`OBJ?.PROP` → `OBJ.PROP`) as equivalent to
//! the base when the surrounding block proves the chained object can never
//! be null/undefined at runtime — making the `?.` dead defensive code in JS.
//!
//! Concretely, the rule fires when the optional chain sits in the condition
//! of an `if` whose `then` branch contains an unsafe write to the chain's
//! target object (e.g. `OBJ.X = ...`, `OBJ[i] = ...`, `Object.assign(OBJ, ...)`).
//! If `OBJ` were null/undefined at runtime, the write would itself throw —
//! so the optional chain's null-guard never meaningfully changed behavior.
//!
//! **WARNING — observable behavior change at the error location.** Base and
//! head differ when `OBJ` is null/undefined at runtime: base reads
//! `undefined` from the optional chain, takes the truthy/falsy branch, and
//! then throws on the write; head throws on the condition itself. The exact
//! error and call site differ — relevant only to error monitoring, not to
//! product behavior. The rule defaults OFF.
//!
//! Directional: only the head removes the `?.`. The MoreDefensive direction
//! (head adds `?.`) is handled by `walk_optional_chain_more_defensive` and is
//! not affected.

use tree_sitter::Node;

use crate::dead_defensive_log_consumer::log_consumer_witness;
use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::walk::WalkCtx;

/// Returns `true` if the base `?.` chain is dead defensive — i.e. an
/// equivalent removal under this rule, given the head shape and surrounding
/// block context. Caller has already verified base has `?.` and head does
/// not, and both nodes are optional-chain-capable.
pub(super) fn is_dead_defensive_chain(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if !ctx.allow_dead_defensive_optional_chain_removal {
        return false;
    }
    if base.kind() != "member_expression" || head.kind() != "member_expression" {
        return false;
    }
    let Some(base_object) = base.child_by_field_name("object") else {
        return false;
    };
    let Some(base_property) = base.child_by_field_name("property") else {
        return false;
    };
    let Some(head_object) = head.child_by_field_name("object") else {
        return false;
    };
    let Some(head_property) = head.child_by_field_name("property") else {
        return false;
    };

    if !same_property_path(ctx, base_object, base_property, head_object, head_property) {
        return false;
    }

    let obj_text = compact_node_text(base_object, ctx.base_src);
    if obj_text.is_empty() {
        return false;
    }

    if if_unsafe_write_witness(ctx, base, &obj_text) {
        return true;
    }
    log_consumer_witness(ctx, base)
}

/// Existing witness: the optional chain sits in the condition of an `if`
/// whose `then` branch contains an unguarded unsafe write to `obj_text`.
fn if_unsafe_write_witness(ctx: &WalkCtx, base: Node, obj_text: &str) -> bool {
    let Some(if_stmt) = enclosing_if_with_condition(base) else {
        return false;
    };
    let Some(consequence) = if_stmt.child_by_field_name("consequence") else {
        return false;
    };
    let body = unwrap_block(consequence);
    deadness_witness(body, ctx.base_src, obj_text)
}

/// Returns `true` if `obj` and `property` match between base and head, either
/// directly or via active alias rules. Single-link chains only — the rule
/// does not descend into nested member expressions on the object side
/// because the deadness witness reasons about the immediate chained value.
fn same_property_path(
    ctx: &WalkCtx,
    base_object: Node,
    base_property: Node,
    head_object: Node,
    head_property: Node,
) -> bool {
    if compact_node_text(base_property, ctx.base_src)
        != compact_node_text(head_property, ctx.head_src)
    {
        return false;
    }
    object_matches(ctx, base_object, head_object)
}

fn object_matches(ctx: &WalkCtx, base_object: Node, head_object: Node) -> bool {
    let base_text = compact_node_text(base_object, ctx.base_src);
    let head_text = compact_node_text(head_object, ctx.head_src);
    if base_text == head_text {
        return true;
    }
    if matches_cache_field_alias(ctx, base_object, head_object, &base_text) {
        return true;
    }
    matches_non_null_alias(ctx, head_object, &base_text, &head_text)
}

/// Cache-alias substitution: base bare identifier `X` resolves to head
/// `this.PROP` via [`crate::walk::CacheAlias`].
fn matches_cache_field_alias(
    ctx: &WalkCtx,
    base_object: Node,
    head_object: Node,
    base_text: &str,
) -> bool {
    if base_object.kind() != "identifier" || head_object.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = this_property(head_object, ctx.head_src) else {
        return false;
    };
    ctx.aliases
        .iter()
        .any(|a| a.base_name == base_text && a.head_property == prop)
}

/// Non-null alias local substitution: head local `LOCAL` was extracted as
/// `const LOCAL = CACHE;`. Either `CACHE` matches `base_text` directly, or
/// it does so transitively via a cache-field alias.
fn matches_non_null_alias(
    ctx: &WalkCtx,
    head_object: Node,
    base_text: &str,
    head_text: &str,
) -> bool {
    if head_object.kind() != "identifier" {
        return false;
    }
    let Some(alias) = ctx
        .non_null_aliases
        .iter()
        .find(|a| a.head_local == head_text)
    else {
        return false;
    };
    if alias.base_target_text == base_text {
        return true;
    }
    let Some(prop) = alias.head_this_property.as_ref() else {
        return false;
    };
    ctx.aliases
        .iter()
        .any(|a| a.base_name == base_text && a.head_property == *prop)
}

fn this_property(node: Node, src: &str) -> Option<String> {
    if node.kind() != "member_expression" {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if node_text(object, src) != "this" {
        return None;
    }
    node.child_by_field_name("property")
        .map(|p| node_text(p, src))
}

/// Climb from `node` until reaching an `if_statement` whose condition
/// subtree contains `node`. Returns the if statement.
fn enclosing_if_with_condition<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if if_condition_contains(parent, node) {
            return Some(parent);
        }
        current = parent;
    }
    None
}

fn if_condition_contains(if_stmt: Node, target: Node) -> bool {
    if if_stmt.kind() != "if_statement" {
        return false;
    }
    let Some(cond) = if_stmt.child_by_field_name("condition") else {
        return false;
    };
    cond.start_byte() <= target.start_byte() && target.end_byte() <= cond.end_byte()
}

fn unwrap_block(node: Node) -> Node {
    if node.kind() == "statement_block" {
        return node;
    }
    node
}

/// Walks `body` looking for at least one unguarded unsafe write to `obj`.
/// Returns `false` if `obj` is reassigned anywhere in the body — the
/// deadness inference cannot be trusted if the identifier may now point
/// somewhere else by the time of the write.
fn deadness_witness(body: Node, src: &str, obj: &str) -> bool {
    if is_reassigned(body, src, obj) {
        return false;
    }
    has_unguarded_unsafe_write(body, src, obj)
}

/// Recursive search for an unsafe write to `obj` that is not nested inside
/// a guard `if (obj)` / `if (obj != null)` / etc.
fn has_unguarded_unsafe_write(node: Node, src: &str, obj: &str) -> bool {
    if is_unsafe_write(node, src, obj) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child_witnesses_unguarded_write(child, src, obj) {
            return true;
        }
    }
    false
}

fn child_witnesses_unguarded_write(child: Node, src: &str, obj: &str) -> bool {
    if child.kind() == "if_statement" && condition_guards_object(child, src, obj) {
        return guarded_if_else_witnesses(child, src, obj);
    }
    has_unguarded_unsafe_write(child, src, obj)
}

/// Guarded `if (OBJ) { … }`: writes in the `then` branch are protected and
/// do not witness deadness, but we still descend into the `else` branch.
fn guarded_if_else_witnesses(if_stmt: Node, src: &str, obj: &str) -> bool {
    let Some(alt) = if_stmt.child_by_field_name("alternative") else {
        return false;
    };
    has_unguarded_unsafe_write(alt, src, obj)
}

/// Recognizes:
///   - `OBJ.PROP = ...`
///   - `OBJ[EXPR] = ...`
///   - augmented forms (`+=`, etc.)
///   - `Object.assign(OBJ, ...)` and `Object.defineProperty(OBJ, ...)`
fn is_unsafe_write(node: Node, src: &str, obj: &str) -> bool {
    match node.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else {
                return false;
            };
            left_is_member_of(left, src, obj)
        }
        "call_expression" => is_object_method_call_on(node, src, obj),
        _ => false,
    }
}

fn left_is_member_of(left: Node, src: &str, obj: &str) -> bool {
    match left.kind() {
        "member_expression" | "subscript_expression" => {
            let Some(object) = left.child_by_field_name("object") else {
                return false;
            };
            compact_node_text(object, src) == obj
        }
        _ => false,
    }
}

/// `Object.assign(OBJ, …)` / `Object.defineProperty(OBJ, …)` /
/// `Object.defineProperties(OBJ, …)` / `Object.setPrototypeOf(OBJ, …)`.
fn is_object_method_call_on(call: Node, src: &str, obj: &str) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(receiver) = callee.child_by_field_name("object") else {
        return false;
    };
    if node_text(receiver, src) != "Object" {
        return false;
    }
    let Some(method) = callee.child_by_field_name("property") else {
        return false;
    };
    if !matches!(
        node_text(method, src).as_str(),
        "assign" | "defineProperty" | "defineProperties" | "setPrototypeOf"
    ) {
        return false;
    }
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let arg_list: Vec<Node> = raw_comparable_children(args)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let Some(first) = arg_list.first() else {
        return false;
    };
    compact_node_text(*first, src) == obj
}

/// Returns `true` for `if (OBJ)` / `if (OBJ != null)` / `if (OBJ !== null)` /
/// `if (OBJ != undefined)` / `if (OBJ !== undefined)`. Negated forms (e.g.
/// `if (!OBJ)`) are NOT guards — they are exclusion checks.
fn condition_guards_object(if_stmt: Node, src: &str, obj: &str) -> bool {
    let Some(condition) = if_stmt.child_by_field_name("condition") else {
        return false;
    };
    let condition = unwrap_parens(condition);
    match condition.kind() {
        "identifier" | "member_expression" => compact_node_text(condition, src) == obj,
        "binary_expression" => binary_guards_object(condition, src, obj),
        _ => false,
    }
}

fn binary_guards_object(binary: Node, src: &str, obj: &str) -> bool {
    let Some(left) = binary.child_by_field_name("left") else {
        return false;
    };
    let Some(right) = binary.child_by_field_name("right") else {
        return false;
    };
    let Some(op) = binary.child_by_field_name("operator") else {
        return false;
    };
    let op = node_text(op, src);
    if !matches!(op.as_str(), "!=" | "!==") {
        return false;
    }
    let l = compact_node_text(left, src);
    let r = compact_node_text(right, src);
    let nullish = |t: &str| t == "null" || t == "undefined";
    (l == obj && nullish(&r)) || (r == obj && nullish(&l))
}

fn is_reassigned(node: Node, src: &str, obj: &str) -> bool {
    if node_reassigns_target(node, src, obj) {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .any(|child| is_reassigned(child, src, obj))
}

fn node_reassigns_target(node: Node, src: &str, obj: &str) -> bool {
    let target = match node.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            node.child_by_field_name("left")
        }
        "update_expression" => node
            .child_by_field_name("argument")
            .or_else(|| first_named_child(node)),
        _ => None,
    };
    target.is_some_and(|t| compact_node_text(t, src) == obj)
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
