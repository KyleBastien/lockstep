//! Dual-walk AST comparator.
//!
//! Both inputs are parsed with `tree-sitter-javascript`. Walks named children
//! in lockstep, comparing `kind()`, child arity, and (at leaves) canonical
//! token text. Skips `comment` nodes and other JS trivia.

use std::path::{Path, PathBuf};

use lockstep_core::{Category, Finding};
use tree_sitter::{Node, Parser, Tree};

use crate::align::align_children;
use crate::array_first_equivalence::is_array_first_pair;
use crate::async_propagation::{maybe_unwrap_await, try_callable_async_propagation};
use crate::class_equivalence::{is_cache_alias_pair, walk_class_body};
use crate::defensive_null_guard::try_defensive_null_guard;
use crate::findings::{arity_mismatch, kind_mismatch, token_mismatch, unmatched_child};
use crate::node_utils::{is_meaningful_unnamed, is_trivia, raw_comparable_children};
use crate::nullish_widening_equivalence::is_nullish_widening_pair;
use crate::optional_chain::handle_optional_chain;
use crate::request_field_narrowing::{is_narrowed_request_field_pair, try_request_field_narrowing};
use crate::tokens::canonical;
use crate::transient_cache_wrap::{is_transient_local_pair, try_transient_cache_wrap};

pub struct CompareOptions {
    pub path: PathBuf,
    /// If false, stop walking after the first divergence in a file.
    pub report_all: bool,
    /// Treat constructor assignments like `this.foo = function () {}` as
    /// equivalent to class methods named `foo` when their callable bodies match.
    pub allow_constructor_assigned_method_equivalence: bool,
    /// Treat matching constructor-local caches and instance fields as aliases.
    pub allow_closure_cache_field_alias: bool,
    /// Treat condition-guarded "first element or null" ternaries as equivalent
    /// to `EXPR[0] ?? null` (see `array_first_equivalence`).
    pub allow_array_first_element_or_null: bool,
    /// Additionally accept `EXPR[0] || null` and bare `EXPR[0]` as equivalent
    /// base shapes for `EXPR[0] ?? null` on head.
    pub allow_array_first_element_or_null_loose: bool,
    /// Treat `EXPR` ↔ `EXPR ?? null` (or `EXPR ?? undefined`) as equivalent at
    /// any AST position. Directional: head must be the widener.
    pub allow_nullish_widening: bool,
    /// Sub-flag of [`Self::allow_nullish_widening`]: additionally accept a bare
    /// `null` ↔ `undefined` literal swap at any position. Off by default even
    /// when widening is on, because `=== null` / `=== undefined` are
    /// observationally distinct.
    pub allow_null_undefined_swap: bool,
    /// Treat a sync head method whose body is `return (async () => BODY)();`
    /// as equivalent to a base async callable whose body is BODY. Lets TS
    /// migrations carry phantom-branded return types that block bare `async`.
    pub allow_iife_async_wrapper: bool,
    /// Accept the two-statement base pattern `CACHE = X; CACHE = unwrap(CACHE);`
    /// as equivalent to head `const LOCAL = X; CACHE = unwrap(LOCAL);` when
    /// LOCAL is a fresh local used only inside the unwrap. Composes with
    /// `allow_closure_cache_field_alias`.
    pub allow_transient_cache_wrap: bool,
    /// Accept a head `const IDENT = "PROP" in OBJ && typeof OBJ.PROP === T ? OBJ.PROP : undefined;`
    /// extraction, treating later `IDENT` uses as equivalent to base
    /// `OBJ.PROP` accesses in the same scope.
    pub allow_request_field_narrowing: bool,
    /// Accept head async + `await EXPR` where base is sync + bare `EXPR`,
    /// provided at least one new `await` appears on head. Directional: never
    /// the reverse.
    pub allow_async_propagation: bool,
    /// Accept a head-inserted `if (!CACHE) { logCall(...); return LIT; }`
    /// guard between two base statements. Observably changes behavior —
    /// stays off by default.
    pub allow_defensive_null_guard: bool,
}

pub fn compare(base_src: &str, head_src: &str, opts: &CompareOptions) -> Vec<Finding> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_javascript::language())
        .is_err()
    {
        return vec![Finding::new(
            &opts.path,
            Category::ParseError,
            "failed to load javascript grammar",
        )];
    }
    let base_tree = match parse(&mut parser, base_src) {
        Some(t) => t,
        None => return vec![parse_error(&opts.path, true)],
    };
    let head_tree = match parse(&mut parser, head_src) {
        Some(t) => t,
        None => return vec![parse_error(&opts.path, false)],
    };

    let ctx = WalkCtx::from_opts(base_src, head_src, opts);
    let mut findings = Vec::new();
    walk(
        &ctx,
        base_tree.root_node(),
        head_tree.root_node(),
        &mut findings,
    );
    findings
}

fn parse(parser: &mut Parser, src: &str) -> Option<Tree> {
    parser.parse(src, None)
}

fn parse_error(path: &Path, base_side: bool) -> Finding {
    let which = if base_side {
        "base (post-normalize)"
    } else {
        "head (post-strip+normalize)"
    };
    Finding::new(
        path,
        Category::ParseError,
        format!("failed to parse {which} as JavaScript"),
    )
}

pub(super) use crate::walk_ctx::{CacheAlias, NarrowedRequestField, Side, TransientLocal, WalkCtx};

pub(super) fn walk(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    if !ctx.report_all && !findings.is_empty() {
        return;
    }
    if leaf_alias_consumed(ctx, base, head) {
        return;
    }
    if block_rule_consumed(ctx, base, head, findings) {
        return;
    }
    if base.kind() != head.kind() {
        findings.push(kind_mismatch(ctx, base, head));
        return;
    }
    if ctx.allow_constructor_assigned_method_equivalence
        && base.kind() == "class_body"
        && walk_class_body(ctx, base, head, findings)
    {
        return;
    }
    walk_regular(ctx, base, head, findings);
}

/// Leaf-level pre-empts that resolve a pair via a registered alias or a
/// purely structural equivalence — no body walk required.
fn leaf_alias_consumed(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    is_cache_alias_pair(ctx, base, head)
        || is_transient_local_pair(ctx, base, head)
        || is_narrowed_request_field_pair(ctx, base, head)
        || is_array_first_pair(ctx, base, head)
        || is_nullish_widening_pair(ctx, base, head)
}

/// Block- and callable-scoped pre-empts that may consume the pair by running
/// their own sub-walk and feeding findings back to the outer pass.
fn block_rule_consumed(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) -> bool {
    try_transient_cache_wrap(ctx, base, head, findings)
        || try_request_field_narrowing(ctx, base, head, findings)
        || try_defensive_null_guard(ctx, base, head, findings)
        || maybe_unwrap_await(ctx, base, head, findings)
        || (is_method_definition_pair(base, head)
            && try_callable_async_propagation(ctx, base, head, findings))
}

pub(super) fn walk_regular(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    if is_atomic(base.kind()) {
        compare_leaf(ctx, base, head, findings);
        return;
    }
    if handle_optional_chain(ctx, base, head, findings) {
        return;
    }
    let base_children = comparable_children(ctx, base, Side::Base);
    let head_children = comparable_children(ctx, head, Side::Head);
    walk_collected(
        ctx,
        NodePair { base, head },
        ChildPair {
            base: base_children,
            head: head_children,
        },
        findings,
    );
}

pub(super) fn walk_optional_chain_more_defensive(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) {
    let base_children = comparable_children(ctx, base, Side::Base);
    let head_children: Vec<Node> = comparable_children(ctx, head, Side::Head)
        .into_iter()
        .filter(|n| n.kind() != "optional_chain")
        .collect();
    walk_collected(
        ctx,
        NodePair { base, head },
        ChildPair {
            base: base_children,
            head: head_children,
        },
        findings,
    );
}

pub(super) struct NodePair<'a> {
    pub(super) base: Node<'a>,
    pub(super) head: Node<'a>,
}

struct ChildPair<'a> {
    base: Vec<Node<'a>>,
    head: Vec<Node<'a>>,
}

fn walk_collected(
    ctx: &WalkCtx,
    nodes: NodePair,
    children: ChildPair,
    findings: &mut Vec<Finding>,
) {
    let NodePair { base, head } = nodes;
    let ChildPair {
        base: base_children,
        head: head_children,
    } = children;
    if base_children.len() != head_children.len() {
        if !walk_aligned_arity_mismatch(
            ctx,
            (base, head),
            (&base_children, &head_children),
            findings,
        ) {
            findings.push(arity_mismatch(
                ctx,
                base,
                head,
                base_children.len(),
                head_children.len(),
            ));
        }
        return;
    }
    if base_children.is_empty() {
        if has_filtered_children(ctx, base, Side::Base)
            || has_filtered_children(ctx, head, Side::Head)
        {
            return;
        }
        compare_leaf(ctx, base, head, findings);
        return;
    }
    walk_children(ctx, base_children, head_children, findings);
}

fn walk_children(
    ctx: &WalkCtx,
    base_children: Vec<Node>,
    head_children: Vec<Node>,
    findings: &mut Vec<Finding>,
) {
    for (b, h) in base_children.into_iter().zip(head_children) {
        walk(ctx, b, h, findings);
        if !ctx.report_all && !findings.is_empty() {
            return;
        }
    }
}

fn walk_aligned_arity_mismatch(
    ctx: &WalkCtx,
    parents: (Node, Node),
    children: (&[Node], &[Node]),
    findings: &mut Vec<Finding>,
) -> bool {
    let (base, head) = parents;
    let (base_children, head_children) = children;
    let alignment = align_children(ctx, base_children, head_children);
    if alignment.pairs.is_empty() {
        return false;
    }
    for idx in alignment.unmatched_base {
        findings.push(unmatched_child(
            ctx,
            base,
            head,
            base_children[idx],
            Side::Base,
        ));
        if !ctx.report_all {
            return true;
        }
    }
    for idx in alignment.unmatched_head {
        findings.push(unmatched_child(
            ctx,
            base,
            head,
            head_children[idx],
            Side::Head,
        ));
        if !ctx.report_all {
            return true;
        }
    }
    for (base_idx, head_idx) in alignment.pairs {
        walk(
            ctx,
            base_children[base_idx],
            head_children[head_idx],
            findings,
        );
        if !ctx.report_all && !findings.is_empty() {
            return true;
        }
    }
    true
}

fn compare_leaf(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    let base_text = base.utf8_text(ctx.base_src.as_bytes()).unwrap_or("");
    let head_text = head.utf8_text(ctx.head_src.as_bytes()).unwrap_or("");
    if canonical(base.kind(), base_text) != canonical(head.kind(), head_text) {
        findings.push(token_mismatch(ctx, base, head, base_text, head_text));
    }
}

/// Children that participate in the structural compare.
///
/// Includes every named child that isn't trivia, plus unnamed children whose
/// kind is *meaningful* — operators, keyword operators (`typeof`, `instanceof`,
/// `in`, `of`, `void`, `delete`). Excludes pure punctuation and the declaration
/// keywords (`let`/`const`/`var`) so `var` → `const`/`let` normalization is
/// silently accepted.
fn comparable_children<'a>(ctx: &WalkCtx, node: Node<'a>, side: Side) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_trivia(child) {
            continue;
        }
        if is_ignored(ctx, child, side) {
            continue;
        }
        if child.is_named() || is_meaningful_unnamed(child.kind()) {
            out.push(child);
        }
    }
    out
}

fn is_ignored(ctx: &WalkCtx, node: Node, side: Side) -> bool {
    let starts = match side {
        Side::Base => &ctx.ignored_base_starts,
        Side::Head => &ctx.ignored_head_starts,
    };
    starts.contains(&node.start_byte())
}

fn has_filtered_children(ctx: &WalkCtx, node: Node, side: Side) -> bool {
    raw_comparable_children(node)
        .into_iter()
        .any(|child| is_ignored(ctx, child, side))
}

fn is_method_definition_pair(base: Node, head: Node) -> bool {
    base.kind() == "method_definition" && head.kind() == "method_definition"
}

fn is_atomic(kind: &str) -> bool {
    matches!(
        kind,
        "string" | "template_string" | "regex" | "number" | "identifier" | "property_identifier"
    )
}
