use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        }
    }
}

/// Why a pair diverged. Drives the human message and remediation hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// AST node kinds at the same path differ (e.g., `if_statement` vs `call_expression`).
    KindMismatch,
    /// Same kind, but leaf token text differs (identifier renamed, literal value changed).
    TokenMismatch,
    /// Same kind, but named-child arity differs (extra/missing argument, branch, statement).
    ArityMismatch,
    /// Statement present on one side, absent on the other (after type-strip).
    DroppedStatement,
    /// Head has a TS construct that v1 refuses to compare — e.g. enum, ctor parameter property.
    StrippedTsConstruct,
    /// Parser failed on stripped/normalized source — likely an elision bug.
    ParseError,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::KindMismatch => "kind_mismatch",
            Category::TokenMismatch => "token_mismatch",
            Category::ArityMismatch => "arity_mismatch",
            Category::DroppedStatement => "dropped_statement",
            Category::StrippedTsConstruct => "stripped_ts_construct",
            Category::ParseError => "parse_error",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Category::KindMismatch => {
                "AST nodes at the same position have different kinds. The migration changed the \
                 shape of the code, not just its types. Revert the structural change and land it \
                 as a separate non-migration PR."
            }
            Category::TokenMismatch => {
                "Leaf tokens differ (an identifier was renamed or a literal value was changed). \
                 Migrations should not rename or re-value tokens. Restore the original names."
            }
            Category::ArityMismatch => {
                "Same node kind, but a different number of children — e.g. an extra argument, a \
                 missing branch, an inserted statement. Land structural edits separately."
            }
            Category::DroppedStatement => {
                "A statement that exists on the base side has no counterpart on the head side \
                 (or vice versa) after type-stripping. Restore the missing statement."
            }
            Category::StrippedTsConstruct => {
                "Head uses a TypeScript construct (enum, constructor parameter property, …) that \
                 lockstep cannot verify mechanically. Desugar the construct in head, or set the \
                 corresponding `allow_*` flag in .lockstep/config.toml after manual review."
            }
            Category::ParseError => {
                "Stripped/normalized source failed to parse as JavaScript. This is usually a bug \
                 in lockstep's stripper. Re-run with --verbose to dump the normalized source."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub path: PathBuf,
    pub category: Category,
    pub severity: Severity,
    pub base_kind: Option<String>,
    pub head_kind: Option<String>,
    pub base_line: Option<u32>,
    pub head_line: Option<u32>,
    pub base_snippet: Option<String>,
    pub head_snippet: Option<String>,
    pub message: String,
}

impl Finding {
    pub fn new(path: impl Into<PathBuf>, category: Category, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            category,
            severity: Severity::Error,
            base_kind: None,
            head_kind: None,
            base_line: None,
            head_line: None,
            base_snippet: None,
            head_snippet: None,
            message: message.into(),
        }
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_kinds(mut self, base: impl Into<String>, head: impl Into<String>) -> Self {
        self.base_kind = Some(base.into());
        self.head_kind = Some(head.into());
        self
    }

    pub fn with_lines(mut self, base: u32, head: u32) -> Self {
        self.base_line = Some(base);
        self.head_line = Some(head);
        self
    }

    pub fn with_snippets(mut self, base: impl Into<String>, head: impl Into<String>) -> Self {
        self.base_snippet = Some(base.into());
        self.head_snippet = Some(head.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_builder_round_trips() {
        let f = Finding::new("a.ts", Category::KindMismatch, "boom")
            .with_severity(Severity::Warn)
            .with_kinds("if_statement", "call_expression")
            .with_lines(3, 5);
        assert_eq!(f.severity, Severity::Warn);
        assert_eq!(f.base_line, Some(3));
        assert_eq!(f.head_kind.as_deref(), Some("call_expression"));
    }

    #[test]
    fn category_explanations_are_non_empty() {
        for c in [
            Category::KindMismatch,
            Category::TokenMismatch,
            Category::ArityMismatch,
            Category::DroppedStatement,
            Category::StrippedTsConstruct,
            Category::ParseError,
        ] {
            assert!(!c.explain().is_empty());
        }
    }

    #[test]
    fn severity_as_str_maps_each_variant() {
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Warn.as_str(), "warn");
        assert_eq!(Severity::Error.as_str(), "error");
    }

    #[test]
    fn category_as_str_maps_each_variant() {
        assert_eq!(Category::KindMismatch.as_str(), "kind_mismatch");
        assert_eq!(Category::TokenMismatch.as_str(), "token_mismatch");
        assert_eq!(Category::ArityMismatch.as_str(), "arity_mismatch");
        assert_eq!(Category::DroppedStatement.as_str(), "dropped_statement");
        assert_eq!(
            Category::StrippedTsConstruct.as_str(),
            "stripped_ts_construct"
        );
        assert_eq!(Category::ParseError.as_str(), "parse_error");
    }

    #[test]
    fn finding_with_snippets_stores_both_sides() {
        let f = Finding::new("a.ts", Category::KindMismatch, "x").with_snippets("BASE", "HEAD");
        assert_eq!(f.base_snippet.as_deref(), Some("BASE"));
        assert_eq!(f.head_snippet.as_deref(), Some("HEAD"));
    }
}
