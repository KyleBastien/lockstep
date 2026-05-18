use serde::{Deserialize, Serialize};

use crate::SeverityCounts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictKind {
    Approve,
    RequestChanges,
}

impl VerdictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictKind::Approve => "approve",
            VerdictKind::RequestChanges => "request_changes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub kind: VerdictKind,
    pub reason: String,
}

impl Verdict {
    pub fn approve() -> Self {
        Self {
            kind: VerdictKind::Approve,
            reason: "no divergence".into(),
        }
    }

    pub fn request_changes(reason: impl Into<String>) -> Self {
        Self {
            kind: VerdictKind::RequestChanges,
            reason: reason.into(),
        }
    }

    /// Approve when there are no errors; request changes otherwise.
    /// Warns / infos never block a verdict in v1.
    pub fn from_counts(counts: &SeverityCounts) -> Self {
        if counts.error == 0 {
            Self::approve()
        } else {
            Self::request_changes(format!("{} error(s) found", counts.error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdictkind_as_str_maps_each_variant() {
        assert_eq!(VerdictKind::Approve.as_str(), "approve");
        assert_eq!(VerdictKind::RequestChanges.as_str(), "request_changes");
    }

    #[test]
    fn verdict_approve_sets_default_reason() {
        let v = Verdict::approve();
        assert_eq!(v.kind, VerdictKind::Approve);
        assert_eq!(v.reason, "no divergence");
    }

    #[test]
    fn verdict_request_changes_stores_reason() {
        let v = Verdict::request_changes("nope");
        assert_eq!(v.kind, VerdictKind::RequestChanges);
        assert_eq!(v.reason, "nope");
    }

    #[test]
    fn verdict_from_counts_approves_on_zero_errors() {
        let c = SeverityCounts {
            error: 0,
            warn: 5,
            info: 9,
        };
        assert_eq!(Verdict::from_counts(&c).kind, VerdictKind::Approve);
    }

    #[test]
    fn verdict_from_counts_requests_changes_on_errors() {
        let c = SeverityCounts {
            error: 2,
            warn: 0,
            info: 0,
        };
        let v = Verdict::from_counts(&c);
        assert_eq!(v.kind, VerdictKind::RequestChanges);
        assert!(v.reason.contains("2"));
    }
}
