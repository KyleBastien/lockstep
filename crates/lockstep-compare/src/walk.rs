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
use crate::class_equivalence::{is_cache_alias_pair, walk_class_body};
use crate::findings::{arity_mismatch, kind_mismatch, token_mismatch, unmatched_child};
use crate::node_utils::{is_meaningful_unnamed, is_trivia, raw_comparable_children};
use crate::nullish_widening_equivalence::is_nullish_widening_pair;
use crate::optional_chain::handle_optional_chain;
use crate::tokens::canonical;

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

    let ctx = WalkCtx {
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
        ignored_base_starts: Vec::new(),
        ignored_head_starts: Vec::new(),
        aliases: Vec::new(),
    };
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

#[derive(Clone)]
pub(super) struct WalkCtx<'a> {
    pub(super) base_src: &'a str,
    pub(super) head_src: &'a str,
    pub(super) path: &'a Path,
    pub(super) report_all: bool,
    allow_constructor_assigned_method_equivalence: bool,
    pub(super) allow_closure_cache_field_alias: bool,
    pub(super) allow_array_first_element_or_null: bool,
    pub(super) allow_array_first_element_or_null_loose: bool,
    pub(super) allow_nullish_widening: bool,
    pub(super) allow_null_undefined_swap: bool,
    pub(super) ignored_base_starts: Vec<usize>,
    pub(super) ignored_head_starts: Vec<usize>,
    pub(super) aliases: Vec<CacheAlias>,
}

impl<'a> WalkCtx<'a> {
    /// Clone with accumulator state cleared. Config flags and sources preserved.
    /// For sub-comparisons whose findings should not feed back into the outer pass.
    pub(super) fn scratch(&self) -> Self {
        let mut s = self.clone();
        s.ignored_base_starts.clear();
        s.ignored_head_starts.clear();
        s.aliases.clear();
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CacheAlias {
    pub(super) base_name: String,
    pub(super) head_property: String,
}

#[derive(Clone, Copy)]
pub(super) enum Side {
    Base,
    Head,
}

pub(super) fn walk(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    if !ctx.report_all && !findings.is_empty() {
        return;
    }
    if is_cache_alias_pair(ctx, base, head) {
        return;
    }
    if is_array_first_pair(ctx, base, head) {
        return;
    }
    if is_nullish_widening_pair(ctx, base, head) {
        return;
    }
    if base.kind() != head.kind() {
        findings.push(kind_mismatch(ctx, base, head));
        return;
    }
    if ctx.allow_constructor_assigned_method_equivalence
        && base.kind() == "class_body"
        && head.kind() == "class_body"
        && walk_class_body(ctx, base, head, findings)
    {
        return;
    }
    walk_regular(ctx, base, head, findings);
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

fn is_atomic(kind: &str) -> bool {
    matches!(
        kind,
        "string" | "template_string" | "regex" | "number" | "identifier" | "property_identifier"
    )
}
