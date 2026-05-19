use tree_sitter::Node;

pub(super) fn raw_comparable_children(node: Node) -> Vec<Node> {
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

pub(super) fn is_trivia(node: Node) -> bool {
    matches!(node.kind(), "comment" | "hash_bang_line")
}

pub(super) fn is_meaningful_unnamed(kind: &str) -> bool {
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

pub(super) fn first_named_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let child = node.named_children(&mut cursor).next();
    child
}

pub(super) fn find_direct_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    raw_comparable_children(node)
        .into_iter()
        .find(|child| child.kind() == kind)
}

pub(super) fn statement_block(node: Node) -> Option<Node> {
    node.child_by_field_name("body")
        .filter(|body| body.kind() == "statement_block")
        .or_else(|| find_direct_child(node, "statement_block"))
}

pub(super) fn direct_children_of_kind<'a>(node: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    raw_comparable_children(node)
        .into_iter()
        .filter(|child| child.kind() == kind)
        .collect()
}

pub(super) fn find_descendant<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_descendant(child, kind) {
            return Some(found);
        }
    }
    None
}

pub(super) fn node_text(node: Node, src: &str) -> String {
    node.utf8_text(src.as_bytes()).unwrap_or("").to_string()
}

pub(super) fn compact_node_text(node: Node, src: &str) -> String {
    node_text(node, src)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}
