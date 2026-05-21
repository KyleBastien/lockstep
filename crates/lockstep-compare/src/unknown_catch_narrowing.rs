//! Equivalence rule for strict-TS `unknown` catch binding narrowing.
//!
//! Gated on `allow_unknown_catch_narrowing`. TypeScript's strict mode (and
//! `useUnknownInCatchVariables`) types the catch binding as `unknown`, so any
//! access like `err.message` requires a runtime narrowing. The canonical
//! migration shape is one of:
//!
//! ```text
//! // A. Inline ternary at the use site:
//! catch (err) { console.log(err instanceof Error ? err.message : String(err)); }
//!
//! // B. Const extraction at the top of the catch block:
//! catch (err) {
//!     const message = err instanceof Error ? err.message : String(err);
//!     console.log(message);
//! }
//! ```
//!
//! Each is accepted as equivalent to the base shape that reads `err.message`
//! directly. The alternative branch of the ternary may take any of the
//! shapes documented in [`crate::unknown_catch_fallbacks`] — all are
//! runtime-equivalent stringifications of the caught value for the
//! JS-typical Error throw case.
//!
//! Precondition: `ERR` must be bound by an enclosing `catch_clause` parameter.
//! This is the TS-forced gate — without it the rule risks accepting
//! arbitrary `EXPR.PROP` ↔ ternary substitutions. Destructured catch
//! parameters are out of scope (rare in practice).
//!
//! **Observable divergence (small):** when the caught value is not an
//! `Error` instance (e.g. `throw "x"`, `throw {plain:1}`), base reads
//! `undefined`/the raw value while head produces a stringified form. Both
//! are JS antipatterns; the rule defaults OFF and the divergence stays at
//! the stringification boundary, never affecting control flow.

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::unknown_catch_fallbacks::alternative_is_accepted;
use crate::walk::{CatchNarrowedLocal, WalkCtx};

/// Leaf-pair-style pre-empt for the **inline ternary** form.
///
/// Returns `true` when:
/// - base is a `member_expression` `ERR.PROP`
/// - head is a `ternary_expression` `ERR instanceof Error ? ERR.PROP : <fallback>`
/// - `ERR` resolves to an enclosing `catch_clause` parameter on the head side
/// - The base member text and the head consequence member text match.
pub(super) fn is_unknown_catch_narrowing_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if !ctx.allow_unknown_catch_narrowing {
        return false;
    }
    if base.kind() != "member_expression" || head.kind() != "ternary_expression" {
        return false;
    }
    let Some(parts) = extract_ternary_parts(head, ctx.head_src) else {
        return false;
    };
    if !head_is_inside_catch_binding(head, ctx.head_src, &parts.err_name) {
        return false;
    }
    let base_text = compact_node_text(base, ctx.base_src);
    let expected = format!("{}.{}", parts.err_name, parts.property);
    base_text == expected
}

/// Leaf-pair resolver for the **const extraction** form. Returns `true` when
/// head identifier matches a registered `CatchNarrowedLocal` and base is the
/// equivalent `ERR.PROP` member expression.
pub(super) fn is_catch_narrowed_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if head.kind() != "identifier" {
        return false;
    }
    let head_text = node_text(head, ctx.head_src);
    let Some(alias) = ctx
        .catch_narrowed_locals
        .iter()
        .find(|a| a.head_local == head_text)
    else {
        return false;
    };
    if base.kind() != "member_expression" {
        return false;
    }
    let Some(object) = base.child_by_field_name("object") else {
        return false;
    };
    let Some(property) = base.child_by_field_name("property") else {
        return false;
    };
    compact_node_text(object, ctx.base_src) == alias.err_name
        && node_text(property, ctx.base_src) == alias.property
}

/// Composable block-strip for the **const extraction** form. Detects
/// `const LOCAL = ERR instanceof Error ? ERR.PROP : <fallback>;` at the head
/// block top, validates `ERR` is the enclosing catch binding, and registers a
/// scope-local alias. Returns `true` when at least one alias was registered.
pub(super) fn apply_unknown_catch_narrowing(
    child_ctx: &mut WalkCtx,
    base: Node,
    head: Node,
) -> bool {
    if !child_ctx.allow_unknown_catch_narrowing {
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
    let mut applied = false;
    for stmt in &head_stmts {
        let Some(extract) = extract_const_catch_narrowing(*stmt, head_src) else {
            continue;
        };
        if !head_is_inside_catch_binding(*stmt, head_src, &extract.err_name) {
            continue;
        }
        child_ctx.ignored_head_starts.push(stmt.start_byte());
        child_ctx.catch_narrowed_locals.push(CatchNarrowedLocal {
            head_local: extract.local_name,
            err_name: extract.err_name,
            property: extract.property,
        });
        applied = true;
    }
    applied
}

struct TernaryParts {
    err_name: String,
    property: String,
}

struct Extract {
    local_name: String,
    err_name: String,
    property: String,
}

fn extract_const_catch_narrowing(stmt: Node, src: &str) -> Option<Extract> {
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
    let decl = declarators[0];
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let local_name = node_text(name, src);
    let value = unwrap_parens(decl.child_by_field_name("value")?);
    if value.kind() != "ternary_expression" {
        return None;
    }
    let parts = extract_ternary_parts(value, src)?;
    Some(Extract {
        local_name,
        err_name: parts.err_name,
        property: parts.property,
    })
}

/// Verifies head ternary shape `ERR instanceof Error ? ERR.PROP : <fallback>`,
/// then returns the bound name and the accessed property.
fn extract_ternary_parts(ternary: Node, src: &str) -> Option<TernaryParts> {
    let condition = unwrap_parens(ternary.child_by_field_name("condition")?);
    let err_name = condition_instanceof_error(condition, src)?;

    let consequence = unwrap_parens(ternary.child_by_field_name("consequence")?);
    let (cons_obj, cons_prop) = consequence_member(consequence, src)?;
    if cons_obj != err_name {
        return None;
    }

    let alternative = unwrap_parens(ternary.child_by_field_name("alternative")?);
    if !alternative_is_accepted(alternative, src, &err_name, &cons_prop) {
        return None;
    }

    Some(TernaryParts {
        err_name,
        property: cons_prop,
    })
}

fn condition_instanceof_error(node: Node, src: &str) -> Option<String> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let op = node.child_by_field_name("operator")?;
    if node_text(op, src) != "instanceof" {
        return None;
    }
    let left = unwrap_parens(node.child_by_field_name("left")?);
    let right = unwrap_parens(node.child_by_field_name("right")?);
    if left.kind() != "identifier" {
        return None;
    }
    if right.kind() != "identifier" || node_text(right, src) != "Error" {
        return None;
    }
    Some(node_text(left, src))
}

fn consequence_member(node: Node, src: &str) -> Option<(String, String)> {
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
    Some((node_text(object, src), node_text(property, src)))
}

/// Climbs from `node` to a `catch_clause` whose parameter identifier matches
/// `err_name`. Returns `true` on hit, `false` otherwise (including when an
/// enclosing function boundary is crossed first — out of scope).
fn head_is_inside_catch_binding(node: Node, src: &str, err_name: &str) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "catch_clause" && catch_parameter_matches(parent, src, err_name) {
            return true;
        }
        if is_function_boundary(parent.kind()) {
            return false;
        }
        current = parent;
    }
    false
}

fn catch_parameter_matches(catch: Node, src: &str, err_name: &str) -> bool {
    let Some(param) = catch.child_by_field_name("parameter") else {
        return false;
    };
    if param.kind() != "identifier" {
        return false;
    }
    node_text(param, src) == err_name
}

fn is_function_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration"
            | "generator_function"
    )
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
