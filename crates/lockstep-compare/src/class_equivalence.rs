use lockstep_core::Finding;
use tree_sitter::Node;

use crate::callable_equivalence::compare_assigned_function_to_method;
use crate::node_utils::{
    direct_children_of_kind, first_named_child, node_text, raw_comparable_children, statement_block,
};

use crate::walk::{walk_regular, CacheAlias, WalkCtx};

pub(super) fn walk_class_body(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    let transform = class_transform(ctx, base, head);
    if transform.is_empty() {
        return false;
    }

    let mut method_ctx = ctx.clone();
    method_ctx.aliases.extend(transform.aliases.clone());
    for pair in &transform.method_pairs {
        compare_assigned_function_to_method(
            &method_ctx,
            pair.base_value,
            pair.head_method,
            findings,
        );
        if !ctx.report_all && !findings.is_empty() {
            return true;
        }
    }

    let mut child_ctx = ctx.clone();
    child_ctx
        .ignored_base_starts
        .extend(transform.ignored_base_starts);
    child_ctx
        .ignored_head_starts
        .extend(transform.ignored_head_starts);
    child_ctx.aliases.extend(transform.aliases);
    walk_regular(&child_ctx, base, head, findings);
    true
}

pub(super) fn is_cache_alias_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if base.kind() != "identifier" || head.kind() != "member_expression" {
        return false;
    }
    let base_name = node_text(base, ctx.base_src);
    ctx.aliases.iter().any(|alias| {
        alias.base_name == base_name
            && this_property_name(head, ctx.head_src)
                .as_deref()
                .is_some_and(|property| property == alias.head_property)
    })
}

struct ClassTransform<'a> {
    method_pairs: Vec<AssignedMethodPair<'a>>,
    ignored_base_starts: Vec<usize>,
    ignored_head_starts: Vec<usize>,
    aliases: Vec<CacheAlias>,
}

impl ClassTransform<'_> {
    fn is_empty(&self) -> bool {
        self.method_pairs.is_empty()
            && self.ignored_base_starts.is_empty()
            && self.ignored_head_starts.is_empty()
            && self.aliases.is_empty()
    }
}

#[derive(Clone, Copy)]
struct AssignedMethodPair<'a> {
    base_value: Node<'a>,
    head_method: Node<'a>,
}

fn class_transform<'a>(ctx: &WalkCtx, base: Node<'a>, head: Node<'a>) -> ClassTransform<'a> {
    let assignments = constructor_assignments(base, ctx.base_src);
    let head_methods = class_methods(head, ctx.head_src);
    let mut ignored_base_starts = Vec::new();
    let mut ignored_head_starts = Vec::new();
    let mut method_pairs = Vec::new();

    for assignment in assignments {
        if let Some(method) = head_methods.iter().find(|m| m.name == assignment.name) {
            method_pairs.push(AssignedMethodPair {
                base_value: assignment.value,
                head_method: method.node,
            });
            ignored_base_starts.push(assignment.statement.start_byte());
            ignored_head_starts.push(method.node.start_byte());
        }
    }

    let aliases = if ctx.allow_closure_cache_field_alias {
        cache_aliases(
            ctx,
            base,
            head,
            &mut ignored_base_starts,
            &mut ignored_head_starts,
        )
    } else {
        Vec::new()
    };

    ClassTransform {
        method_pairs,
        ignored_base_starts,
        ignored_head_starts,
        aliases,
    }
}

struct ConstructorAssignment<'a> {
    name: String,
    statement: Node<'a>,
    value: Node<'a>,
}

struct ClassMethod<'a> {
    name: String,
    node: Node<'a>,
}

fn constructor_assignments<'a>(class_body: Node<'a>, src: &str) -> Vec<ConstructorAssignment<'a>> {
    constructor_this_assignments(class_body, src, true)
        .into_iter()
        .map(|p| ConstructorAssignment {
            name: p.name,
            statement: p.statement,
            value: p.right,
        })
        .collect()
}

fn constructor_this_assignments<'a>(
    class_body: Node<'a>,
    src: &str,
    callable: bool,
) -> Vec<ThisPropertyAssignment<'a>> {
    let Some(body) = constructor_body(class_body, src) else {
        return Vec::new();
    };
    raw_comparable_children(body)
        .into_iter()
        .filter_map(|statement| parse_this_property_assignment(statement, src))
        .filter(|p| is_callable(p.right.kind()) == callable)
        .collect()
}

fn is_callable(kind: &str) -> bool {
    matches!(kind, "function_expression" | "arrow_function")
}

struct ThisPropertyAssignment<'a> {
    name: String,
    right: Node<'a>,
    statement: Node<'a>,
}

fn parse_this_property_assignment<'a>(
    statement: Node<'a>,
    src: &str,
) -> Option<ThisPropertyAssignment<'a>> {
    if statement.kind() != "expression_statement" {
        return None;
    }
    let assignment = first_named_child(statement)?;
    if assignment.kind() != "assignment_expression" {
        return None;
    }
    let left = assignment.child_by_field_name("left")?;
    let right = unwrap_expression(assignment.child_by_field_name("right")?);
    let name = this_property_name(left, src)?;
    Some(ThisPropertyAssignment {
        name,
        right,
        statement,
    })
}

fn class_methods<'a>(class_body: Node<'a>, src: &str) -> Vec<ClassMethod<'a>> {
    raw_comparable_children(class_body)
        .into_iter()
        .filter(|child| child.kind() == "method_definition")
        .filter_map(|node| method_name(node, src).map(|name| ClassMethod { name, node }))
        .collect()
}

fn constructor_body<'a>(class_body: Node<'a>, src: &str) -> Option<Node<'a>> {
    let constructor = class_methods(class_body, src)
        .into_iter()
        .find(|method| method.name == "constructor")?
        .node;
    statement_block(constructor)
}

fn cache_aliases(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    ignored_base_starts: &mut Vec<usize>,
    ignored_head_starts: &mut Vec<usize>,
) -> Vec<CacheAlias> {
    let base_vars = constructor_cache_declarations(base, ctx.base_src);
    let mut head_caches = class_fields(head, ctx.head_src);
    head_caches.extend(head_constructor_cache_assignments(head, ctx.head_src));
    let mut aliases = Vec::new();
    for base_var in base_vars {
        if let Some(field) = head_caches.iter().find(|field| {
            strip_leading_underscore(&field.name) == base_var.name
                && normalized_source(field.value, ctx.head_src)
                    == normalized_source(base_var.value, ctx.base_src)
        }) {
            aliases.push(CacheAlias {
                base_name: base_var.name.clone(),
                head_property: field.name.clone(),
            });
            ignored_base_starts.push(base_var.statement.start_byte());
            ignored_head_starts.push(field.node.start_byte());
        }
    }
    aliases
}

struct CacheDeclaration<'a> {
    name: String,
    value: Node<'a>,
    statement: Node<'a>,
}

struct ClassField<'a> {
    name: String,
    value: Node<'a>,
    node: Node<'a>,
}

fn constructor_cache_declarations<'a>(
    class_body: Node<'a>,
    src: &str,
) -> Vec<CacheDeclaration<'a>> {
    let Some(body) = constructor_body(class_body, src) else {
        return Vec::new();
    };
    raw_comparable_children(body)
        .into_iter()
        .filter_map(|statement| cache_declaration(statement, src))
        .collect()
}

fn cache_declaration<'a>(statement: Node<'a>, src: &str) -> Option<CacheDeclaration<'a>> {
    if !matches!(
        statement.kind(),
        "lexical_declaration" | "variable_declaration"
    ) {
        return None;
    }
    let declarators = direct_children_of_kind(statement, "variable_declarator");
    if declarators.len() != 1 {
        return None;
    }
    let declarator = declarators[0];
    let name = declarator.child_by_field_name("name")?;
    let value = declarator.child_by_field_name("value")?;
    Some(CacheDeclaration {
        name: node_text(name, src),
        value,
        statement,
    })
}

fn class_fields<'a>(class_body: Node<'a>, src: &str) -> Vec<ClassField<'a>> {
    raw_comparable_children(class_body)
        .into_iter()
        .filter(|child| child.kind() == "field_definition")
        .filter_map(|node| {
            let name = node.child_by_field_name("property")?;
            let value = node.child_by_field_name("value")?;
            Some(ClassField {
                name: node_text(name, src),
                value,
                node,
            })
        })
        .collect()
}

fn head_constructor_cache_assignments<'a>(class_body: Node<'a>, src: &str) -> Vec<ClassField<'a>> {
    constructor_this_assignments(class_body, src, false)
        .into_iter()
        .map(|p| ClassField {
            name: p.name,
            value: p.right,
            node: p.statement,
        })
        .collect()
}

fn method_name(node: Node, src: &str) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("property"))
        .map(|name| node_text(name, src))
}

fn this_property_name(node: Node, src: &str) -> Option<String> {
    if node.kind() != "member_expression" {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if node_text(object, src) != "this" {
        return None;
    }
    node.child_by_field_name("property")
        .map(|property| node_text(property, src))
}

fn unwrap_expression(mut node: Node) -> Node {
    while matches!(node.kind(), "parenthesized_expression") {
        let Some(child) = first_named_child(node) else {
            break;
        };
        node = child;
    }
    node
}

fn normalized_source(node: Node, src: &str) -> String {
    crate::node_utils::compact_node_text(node, src)
}

fn strip_leading_underscore(name: &str) -> &str {
    name.strip_prefix('_').unwrap_or(name)
}
