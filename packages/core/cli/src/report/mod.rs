//! The shape a per-file finding report shares, so no report has to restate it.
//!
//! Most reports in this tool produce per-file findings with a span, a line, and
//! a summary. The *envelope* is identical every time — the file list, the
//! unmodelled-dialect notice, the counts, the policy gate, the text/JSON
//! switch — while the finding is different every time. Factoring the envelope
//! and leaving the finding bespoke keeps each report's own file down to its
//! actual analysis, without erasing the finding into an untyped bag of strings.
//!
//! It lives in core rather than in one feature because three feature packages
//! now produce reports of this shape, and a copy per package is three places
//! for the output contract to drift.

pub mod graph;
pub mod interop;
pub mod render;

use std::path::PathBuf;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteSpan;
use serde_json::Value;

/// One report's finding, rendered by the shared envelope.
///
/// The envelope supplies the path, span, and line; an implementor supplies only
/// what is specific to it. `kind` leads every text row so a consumer piping
/// output through `grep` can select one class of finding without parsing JSON.
pub trait Finding {
    fn kind(&self) -> &'static str;
    fn span(&self) -> ByteSpan;
    fn line(&self) -> usize;
    /// The tab-separated columns after the leading `kind`, path, and line.
    fn text_columns(&self) -> Vec<String>;
    /// The finding's own JSON fields. `line` and `span` are added around them.
    fn json_fields(&self) -> Vec<(&'static str, Value)>;

    /// How serious this finding is, for the interop formats that carry a level.
    ///
    /// Defaulted rather than required. Most of these reports describe one thing
    /// worth looking at, with no internal gradation, and forcing every one to
    /// restate `Warning` would be ceremony. A report whose findings *are*
    /// graded — a defect versus an observation — overrides it.
    fn severity(&self) -> FindingSeverity {
        FindingSeverity::Warning
    }

    /// One line of prose for a consumer that has no column layout: a SARIF
    /// result message, a JUnit failure message, a Code Climate description.
    ///
    /// Defaults to the text columns joined, which is what those columns already
    /// are — a human-readable description split for `cut`. A report with a
    /// better sentence to offer overrides it.
    fn message(&self) -> String {
        let columns = self.text_columns();
        if columns.is_empty() {
            self.kind().to_owned()
        } else {
            columns.join(" ")
        }
    }
}

/// The level an interop consumer files a finding under.
///
/// Three rungs because that is the intersection of what the target formats can
/// express: SARIF has `note`/`warning`/`error`, Code Climate has
/// `info`/`minor`/`major`, and both map onto these without inventing a rung
/// this tool cannot justify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingSeverity {
    Note,
    Warning,
    Error,
}

impl FindingSeverity {
    /// The SARIF `level` vocabulary, which is also what the text and CSV
    /// outputs print.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// The Code Climate `severity` vocabulary, which is a different set of
    /// words for the same three rungs.
    #[must_use]
    pub const fn code_climate(self) -> &'static str {
        match self {
            Self::Note => "info",
            Self::Warning => "minor",
            Self::Error => "major",
        }
    }
}

/// One analyzed file.
#[derive(Debug, Clone)]
pub struct FileFindings<F> {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Whether this report's analysis covers the file's dialect.
    ///
    /// Printed rather than implied. An empty finding list means "nothing found"
    /// for a modelled dialect and "nothing looked for" otherwise, and a
    /// consumer cannot tell those apart from the list alone.
    pub dialect_modelled: bool,
    pub findings: Vec<F>,
    /// Per-file counts this report wants beside its findings.
    pub summary: Vec<(&'static str, Value)>,
}

impl<F: Finding> FileFindings<F> {
    #[must_use]
    pub fn new(
        path: PathBuf,
        dialect: Dialect,
        dialect_modelled: bool,
        mut findings: Vec<F>,
        summary: Vec<(&'static str, Value)>,
    ) -> Self {
        // Source order, imposed once here. Several of these analyses collect
        // through a map or visit clauses out of order, and the
        // byte-identical-output contract does not survive either.
        findings.sort_by_key(|finding| (finding.span().start().get(), finding.span().end().get()));
        Self {
            path,
            dialect,
            dialect_modelled,
            findings,
            summary,
        }
    }
}

/// The outcome of a report's `--fail-on-*` gate.
#[derive(Debug, Clone)]
pub struct ReportPolicy {
    /// The flag that armed the gate, or `None` when nothing armed it.
    pub gate: Option<&'static str>,
    pub finding_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

impl ReportPolicy {
    /// Fails when any file has a finding, which is the gate most of these
    /// reports want: their findings are defects.
    #[must_use]
    pub fn fail_on_any<F: Finding>(
        gate: Option<&'static str>,
        reports: &[FileFindings<F>],
        describe: impl Fn(&FileFindings<F>) -> String,
    ) -> Self {
        let finding_count = reports.iter().map(|report| report.findings.len()).sum();
        let violations = if gate.is_some() {
            reports
                .iter()
                .filter(|report| !report.findings.is_empty())
                .map(describe)
                .collect()
        } else {
            Vec::new()
        };
        Self {
            gate,
            finding_count,
            passed: violations.is_empty(),
            violations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::ByteOffset;

    #[derive(Debug, Clone)]
    struct Probe(usize);

    impl Finding for Probe {
        fn kind(&self) -> &'static str {
            "probe"
        }
        fn span(&self) -> ByteSpan {
            ByteSpan::new(ByteOffset::new(self.0), ByteOffset::new(self.0 + 1))
        }
        fn line(&self) -> usize {
            1
        }
        fn text_columns(&self) -> Vec<String> {
            Vec::new()
        }
        fn json_fields(&self) -> Vec<(&'static str, Value)> {
            Vec::new()
        }
    }

    fn report(starts: &[usize]) -> FileFindings<Probe> {
        FileFindings::new(
            PathBuf::from("t.lisp"),
            Dialect::CommonLisp,
            true,
            starts.iter().copied().map(Probe).collect(),
            Vec::new(),
        )
    }

    #[test]
    fn construction_imposes_source_order() {
        let report = report(&[9, 1, 5]);
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.0)
            .collect::<Vec<_>>();
        assert_eq!(starts, vec![1, 5, 9]);
    }

    #[test]
    fn an_unarmed_gate_never_fails_however_many_findings_there_are() {
        let policy = ReportPolicy::fail_on_any(None, &[report(&[1, 2])], |_| "boom".to_owned());
        assert!(policy.passed);
        assert_eq!(policy.finding_count, 2);
        assert!(policy.violations.is_empty());
    }

    #[test]
    fn an_armed_gate_fails_once_a_file_has_a_finding() {
        let policy = ReportPolicy::fail_on_any(Some("--fail-on-x"), &[report(&[1])], |report| {
            format!("{} has findings", report.path.display())
        });
        assert!(!policy.passed);
        assert_eq!(policy.violations, vec!["t.lisp has findings".to_owned()]);
    }

    #[test]
    fn an_armed_gate_passes_a_clean_file() {
        let policy =
            ReportPolicy::fail_on_any(Some("--fail-on-x"), &[report(&[])], |_| "boom".to_owned());
        assert!(policy.passed);
    }
}
