use serde::{Deserialize, Serialize};

use crate::{Finding, Severity, Verdict};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub error: u32,
    pub warn: u32,
    pub info: u32,
}

impl SeverityCounts {
    pub fn from_findings(findings: &[Finding]) -> Self {
        let mut c = Self::default();
        for f in findings {
            match f.severity {
                Severity::Error => c.error += 1,
                Severity::Warn => c.warn += 1,
                Severity::Info => c.info += 1,
            }
        }
        c
    }

    pub fn total(&self) -> u32 {
        self.error + self.warn + self.info
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub counts: SeverityCounts,
    pub verdict: Verdict,
    /// One-line synopsis. Designed for LLM consumption.
    pub summary: String,
    /// Number of file pairs the engine considered (some may have zero findings).
    pub pairs_examined: u32,
}

impl Report {
    pub fn from_findings(findings: Vec<Finding>, pairs_examined: u32) -> Self {
        let counts = SeverityCounts::from_findings(&findings);
        let verdict = Verdict::from_counts(&counts);
        let summary = render_summary(&counts, pairs_examined, &verdict);
        Self {
            findings,
            counts,
            verdict,
            summary,
            pairs_examined,
        }
    }
}

fn render_summary(counts: &SeverityCounts, pairs: u32, v: &Verdict) -> String {
    if counts.total() == 0 {
        format!("approve: {} pair(s) examined, no divergence", pairs)
    } else {
        format!(
            "{}: {} pair(s) examined, {} error / {} warn / {} info",
            v.kind.as_str(),
            pairs,
            counts.error,
            counts.warn,
            counts.info
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Finding};
    use std::path::PathBuf;

    #[test]
    fn empty_report_approves() {
        let r = Report::from_findings(vec![], 3);
        assert_eq!(r.verdict.kind, crate::VerdictKind::Approve);
        assert!(r.summary.contains("approve"));
    }

    #[test]
    fn error_finding_requests_changes() {
        let f = Finding::new(PathBuf::from("a.ts"), Category::KindMismatch, "x");
        let r = Report::from_findings(vec![f], 1);
        assert_eq!(r.verdict.kind, crate::VerdictKind::RequestChanges);
        assert_eq!(r.counts.error, 1);
    }

    #[test]
    fn severity_counts_total_sums_all_severities() {
        let counts = SeverityCounts {
            error: 2,
            warn: 3,
            info: 5,
        };
        assert_eq!(counts.total(), 10);
        assert_eq!(SeverityCounts::default().total(), 0);
    }
}
