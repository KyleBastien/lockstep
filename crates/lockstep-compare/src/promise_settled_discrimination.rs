//! Equivalence rule for `Promise.allSettled` discriminated-union narrowing.
//!
//! Gated on `allow_promise_settled_discrimination`. `PromiseSettledResult<T>`
//! is `{ status: "fulfilled", value: T } | { status: "rejected", reason: any }`,
//! so any direct access on `.value` / `.reason` in strict TS requires a
//! status check first. The canonical migration shape is an early-return
//! guard inserted before each access:
//!
//! ```text
//! const [a, b] = await Promise.allSettled([P, Q]);
//! if (a.status !== "fulfilled") return EARLY;   // <-- head-only
//! log(a.value.x);
//! if (b.status !== "rejected") return EARLY;    // <-- head-only
//! return b.reason;
//! ```
//!
//! Base reads `a.value.x` / `b.reason` directly. The head-inserted guard is
//! accepted as equivalent under a **deadness witness**: base would have
//! thrown a `TypeError` reading `.value` on a rejected result (or `.reason`
//! on a fulfilled result), so the head's early return only triggers on a
//! path where base would also have aborted with an exception.
//!
//! **WARNING — observable behavior change at the error location.** Base
//! throws `TypeError`; head returns (or throws) from a different call site
//! with different state. Relevant to error monitoring; not to product
//! behavior on the happy path. The rule defaults OFF, matching the
//! precedent set by `allow_dead_defensive_optional_chain_removal`.
//!
//! V1 binding shapes recognized:
//! - `const NAME = await Promise.allSettled(...)`
//! - `const [N1, N2, ...] = await Promise.allSettled(...)` (named identifiers
//!   in the array pattern; rest elements and nested patterns out of scope).
//!
//! Refuses to fire when `NAME` is reassigned anywhere in the block.

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::walk::WalkCtx;

/// Composable block-strip. Scans `head` for `if (NAME.status OP "TAG") { TERM; }`
/// guards inserted after a `Promise.allSettled` binding for `NAME`, where a
/// later head access on the surviving branch witnesses the deadness of the
/// guarded branch in base. Returns `true` when at least one guard was hidden.
pub(super) fn apply_promise_settled_discrimination(
    child_ctx: &mut WalkCtx,
    base: Node,
    head: Node,
) -> bool {
    if !child_ctx.allow_promise_settled_discrimination {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let head_src = child_ctx.head_src;
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let settled_names = collect_settled_names(&head_stmts, head_src);
    if settled_names.is_empty() {
        return false;
    }
    let mut applied = false;
    for (i, stmt) in head_stmts.iter().enumerate() {
        let Some(guard) = match_settled_guard(*stmt, head_src) else {
            continue;
        };
        if !settled_names.contains(&guard.name) {
            continue;
        }
        if name_is_reassigned(head, head_src, &guard.name) {
            continue;
        }
        if !witness_after(&head_stmts[i + 1..], head_src, &guard.name, guard.survives) {
            continue;
        }
        child_ctx.ignored_head_starts.push(stmt.start_byte());
        applied = true;
    }
    applied
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurvivingStatus {
    Fulfilled,
    Rejected,
}

struct Guard {
    name: String,
    survives: SurvivingStatus,
}

/// Recognizes `if (NAME.status OP "TAG") { return X; }` (or `throw`).
/// `OP ∈ {===, !==}`, `TAG ∈ {"fulfilled", "rejected"}`. Returns the bound
/// `NAME` and the status that survives past the guard.
fn match_settled_guard(stmt: Node, src: &str) -> Option<Guard> {
    if stmt.kind() != "if_statement" {
        return None;
    }
    let (op, name, tag) = match_status_condition(stmt.child_by_field_name("condition")?, src)?;
    if !body_terminates(stmt.child_by_field_name("consequence")?) {
        return None;
    }
    let survives = surviving_status(op.as_str(), tag)?;
    Some(Guard { name, survives })
}

/// Parses `NAME.status === "TAG"` / `"TAG" !== NAME.status` shapes from the
/// `if` condition. Returns the operator, bound name, and tag text.
fn match_status_condition(condition: Node, src: &str) -> Option<(String, String, String)> {
    let condition = unwrap_parens(condition);
    if condition.kind() != "binary_expression" {
        return None;
    }
    let op = node_text(condition.child_by_field_name("operator")?, src);
    if !matches!(op.as_str(), "===" | "!==") {
        return None;
    }
    let left = unwrap_parens(condition.child_by_field_name("left")?);
    let right = unwrap_parens(condition.child_by_field_name("right")?);
    let (name, tag) = extract_status_check(left, right, src)?;
    Some((op, name, tag))
}

/// Accepts either `NAME.status === "TAG"` or `"TAG" === NAME.status` shape.
/// Returns the bound name and the tag text.
fn extract_status_check(left: Node, right: Node, src: &str) -> Option<(String, String)> {
    if let (Some(name), Some(tag)) = (status_member(left, src), string_literal_value(right, src)) {
        return Some((name, tag));
    }
    if let (Some(tag), Some(name)) = (string_literal_value(left, src), status_member(right, src)) {
        return Some((name, tag));
    }
    None
}

fn status_member(node: Node, src: &str) -> Option<String> {
    if node.kind() != "member_expression" {
        return None;
    }
    if node.child_by_field_name("optional_chain").is_some() {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if object.kind() != "identifier" {
        return None;
    }
    let property = node.child_by_field_name("property")?;
    if node_text(property, src) != "status" {
        return None;
    }
    Some(node_text(object, src))
}

fn string_literal_value(node: Node, src: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let text = node_text(node, src);
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if first != last || !matches!(first, b'\'' | b'"' | b'`') {
        return None;
    }
    Some(text[1..text.len() - 1].to_string())
}

fn surviving_status(op: &str, tag: String) -> Option<SurvivingStatus> {
    match (op, tag.as_str()) {
        ("!==", "fulfilled") | ("===", "rejected") => Some(SurvivingStatus::Fulfilled),
        ("!==", "rejected") | ("===", "fulfilled") => Some(SurvivingStatus::Rejected),
        _ => None,
    }
}

fn body_terminates(body: Node) -> bool {
    match body.kind() {
        "return_statement" | "throw_statement" => true,
        "statement_block" => raw_comparable_children(body)
            .into_iter()
            .filter(|n| n.is_named())
            .any(|n| matches!(n.kind(), "return_statement" | "throw_statement")),
        _ => false,
    }
}

/// Returns every identifier in `head_stmts` that is declared as
/// `const NAME = await Promise.allSettled(...)` or as an element of
/// `const [N1, N2, ...] = await Promise.allSettled(...)`.
fn collect_settled_names(stmts: &[Node], src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for stmt in stmts {
        extend_with_settled_names(&mut out, *stmt, src);
    }
    out
}

fn extend_with_settled_names(out: &mut Vec<String>, stmt: Node, src: &str) {
    let Some(decl) = sole_variable_declarator(stmt) else {
        return;
    };
    let Some(value) = decl.child_by_field_name("value") else {
        return;
    };
    if !value_is_promise_allsettled(unwrap_parens(value), src) {
        return;
    }
    let Some(name) = decl.child_by_field_name("name") else {
        return;
    };
    match name.kind() {
        "identifier" => out.push(node_text(name, src)),
        "array_pattern" => extend_with_pattern_idents(out, name, src),
        _ => {}
    }
}

fn extend_with_pattern_idents(out: &mut Vec<String>, pattern: Node, src: &str) {
    for child in raw_comparable_children(pattern) {
        if child.kind() == "identifier" {
            out.push(node_text(child, src));
        }
    }
}

fn sole_variable_declarator(stmt: Node) -> Option<Node> {
    if !matches!(stmt.kind(), "lexical_declaration" | "variable_declaration") {
        return None;
    }
    let declarators: Vec<Node> = raw_comparable_children(stmt)
        .into_iter()
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if declarators.len() != 1 {
        return None;
    }
    Some(declarators[0])
}

/// Returns `true` for `await Promise.allSettled(...)` (with or without
/// surrounding parens).
fn value_is_promise_allsettled(value: Node, src: &str) -> bool {
    if value.kind() != "await_expression" {
        return false;
    }
    let Some(arg) = await_argument(value) else {
        return false;
    };
    let call = unwrap_parens(arg);
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    compact_node_text(callee, src) == "Promise.allSettled"
}

fn await_argument(node: Node) -> Option<Node> {
    if let Some(arg) = node.child_by_field_name("argument") {
        return Some(arg);
    }
    first_named_child(node)
}

/// Returns `true` when at least one statement after the guard reads
/// `NAME.value.<X>` (when fulfilled survives) or `NAME.reason.<X>` (when
/// rejected survives). The access can be at any depth in a later statement.
fn witness_after(later_stmts: &[Node], src: &str, name: &str, survives: SurvivingStatus) -> bool {
    let expected_prop = match survives {
        SurvivingStatus::Fulfilled => "value",
        SurvivingStatus::Rejected => "reason",
    };
    later_stmts
        .iter()
        .any(|stmt| has_settled_access(*stmt, src, name, expected_prop))
}

fn has_settled_access(node: Node, src: &str, name: &str, prop: &str) -> bool {
    if is_settled_access(node, src, name, prop) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_settled_access(child, src, name, prop) {
            return true;
        }
    }
    false
}

/// Matches `NAME.PROP` member expressions exactly.
fn is_settled_access(node: Node, src: &str, name: &str, prop: &str) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(object) = node.child_by_field_name("object") else {
        return false;
    };
    if object.kind() != "identifier" || node_text(object, src) != name {
        return false;
    }
    let Some(property) = node.child_by_field_name("property") else {
        return false;
    };
    node_text(property, src) == prop
}

fn name_is_reassigned(root: Node, src: &str, name: &str) -> bool {
    if reassigns_target(root, src, name) {
        return true;
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if name_is_reassigned(child, src, name) {
            return true;
        }
    }
    false
}

fn reassigns_target(node: Node, src: &str, name: &str) -> bool {
    let target = match node.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            node.child_by_field_name("left")
        }
        "update_expression" => node
            .child_by_field_name("argument")
            .or_else(|| first_named_child(node)),
        _ => return false,
    };
    target.is_some_and(|t| t.kind() == "identifier" && node_text(t, src) == name)
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
