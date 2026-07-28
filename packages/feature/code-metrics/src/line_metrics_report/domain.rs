//! Line length, file length, and lines per definition.
//!
//! The shallowest report in the tool, and the only one that answers "is this
//! file too big to work with" — a question that decides whether a refactor is
//! worth starting, and which `complexity` deliberately does not answer.
//! `complexity` measures nesting depth and form counts, which is about how hard
//! a definition is to *reason* about; this is about how hard a file is to
//! *navigate*, and the two diverge: a thousand lines of flat `defparameter` is
//! trivial to reason about and miserable to work in.
//!
//! Thresholds are arguments rather than constants. There is no defensible
//! universal line length, and a report that hardcoded one would be reporting a
//! preference as a defect. The defaults below are the ones the Common Lisp
//! community converged on, and every one can be overridden.
//!
//! Counting is by *character*, not by byte. A file of Japanese comments is not
//! twice as wide as it looks, and a byte count would say it is.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// The thresholds a run measures against.
#[derive(Debug, Clone, Copy)]
pub struct LineThresholds {
    pub max_line_length: usize,
    pub max_file_lines: usize,
    pub max_definition_lines: usize,
}

impl Default for LineThresholds {
    /// The conventional Common Lisp defaults: 100 columns, 1000 lines a file,
    /// 50 lines a definition. Every one is a starting point, not a rule.
    fn default() -> Self {
        Self {
            max_line_length: 100,
            max_file_lines: 1000,
            max_definition_lines: 50,
        }
    }
}

/// What exceeded its threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    LongLine,
    LongFile,
    LongDefinition,
}

impl Overflow {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LongLine => "long-line",
            Self::LongFile => "long-file",
            Self::LongDefinition => "long-definition",
        }
    }
}

/// One measurement that exceeded its threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineFinding {
    pub overflow: Overflow,
    /// The definition's name, for a long definition.
    pub subject: Option<String>,
    pub measured: usize,
    pub threshold: usize,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for LineFinding {
    fn kind(&self) -> &'static str {
        self.overflow.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.subject.clone().unwrap_or_else(|| "-".to_owned()),
            format!("measured={}", self.measured),
            format!("threshold={}", self.threshold),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("overflow", json!(self.overflow.label())),
            ("subject", json!(self.subject)),
            ("measured", json!(self.measured)),
            ("threshold", json!(self.threshold)),
        ]
    }
}

#[must_use]
pub fn build_line_metrics_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    thresholds: LineThresholds,
) -> FileFindings<LineFinding> {
    let source = tree.source();
    let mut findings = Vec::new();

    let mut offset = 0usize;
    let mut total_lines = 0usize;
    let mut longest = 0usize;
    for (index, line) in source.lines().enumerate() {
        total_lines = index + 1;
        // Characters, not bytes: a file of Japanese comments is not twice as
        // wide as it looks.
        let width = line.chars().count();
        longest = longest.max(width);
        if width > thresholds.max_line_length {
            findings.push(LineFinding {
                overflow: Overflow::LongLine,
                subject: None,
                measured: width,
                threshold: thresholds.max_line_length,
                span: span_at(offset, offset + line.len()),
                line: index + 1,
            });
        }
        // `lines()` strips the terminator, so it is added back to keep the
        // running offset aligned with the source. A file whose last line has no
        // terminator overshoots by one at the very end, which no later finding
        // reads.
        offset += line.len() + 1;
    }

    if total_lines > thresholds.max_file_lines {
        findings.push(LineFinding {
            overflow: Overflow::LongFile,
            subject: None,
            measured: total_lines,
            threshold: thresholds.max_file_lines,
            span: span_at(0, source.len()),
            line: 1,
        });
    }

    let mut definition_count = 0usize;
    for form in &tree.root_view().children {
        let Some(head) = list_head(form) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, form, head) else {
            continue;
        };
        definition_count += 1;
        let start = line_of(source, form.span.start().get());
        let end = line_of(source, form.span.end().get().saturating_sub(1));
        let height = end.saturating_sub(start) + 1;
        if height > thresholds.max_definition_lines {
            findings.push(LineFinding {
                overflow: Overflow::LongDefinition,
                subject: shape.name(form).map(ToOwned::to_owned),
                measured: height,
                threshold: thresholds.max_definition_lines,
                span: form.span,
                line: start,
            });
        }
    }

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Line shape is not a dialect question; every parsed dialect is
        // measured the same way.
        true,
        findings,
        vec![
            ("total_lines", json!(total_lines)),
            ("longest_line", json!(longest)),
            ("definition_count", json!(definition_count)),
        ],
    )
}

const fn span_at(start: usize, end: usize) -> ByteSpan {
    ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str, thresholds: LineThresholds) -> FileFindings<LineFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_line_metrics_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree, thresholds)
    }

    fn tight() -> LineThresholds {
        LineThresholds {
            max_line_length: 20,
            max_file_lines: 3,
            max_definition_lines: 2,
        }
    }

    #[test]
    fn a_line_over_the_threshold_is_reported_with_both_numbers() {
        let report = report("(defun f () \"aaaaaaaaaaaaaaaaaaaaaaa\")", tight());
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.overflow == Overflow::LongLine)
            .expect("a long line is reported");
        assert_eq!(finding.threshold, 20);
        assert!(finding.measured > 20);
    }

    #[test]
    fn a_line_within_the_threshold_is_not_reported() {
        let report = report("(defun f () 1)", LineThresholds::default());
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_file_over_the_line_threshold_is_reported_once() {
        let report = report("(a)\n(b)\n(c)\n(d)\n(e)\n", tight());
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.overflow == Overflow::LongFile)
                .count(),
            1
        );
    }

    #[test]
    fn a_definition_over_the_line_threshold_names_itself() {
        let report = report("(defun render (x)\n  (a x)\n  (b x)\n  (c x))\n", tight());
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.overflow == Overflow::LongDefinition)
            .expect("a long definition is reported");
        assert_eq!(finding.subject.as_deref(), Some("render"));
        assert_eq!(finding.measured, 4);
    }

    #[test]
    fn width_is_counted_in_characters_rather_than_bytes() {
        // Ten multi-byte characters are ten columns wide, not thirty.
        let source = "; ややこしい状況\n";
        let report = report(
            source,
            LineThresholds {
                max_line_length: 20,
                ..LineThresholds::default()
            },
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn the_summary_reports_the_measurements_even_with_no_findings() {
        let report = report(
            "(defun f () 1)\n(defun g () 2)\n",
            LineThresholds::default(),
        );
        assert_eq!(
            report.summary,
            vec![
                ("total_lines", json!(2)),
                ("longest_line", json!(14)),
                ("definition_count", json!(2)),
            ]
        );
    }

    #[test]
    fn an_empty_file_measures_zero_rather_than_panicking() {
        let report = report("", LineThresholds::default());
        assert_eq!(report.summary[0], ("total_lines", json!(0)));
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(a)\n(bbbbbbbbbbbbbbbbbbbbbbbbb)\n(c)\n(d)\n", tight());
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn the_report_answers_for_every_dialect() {
        let tree =
            SyntaxTree::parse_with_dialect("(defn f [x] x)", Dialect::Clojure).expect("parse");
        let report = build_line_metrics_report(
            Path::new("t.clj"),
            Dialect::Clojure,
            &tree,
            LineThresholds::default(),
        );
        assert!(report.dialect_modelled);
    }
}
