use lockstep_core::{Category, Finding, Severity};
use tree_sitter::Node;

use crate::report::snippet;
use crate::walk::{Side, WalkCtx};

pub(super) fn unmatched_child(
    ctx: &WalkCtx,
    base_parent: Node,
    head_parent: Node,
    child: Node,
    side: Side,
) -> Finding {
    let (category, message) = match side {
        Side::Base => (
            unmatched_category(child),
            format!(
                "`{}` exists only on base at line {} while comparing `{}`",
                child.kind(),
                line_of(child),
                base_parent.kind()
            ),
        ),
        Side::Head => (
            unmatched_category(child),
            format!(
                "`{}` exists only on head at line {} while comparing `{}`",
                child.kind(),
                line_of(child),
                head_parent.kind()
            ),
        ),
    };
    let (base_kind, head_kind) = match side {
        Side::Base => (child.kind(), "<missing>"),
        Side::Head => ("<missing>", child.kind()),
    };
    let (base_line, head_line) = match side {
        Side::Base => (line_of(child), line_of(head_parent)),
        Side::Head => (line_of(base_parent), line_of(child)),
    };
    Finding::new(ctx.path, category, message)
        .with_severity(Severity::Error)
        .with_kinds(base_kind, head_kind)
        .with_lines(base_line, head_line)
        .with_snippets(
            snippet(ctx.base_src, base_line),
            snippet(ctx.head_src, head_line),
        )
}

pub(super) fn kind_mismatch(ctx: &WalkCtx, base: Node, head: Node) -> Finding {
    let msg = format!(
        "different node kinds at base:{} head:{}: base=`{}` head=`{}`",
        line_of(base),
        line_of(head),
        base.kind(),
        head.kind(),
    );
    make_finding(ctx, base, head, Category::KindMismatch, msg)
}

pub(super) fn arity_mismatch(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    base_n: usize,
    head_n: usize,
) -> Finding {
    let msg = format!(
        "`{}` has {} named children on base, {} on head (base:{} head:{})",
        base.kind(),
        base_n,
        head_n,
        line_of(base),
        line_of(head),
    );
    make_finding(ctx, base, head, Category::ArityMismatch, msg)
}

pub(super) fn less_defensive_optional_chain(ctx: &WalkCtx, base: Node, head: Node) -> Finding {
    let msg = format!(
        "head removed `?.` optional chaining on `{}` (base:{} head:{}) — head is less defensive than base",
        base.kind(),
        line_of(base),
        line_of(head),
    );
    make_finding(ctx, base, head, Category::ArityMismatch, msg)
}

pub(super) fn token_mismatch(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    base_text: &str,
    head_text: &str,
) -> Finding {
    let msg = format!(
        "`{}` token differs: base=`{}` head=`{}` (base:{} head:{})",
        base.kind(),
        base_text,
        head_text,
        line_of(base),
        line_of(head),
    );
    make_finding(ctx, base, head, Category::TokenMismatch, msg)
}

pub(super) fn make_finding(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    category: Category,
    message: String,
) -> Finding {
    let bl = line_of(base);
    let hl = line_of(head);
    Finding::new(ctx.path, category, message)
        .with_severity(Severity::Error)
        .with_kinds(base.kind(), head.kind())
        .with_lines(bl, hl)
        .with_snippets(snippet(ctx.base_src, bl), snippet(ctx.head_src, hl))
}

pub(super) fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

fn unmatched_category(node: Node) -> Category {
    if is_statement_like(node.kind()) {
        Category::DroppedStatement
    } else {
        Category::ArityMismatch
    }
}

fn is_statement_like(kind: &str) -> bool {
    kind.ends_with("_statement")
        || kind.ends_with("_declaration")
        || matches!(
            kind,
            "lexical_declaration"
                | "variable_declaration"
                | "method_definition"
                | "field_definition"
        )
}
