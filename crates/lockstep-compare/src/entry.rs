//! Public entry point for `compare()` and the parse-failure plumbing it
//! depends on. Kept in its own module so `walk.rs` stays focused on the
//! walker mechanics.

use std::path::Path;

use lockstep_core::{Category, Finding};
use tree_sitter::{Parser, Tree};

use crate::compare_options::CompareOptions;
use crate::pure_narrowing_helper::register_narrowing_helper_declarations;
use crate::walk::{walk, WalkCtx};

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

    let mut ctx = WalkCtx::from_opts(base_src, head_src, opts);
    register_narrowing_helper_declarations(&mut ctx, head_tree.root_node());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::opts_report_all;

    #[test]
    fn compare_returns_empty_for_identical_sources() {
        let src = "function f(x) { return x + 1; }";
        let findings = compare(src, src, &opts_report_all());
        assert!(findings.is_empty(), "got: {:?}", findings);
    }

    #[test]
    fn compare_flags_divergent_sources() {
        let base = "function f(x) { return x + 1; }";
        let head = "function f(x) { return x - 1; }";
        let findings = compare(base, head, &opts_report_all());
        assert!(!findings.is_empty(), "expected divergence");
    }

    #[test]
    fn compare_walks_through_to_walker() {
        let base = "const x = 1;";
        let head = "const x = 2;";
        let findings = compare(base, head, &opts_report_all());
        assert!(!findings.is_empty(), "expected token divergence");
    }
}
