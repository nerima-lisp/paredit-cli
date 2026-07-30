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

pub mod budget;
pub mod graph;
pub mod interop;
pub mod next_command;
pub mod render;

use std::path::PathBuf;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::line_index::LineIndex;
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

/// The 1-based line a byte offset falls on, counted from the start of `source`.
///
/// **This is O(offset).** A caller that asks for more than a couple of lines
/// per file wants [`LineIndex`] instead, which pays O(n) once and answers each
/// lookup in O(log n).
///
/// That distinction is not hypothetical. Every report on this envelope used to
/// call this once per finding, and the doc comment here argued it was fine
/// because "findings are rare". They are not: on a 23 KB document with 1136
/// findings the counts touched 13.5 MB, and sixteen times the forms produced
/// three hundred and fifty times the bytes. Worse, the `examine_*` functions
/// these reports share with the lint engine paid it on the engine's path too,
/// where the line is computed and immediately discarded. The line for a
/// *finding* now comes from [`FileFindings::line_of`], and this function is
/// left only for the callers that genuinely want one or two offsets resolved
/// against a source they hold anyway — a form's first and last line, say.
///
/// An offset past the end of `source` answers for the end of `source` rather
/// than panicking, so a stale span degrades to a wrong line rather than a
/// crash.
#[must_use]
pub fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
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
    /// Line starts for this file, built once in [`FileFindings::new`].
    ///
    /// Private, and the reason [`Finding`] has no `line` method. Every report
    /// on this envelope reports a line for the start of the span it already
    /// carries, so the line is a *function of the finding*, not a fact about
    /// it — and a function the envelope can evaluate for itself.
    ///
    /// Having each finding carry its own line meant resolving one during the
    /// analysis, and the only tool for that was a newline count from byte 0:
    /// O(offset) per finding, quadratic in the file. That cost was paid even
    /// on the lint engine's path, which shares these analyses through the
    /// `examine_*` functions, converts each item into a span and a message,
    /// and never reads the line at all. Holding the index here instead makes
    /// it O(n) once per file and O(log n) per lookup, and makes it impossible
    /// for a report to *have* a line without a source to resolve it against.
    lines: LineIndex,
}

impl<F: Finding> FileFindings<F> {
    /// Builds one file's report.
    ///
    /// `source` is the text the findings' spans index into. It is taken rather
    /// than derived because it is what makes [`FileFindings::line_of`] total:
    /// a report cannot be constructed without the one thing a line needs.
    #[must_use]
    pub fn new(
        path: PathBuf,
        dialect: Dialect,
        dialect_modelled: bool,
        source: &str,
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
            lines: LineIndex::new(source),
        }
    }

    /// The 1-based line one of this file's findings starts on.
    ///
    /// Answers for the start of `finding`'s span, which is what every report
    /// on this envelope meant by "the line" before the line lived here.
    #[must_use]
    pub fn line_of(&self, finding: &F) -> usize {
        self.lines.line_of(finding.span().start().get())
    }

    /// The same file carrying only the findings `keep` accepts.
    ///
    /// The gate-narrowing shape: a report lists every finding but fails a
    /// build on only some of them, so the policy is evaluated against a
    /// subset. This exists rather than a struct literal because the line index
    /// is private and *not* reconstructible from the public fields — the
    /// source it was built from is long gone by the time a gate runs.
    #[must_use]
    pub fn retained(&self, keep: impl Fn(&F) -> bool) -> Self
    where
        F: Clone,
    {
        Self {
            path: self.path.clone(),
            dialect: self.dialect,
            dialect_modelled: self.dialect_modelled,
            // Already in source order; filtering preserves it.
            findings: self
                .findings
                .iter()
                .filter(|finding| keep(finding))
                .cloned()
                .collect(),
            summary: self.summary.clone(),
            lines: self.lines.clone(),
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
        fn text_columns(&self) -> Vec<String> {
            Vec::new()
        }
        fn json_fields(&self) -> Vec<(&'static str, Value)> {
            Vec::new()
        }
    }

    /// Four bytes per line, so a probe at offset `n` belongs on line
    /// `n / 4 + 1` and both an unresolved line and an off-by-one are visible.
    const SOURCE: &str = "(a)\n(b)\n(c)\n";

    fn report(starts: &[usize]) -> FileFindings<Probe> {
        FileFindings::new(
            PathBuf::from("t.lisp"),
            Dialect::CommonLisp,
            true,
            SOURCE,
            starts.iter().copied().map(Probe).collect(),
            Vec::new(),
        )
    }

    #[test]
    fn the_first_byte_is_on_line_one() {
        assert_eq!(line_of("(a)\n(b)\n", 0), 1);
    }

    #[test]
    fn a_byte_after_one_newline_is_on_line_two() {
        assert_eq!(line_of("(a)\n(b)\n", 4), 2);
    }

    #[test]
    fn an_offset_past_the_end_still_answers() {
        assert_eq!(line_of("(a)\n", 999), 2);
    }

    /// The envelope resolves each finding's line from that finding's own span,
    /// on a fixture with several lines.
    ///
    /// A one-line fixture cannot test this: every line there is 1, which is
    /// also what an unbuilt index or an unresolved field would report.
    #[test]
    fn the_envelope_resolves_each_findings_line_from_its_own_span() {
        let report = report(&[0, 4, 8]);
        let lines = report
            .findings
            .iter()
            .map(|finding| report.line_of(finding))
            .collect::<Vec<_>>();
        assert_eq!(lines, vec![1, 2, 3]);
    }

    /// The index answers exactly what counting newlines from byte 0 answered,
    /// at *every* offset in the file including one past the end.
    ///
    /// This is the whole no-output-change argument for moving the line into
    /// the envelope, so it is pinned rather than reasoned about: the two
    /// definitions of "line 1" cannot drift while this passes.
    #[test]
    fn the_index_agrees_with_counting_newlines_at_every_offset() {
        let report = report(&[]);
        for offset in 0..=SOURCE.len() {
            assert_eq!(
                report.lines.line_of(offset),
                line_of(SOURCE, offset),
                "disagreed at offset {offset}"
            );
        }
    }

    /// Narrowing for a gate keeps the line index, so a narrowed report still
    /// resolves lines rather than silently reporting line 1.
    #[test]
    fn a_narrowed_report_still_resolves_its_lines() {
        let report = report(&[0, 4, 8]).retained(|finding| finding.0 != 4);
        let lines = report
            .findings
            .iter()
            .map(|finding| report.line_of(finding))
            .collect::<Vec<_>>();
        assert_eq!(lines, vec![1, 3]);
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
