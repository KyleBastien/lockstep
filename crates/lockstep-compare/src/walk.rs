//! Dual-walk AST comparator.
//!
//! Both inputs are parsed with `tree-sitter-javascript`. Walks named children
//! in lockstep, comparing `kind()`, child arity, and (at leaves) canonical
//! token text. Skips `comment` nodes and other JS trivia.

use std::path::{Path, PathBuf};

use lockstep_core::{Category, Finding, Severity};
use tree_sitter::{Node, Parser, Tree};

use crate::report::snippet;
use crate::tokens::canonical;

pub struct CompareOptions {
    pub path: PathBuf,
    /// If false, stop walking after the first divergence in a file.
    pub report_all: bool,
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

struct WalkCtx<'a> {
    base_src: &'a str,
    head_src: &'a str,
    path: &'a Path,
    report_all: bool,
}

fn walk(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    if !ctx.report_all && !findings.is_empty() {
        return;
    }
    if base.kind() != head.kind() {
        findings.push(kind_mismatch(ctx, base, head));
        return;
    }
    if is_atomic(base.kind()) {
        compare_leaf(ctx, base, head, findings);
        return;
    }

    let base_children = comparable_children(base);
    let head_children = comparable_children(head);
    if base_children.len() != head_children.len() {
        findings.push(arity_mismatch(
            ctx,
            base,
            head,
            base_children.len(),
            head_children.len(),
        ));
        return;
    }
    if base_children.is_empty() {
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
    for (b, h) in base_children.into_iter().zip(head_children.into_iter()) {
        walk(ctx, b, h, findings);
        if !ctx.report_all && !findings.is_empty() {
            return;
        }
    }
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
fn comparable_children(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_trivia(child) {
            continue;
        }
        if child.is_named() || is_meaningful_unnamed(child.kind()) {
            out.push(child);
        }
    }
    out
}

fn is_trivia(node: Node) -> bool {
    matches!(node.kind(), "comment" | "hash_bang_line")
}

fn is_atomic(kind: &str) -> bool {
    matches!(
        kind,
        "string" | "template_string" | "regex" | "number" | "identifier" | "property_identifier"
    )
}

fn is_meaningful_unnamed(kind: &str) -> bool {
    !matches!(
        kind,
        ";" | ","
            | "("
            | ")"
            | "["
            | "]"
            | "{"
            | "}"
            | "."
            | "..."
            | "?"
            | ":"
            | "=>"
            | "let"
            | "const"
            | "var"
    )
}

fn kind_mismatch(ctx: &WalkCtx, base: Node, head: Node) -> Finding {
    let msg = format!(
        "different node kinds at base:{} head:{}: base=`{}` head=`{}`",
        line_of(base),
        line_of(head),
        base.kind(),
        head.kind(),
    );
    make_finding(ctx, base, head, Category::KindMismatch, msg)
}

fn arity_mismatch(ctx: &WalkCtx, base: Node, head: Node, base_n: usize, head_n: usize) -> Finding {
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

fn token_mismatch(
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

fn make_finding(
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

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn opts() -> CompareOptions {
        CompareOptions {
            path: PathBuf::from("test.ts"),
            report_all: false,
        }
    }

    #[test]
    fn identical_sources_have_no_findings() {
        let src = "function f(x) { return x + 1; }";
        let f = compare(src, src, &opts());
        assert!(f.is_empty(), "got: {:?}", f);
    }

    #[test]
    fn renamed_identifier_flags_token_mismatch() {
        let f = compare("let x = 1;", "let y = 1;", &opts());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].category, Category::TokenMismatch);
    }

    #[test]
    fn extra_statement_flags_arity_mismatch() {
        let f = compare("let x = 1;", "let x = 1; let y = 2;", &opts());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].category, Category::ArityMismatch);
    }

    #[test]
    fn changed_node_kind_flags_kind_mismatch() {
        let f = compare("let x = 1;", "let x = foo();", &opts());
        assert_eq!(f.len(), 1);
        assert!(matches!(
            f[0].category,
            Category::KindMismatch | Category::ArityMismatch
        ));
    }

    #[test]
    fn comments_are_ignored() {
        let f = compare("let x = 1; // a", "let x = 1; /* b */", &opts());
        assert!(f.is_empty(), "got: {:?}", f);
    }

    #[test]
    fn quote_style_does_not_flag() {
        let f = compare("let s = 'foo';", "let s = \"foo\";", &opts());
        assert!(f.is_empty(), "got: {:?}", f);
    }

    #[test]
    fn plus_vs_minus_operator_flags_divergence() {
        let f = compare(
            "function add(a, b) { return a + b; }",
            "function add(a, b) { return a - b; }",
            &opts(),
        );
        assert!(!f.is_empty(), "expected divergence");
        assert!(matches!(
            f[0].category,
            Category::KindMismatch | Category::TokenMismatch
        ));
    }

    #[test]
    fn changed_literal_value_flags_divergence() {
        let f = compare("let x = 1;", "let x = 2;", &opts());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].category, Category::TokenMismatch);
    }

    #[test]
    fn report_all_returns_multiple_findings() {
        let opts_all = CompareOptions {
            path: PathBuf::from("x.ts"),
            report_all: true,
        };
        let f = compare("let x = 1; let y = 2;", "let a = 1; let b = 2;", &opts_all);
        assert_eq!(f.len(), 2);
    }
}
