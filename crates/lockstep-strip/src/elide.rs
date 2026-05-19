//! Walk a TS tree, collect byte-range edits, splice them out of the source.

use std::borrow::Cow;

use thiserror::Error;
use tree_sitter::{Node, Parser, Tree, TreeCursor};

use crate::fields::{is_parameter_property, is_typeonly_field};
use crate::imports::{is_type_only_statement, type_specifier_ranges};
use crate::ts_nodes;

#[derive(Debug, Error)]
pub enum StripError {
    #[error("tree-sitter: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsFlavor {
    Ts,
    Tsx,
}

impl TsFlavor {
    pub fn from_extension(path: &std::path::Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("tsx") => TsFlavor::Tsx,
            _ => TsFlavor::Ts,
        }
    }

    fn ts_language(self) -> tree_sitter::Language {
        match self {
            TsFlavor::Ts => tree_sitter_typescript::language_typescript(),
            TsFlavor::Tsx => tree_sitter_typescript::language_tsx(),
        }
    }
}

/// A construct lockstep refuses to mechanically equate to JS. Caller
/// surfaces these as `StrippedTsConstruct` findings.
#[derive(Debug, Clone)]
pub struct Rejection {
    pub kind: &'static str,
    pub line: u32,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct StripOutput {
    pub output: String,
    pub rejections: Vec<Rejection>,
}

#[derive(Debug, Clone)]
pub(crate) struct Edit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) replacement: Cow<'static, str>,
}

impl Edit {
    pub(crate) fn drop_range(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            replacement: Cow::Borrowed(""),
        }
    }
    pub(crate) fn replace_with_whitespace(src: &str, start: usize, end: usize) -> Self {
        let replacement = src[start..end]
            .chars()
            .map(|c| if matches!(c, '\n' | '\r') { c } else { ' ' })
            .collect::<String>();
        Self {
            start,
            end,
            replacement: Cow::Owned(replacement),
        }
    }
    pub(crate) fn replace_with_semi(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            replacement: Cow::Borrowed(";"),
        }
    }
}

pub fn strip(src: &str, flavor: TsFlavor) -> Result<StripOutput, StripError> {
    let mut parser = Parser::new();
    parser
        .set_language(&flavor.ts_language())
        .map_err(|e| StripError::Parse(e.to_string()))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| StripError::Parse("parse returned None".into()))?;

    let mut edits: Vec<Edit> = Vec::new();
    let mut rejections: Vec<Rejection> = Vec::new();
    walk(&tree, src, &mut edits, &mut rejections);

    let output = apply_edits(src, edits);
    Ok(StripOutput { output, rejections })
}

fn walk(tree: &Tree, src: &str, edits: &mut Vec<Edit>, rejections: &mut Vec<Rejection>) {
    let mut cursor = tree.walk();
    visit(&mut cursor, src, edits, rejections);
}

fn visit(
    cursor: &mut TreeCursor,
    src: &str,
    edits: &mut Vec<Edit>,
    rejections: &mut Vec<Rejection>,
) {
    let node = cursor.node();
    classify(node, src, edits, rejections);

    if cursor.goto_first_child() {
        loop {
            visit(cursor, src, edits, rejections);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Dispatch a single tree-sitter node to its matching handler. Each handler
/// returns `true` when the node is fully accounted for (caller stops).
fn classify(node: Node, src: &str, edits: &mut Vec<Edit>, rejections: &mut Vec<Rejection>) {
    if handle_reject(node, src, edits, rejections) {
        return;
    }
    if handle_import_export(node, src, edits) {
        return;
    }
    if handle_drop_statement(node, src, edits) {
        return;
    }
    if handle_unwrap_expression(node, edits) {
        return;
    }
    if handle_drop_child(node, edits) {
        return;
    }
    if handle_typeonly_field(node, src, edits) {
        return;
    }
    handle_parameter_property(node, src, rejections);
    handle_anonymous_marker(node, src, edits);
}

fn handle_reject(
    node: Node,
    src: &str,
    edits: &mut Vec<Edit>,
    rejections: &mut Vec<Rejection>,
) -> bool {
    if !ts_nodes::is_reject(node.kind()) {
        return false;
    }
    rejections.push(Rejection {
        kind: "enum_declaration",
        line: line_of(node),
        snippet: node.utf8_text(src.as_bytes()).unwrap_or("").to_string(),
    });
    // Still strip the enum byte-range so the rest of the file parses as JS.
    edits.push(Edit::replace_with_semi(node.start_byte(), node.end_byte()));
    true
}

/// Returns true when the entire import/export statement was dropped. Returns
/// false (so other handlers can run on inner nodes) when only individual type
/// specifiers were dropped.
fn handle_import_export(node: Node, src: &str, edits: &mut Vec<Edit>) -> bool {
    let kind = node.kind();
    if kind != "import_statement" && kind != "export_statement" {
        return false;
    }
    if is_type_only_statement(node, src) {
        edits.push(Edit::replace_with_whitespace(
            src,
            node.start_byte(),
            node.end_byte(),
        ));
        return true;
    }
    for (s, e) in type_specifier_ranges(node, src) {
        edits.push(Edit::drop_range(s, e));
    }
    false
}

fn handle_drop_statement(node: Node, src: &str, edits: &mut Vec<Edit>) -> bool {
    if !ts_nodes::is_drop_statement(node.kind()) {
        return false;
    }
    edits.push(Edit::replace_with_whitespace(
        src,
        node.start_byte(),
        node.end_byte(),
    ));
    true
}

fn handle_unwrap_expression(node: Node, edits: &mut Vec<Edit>) -> bool {
    if !ts_nodes::is_unwrap_expression(node.kind()) {
        return false;
    }
    if let Some(keep) = first_value_child(node) {
        edits.push(Edit::drop_range(node.start_byte(), keep.start_byte()));
        edits.push(Edit::drop_range(keep.end_byte(), node.end_byte()));
    }
    true
}

fn handle_drop_child(node: Node, edits: &mut Vec<Edit>) -> bool {
    if !ts_nodes::is_drop_child(node.kind()) {
        return false;
    }
    edits.push(Edit::drop_range(node.start_byte(), node.end_byte()));
    true
}

fn handle_typeonly_field(node: Node, src: &str, edits: &mut Vec<Edit>) -> bool {
    if node.kind() != "public_field_definition" || !is_typeonly_field(node, src) {
        return false;
    }
    let mut end = node.end_byte();
    let bytes = src.as_bytes();
    if end < bytes.len() && bytes[end] == b';' {
        end += 1;
    }
    edits.push(Edit::drop_range(node.start_byte(), end));
    true
}

fn handle_parameter_property(node: Node, src: &str, rejections: &mut Vec<Rejection>) {
    let kind = node.kind();
    if kind != "required_parameter" && kind != "optional_parameter" {
        return;
    }
    if !is_parameter_property(node) {
        return;
    }
    rejections.push(Rejection {
        kind: "parameter_property",
        line: line_of(node),
        snippet: node.utf8_text(src.as_bytes()).unwrap_or("").to_string(),
    });
}

/// Drop standalone `?` (optional-parameter) and `!` (definite-assignment) tokens
/// when they appear under a parameter/field/method node. Anywhere else (`a ?: b`,
/// template strings, etc.) these tokens are semantically meaningful, so we leave
/// them alone.
fn handle_anonymous_marker(node: Node, src: &str, edits: &mut Vec<Edit>) {
    if node.is_named() {
        return;
    }
    let text = node.utf8_text(src.as_bytes()).unwrap_or("");
    if text != "?" && text != "!" {
        return;
    }
    let Some(parent) = node.parent() else {
        return;
    };
    if !is_marker_parent(parent.kind()) {
        return;
    }
    edits.push(Edit::drop_range(node.start_byte(), node.end_byte()));
}

fn is_marker_parent(kind: &str) -> bool {
    matches!(
        kind,
        "required_parameter"
            | "optional_parameter"
            | "property_identifier"
            | "public_field_definition"
            | "method_signature"
            | "property_signature"
            | "method_definition"
    )
}

fn first_value_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_type_like_kind(child.kind()) {
            continue;
        }
        return Some(child);
    }
    None
}

fn is_type_like_kind(kind: &str) -> bool {
    matches!(
        kind,
        "type"
            | "type_arguments"
            | "type_parameters"
            | "type_annotation"
            | "type_predicate_annotation"
            | "predefined_type"
            | "generic_type"
            | "literal_type"
            | "tuple_type"
            | "union_type"
            | "intersection_type"
            | "function_type"
            | "constructor_type"
            | "array_type"
            | "object_type"
            | "type_identifier"
            | "nested_type_identifier"
            | "conditional_type"
            | "mapped_type_clause"
            | "lookup_type"
            | "index_type_query"
            | "readonly_type"
    )
}

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

fn apply_edits(src: &str, mut edits: Vec<Edit>) -> String {
    // Sort by start ascending; for equal starts, longer span first so we keep
    // the outer edit and drop contained inner ones.
    edits.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut filtered: Vec<Edit> = Vec::with_capacity(edits.len());
    let mut last_end = 0usize;
    for e in edits {
        if e.start < last_end {
            continue;
        }
        last_end = e.end;
        filtered.push(e);
    }

    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for e in filtered {
        if e.start > cursor {
            out.push_str(&src[cursor..e.start]);
        }
        out.push_str(&e.replacement);
        cursor = e.end;
    }
    if cursor < src.len() {
        out.push_str(&src[cursor..]);
    }
    out
}
