//! Tables describing what tree-sitter-typescript nodes to remove.
//!
//! Three buckets:
//!   * [`DROP_STATEMENT_KINDS`]  — statement-position nodes whose entire byte
//!     range is replaced with whitespace (e.g. `interface_declaration`).
//!   * [`UNWRAP_EXPRESSION_KINDS`] — wrappers whose own bytes are removed but
//!     whose first expression child is kept (e.g. `as_expression`).
//!   * [`DROP_CHILD_KINDS`] — children that must be removed while keeping the
//!     parent (e.g. `type_annotation`, `accessibility_modifier`).

/// Whole-statement deletions. Replaced with whitespace so TS-only declarations
/// do not leave `empty_statement` nodes in the JavaScript AST.
pub const DROP_STATEMENT_KINDS: &[&str] = &[
    "interface_declaration",
    "type_alias_declaration",
    "ambient_declaration",
    "internal_module",
    "import_alias",
    "function_signature",
    "method_signature",
    "abstract_method_signature",
];

/// Expression wrappers. The wrapper node is replaced by its first named child.
pub const UNWRAP_EXPRESSION_KINDS: &[&str] = &[
    "as_expression",
    "satisfies_expression",
    "type_assertion",
    "non_null_expression",
    "instantiation_expression",
];

/// Children removed in place (preserve parent, splice the child's bytes out).
pub const DROP_CHILD_KINDS: &[&str] = &[
    "type_annotation",
    "type_arguments",
    "type_parameters",
    "type_predicate_annotation",
    "asserts_annotation",
    "implements_clause",
    "accessibility_modifier",
    "readonly",
    "override_modifier",
    "abstract_modifier",
    "declare",
    "optional_parameter",
    "predefined_type",
];

/// Marker tokens whose anonymous (unnamed) bytes need to disappear too — the
/// `?` after an optional parameter, `!` definite-assignment on initialized
/// fields. tree-sitter exposes these as anonymous children of their parent.
pub const DROP_TOKEN_TEXTS: &[&str] = &["?", "!"];

/// Statements / declarations that emit JS but lockstep can't *mechanically*
/// prove equivalent. These produce a `StrippedTsConstruct` finding rather than
/// being silently elided.
pub const REJECT_KINDS: &[&str] = &["enum_declaration"];

pub fn is_drop_statement(kind: &str) -> bool {
    DROP_STATEMENT_KINDS.contains(&kind)
}

pub fn is_unwrap_expression(kind: &str) -> bool {
    UNWRAP_EXPRESSION_KINDS.contains(&kind)
}

pub fn is_drop_child(kind: &str) -> bool {
    DROP_CHILD_KINDS.contains(&kind)
}

pub fn is_reject(kind: &str) -> bool {
    REJECT_KINDS.contains(&kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_drop_statement_matches_table_entries() {
        assert!(is_drop_statement("interface_declaration"));
        assert!(is_drop_statement("type_alias_declaration"));
        assert!(!is_drop_statement("call_expression"));
    }

    #[test]
    fn is_unwrap_expression_matches_table_entries() {
        assert!(is_unwrap_expression("as_expression"));
        assert!(is_unwrap_expression("non_null_expression"));
        assert!(!is_unwrap_expression("if_statement"));
    }

    #[test]
    fn is_drop_child_matches_table_entries() {
        assert!(is_drop_child("type_annotation"));
        assert!(is_drop_child("accessibility_modifier"));
        assert!(!is_drop_child("identifier"));
    }

    #[test]
    fn is_reject_matches_table_entries() {
        assert!(is_reject("enum_declaration"));
        assert!(!is_reject("class_declaration"));
    }
}
