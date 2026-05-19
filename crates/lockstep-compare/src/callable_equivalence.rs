use lockstep_core::{Category, Finding};
use tree_sitter::Node;

use crate::findings::{line_of, make_finding};
use crate::node_utils::{find_direct_child, node_text, raw_comparable_children, statement_block};
use crate::walk::{walk, WalkCtx};

pub(super) fn compare_assigned_function_to_method(
    ctx: &WalkCtx,
    base_function: Node,
    head_method: Node,
    findings: &mut Vec<Finding>,
) {
    if async_flag(base_function, ctx.base_src) != async_flag(head_method, ctx.head_src) {
        findings.push(callable_flag_mismatch(
            ctx,
            base_function,
            head_method,
            "async",
        ));
        return;
    }
    if generator_flag(base_function, ctx.base_src) != generator_flag(head_method, ctx.head_src) {
        findings.push(callable_flag_mismatch(
            ctx,
            base_function,
            head_method,
            "generator",
        ));
        return;
    }

    compare_callable_parameters(ctx, base_function, head_method, findings);
    if !ctx.report_all && !findings.is_empty() {
        return;
    }
    compare_callable_bodies(ctx, base_function, head_method, findings);
}

fn compare_callable_parameters(
    ctx: &WalkCtx,
    base_function: Node,
    head_method: Node,
    findings: &mut Vec<Finding>,
) {
    let base_params = parameter_node(base_function);
    let head_params = parameter_node(head_method);
    match (base_params, head_params) {
        (Some(base), Some(head)) => walk(ctx, base, head, findings),
        (None, Some(head)) if base_function.kind() == "arrow_function" => {
            if let (Some(base_param), Some(head_param)) = (
                single_arrow_parameter(base_function),
                single_formal_parameter(head),
            ) {
                walk(ctx, base_param, head_param, findings);
            } else {
                findings.push(callable_shape_mismatch(
                    ctx,
                    base_function,
                    head_method,
                    "parameters",
                ));
            }
        }
        _ => findings.push(callable_shape_mismatch(
            ctx,
            base_function,
            head_method,
            "parameters",
        )),
    }
}

fn compare_callable_bodies(
    ctx: &WalkCtx,
    base_function: Node,
    head_method: Node,
    findings: &mut Vec<Finding>,
) {
    let Some(base_body) = callable_body(base_function) else {
        findings.push(callable_shape_mismatch(
            ctx,
            base_function,
            head_method,
            "body",
        ));
        return;
    };
    let Some(head_body) = statement_block(head_method) else {
        findings.push(callable_shape_mismatch(
            ctx,
            base_function,
            head_method,
            "body",
        ));
        return;
    };
    if base_body.kind() == "statement_block" {
        walk(ctx, base_body, head_body, findings);
        return;
    }
    if let Some(return_value) = single_return_value(head_body) {
        walk(ctx, base_body, return_value, findings);
    } else {
        findings.push(callable_shape_mismatch(
            ctx,
            base_function,
            head_method,
            "body",
        ));
    }
}

fn callable_flag_mismatch(ctx: &WalkCtx, base: Node, head: Node, flag: &str) -> Finding {
    make_finding(
        ctx,
        base,
        head,
        Category::KindMismatch,
        format!(
            "constructor-assigned function and method differ in `{flag}` flag (base:{} head:{})",
            line_of(base),
            line_of(head)
        ),
    )
}

fn callable_shape_mismatch(ctx: &WalkCtx, base: Node, head: Node, part: &str) -> Finding {
    make_finding(
        ctx,
        base,
        head,
        Category::ArityMismatch,
        format!(
            "constructor-assigned function and method have incompatible {part} (base:{} head:{})",
            line_of(base),
            line_of(head)
        ),
    )
}

fn parameter_node(node: Node) -> Option<Node> {
    find_direct_child(node, "formal_parameters")
}

fn single_arrow_parameter(node: Node) -> Option<Node> {
    if node.kind() != "arrow_function" {
        return None;
    }
    raw_comparable_children(node)
        .into_iter()
        .find(|child| child.kind() == "identifier")
}

fn single_formal_parameter(parameters: Node) -> Option<Node> {
    let children = raw_comparable_children(parameters)
        .into_iter()
        .filter(|child| child.is_named())
        .collect::<Vec<_>>();
    if children.len() == 1 {
        Some(children[0])
    } else {
        None
    }
}

fn callable_body(node: Node) -> Option<Node> {
    node.child_by_field_name("body").or_else(|| {
        raw_comparable_children(node)
            .into_iter()
            .rev()
            .find(|child| child.kind() != "formal_parameters")
    })
}

fn single_return_value(statement_block: Node) -> Option<Node> {
    let statements = raw_comparable_children(statement_block)
        .into_iter()
        .filter(|child| child.is_named())
        .collect::<Vec<_>>();
    if statements.len() != 1 || statements[0].kind() != "return_statement" {
        return None;
    }
    raw_comparable_children(statements[0])
        .into_iter()
        .find(|child| child.is_named())
}

fn async_flag(node: Node, src: &str) -> bool {
    direct_token_texts(node, src)
        .iter()
        .any(|text| text == "async")
}

fn generator_flag(node: Node, src: &str) -> bool {
    direct_token_texts(node, src).iter().any(|text| text == "*")
}

fn direct_token_texts(node: Node, src: &str) -> Vec<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| !child.is_named())
        .map(|child| node_text(child, src))
        .collect()
}
