//! One number per file, with every input that produced it shown.
//!
//! A score nobody can argue with is a score nobody will act on. So this reports
//! the weighted contributions alongside the total: a file scoring 40 because it
//! is one enormous definition needs different work from one scoring 40 because
//! it has forty undocumented exports, and a bare number cannot tell them apart.
//!
//! The weights are opinions, stated here rather than buried. They are ordered
//! by how expensive the problem is to live with, not by how easy it is to
//! measure:
//!
//! - **Deep nesting** costs the most. It is the one input that makes every
//!   *other* problem harder to fix.
//! - **Oversized definitions** next: they resist every automated refactor,
//!   because there is no small piece to move.
//! - **Missing documentation** and **parked work** are real and cheap to
//!   discharge, so they are weighted low. A file full of `TODO` is not
//!   necessarily in trouble; it may just be honest.
//!
//! The total is uncapped on purpose. A capped score compresses exactly the
//! files that most need distinguishing.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// What a debt input is worth, and where the threshold for charging it sits.
#[derive(Debug, Clone, Copy)]
struct Weight {
    name: &'static str,
    /// Points charged per unit over the threshold.
    points: usize,
    threshold: usize,
}

/// Deep nesting first, because it is what makes the others hard to fix.
const WEIGHTS: [Weight; 4] = [
    Weight {
        name: "nesting-depth",
        points: 4,
        threshold: 6,
    },
    Weight {
        name: "definition-lines",
        points: 2,
        threshold: 50,
    },
    Weight {
        name: "missing-docstring",
        points: 1,
        threshold: 0,
    },
    Weight {
        name: "parked-work",
        points: 1,
        threshold: 0,
    },
];

/// One definition's contribution to the file's score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtFinding {
    pub name: String,
    /// Each input that charged points, as `input=points`.
    pub contributions: Vec<String>,
    pub score: usize,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for DebtFinding {
    fn kind(&self) -> &'static str {
        "debt"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            format!("score={}", self.score),
            format!("from=({})", self.contributions.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("score", json!(self.score)),
            ("contributions", json!(self.contributions)),
        ]
    }
}

#[must_use]
pub fn build_debt_score_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<DebtFinding> {
    let source = tree.source();
    let parked = parked_work(tree, source);

    let mut findings = Vec::new();
    for form in &tree.root_view().children {
        let Some(head) = list_head(form) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, form, head) else {
            continue;
        };
        let Some(name) = shape.name(form) else {
            continue;
        };

        let depth = max_depth(form);
        let lines = line_of(source, form.span.end().get().saturating_sub(1))
            .saturating_sub(line_of(source, form.span.start().get()))
            + 1;
        // Only where a docstring can be written. `in-package` and `defsystem`
        // have no docstring position, so charging them would be charging for a
        // language feature that does not exist.
        let undocumented = usize::from(
            carries_docstring(shape.category)
                && shape
                    .body_forms(form)
                    .first()
                    .is_none_or(|first| !is_string_literal(first)),
        );
        let markers = parked
            .iter()
            .filter(|offset| {
                form.span.start().get() <= **offset && **offset < form.span.end().get()
            })
            .count();

        let measured = [depth, lines, undocumented, markers];
        let mut contributions = Vec::new();
        let mut score = 0usize;
        for (weight, value) in WEIGHTS.iter().zip(measured) {
            let over = value.saturating_sub(weight.threshold);
            if over == 0 {
                continue;
            }
            let charged = over.saturating_mul(weight.points);
            score += charged;
            contributions.push(format!("{}={charged}", weight.name));
        }

        if score == 0 {
            continue;
        }
        findings.push(DebtFinding {
            name: name.to_owned(),
            contributions,
            score,
            span: form.span,
            line: line_of(source, form.span.start().get()),
        });
    }

    let total: usize = findings.iter().map(|finding| finding.score).sum();
    let worst = findings
        .iter()
        .map(|finding| finding.score)
        .max()
        .unwrap_or(0);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Every input is a shape or comment question, not a dialect one.
        true,
        findings,
        vec![
            ("debt_score", json!(total)),
            ("worst_definition_score", json!(worst)),
        ],
    )
}

/// The offset of every task marker in the file.
fn parked_work(tree: &SyntaxTree, source: &str) -> Vec<usize> {
    tree.comments()
        .filter(|comment| {
            let span = comment.span();
            source
                .get(span.start().get()..span.end().get())
                .is_some_and(|text| {
                    let body = text.trim_start_matches(';').trim_start();
                    ["TODO", "FIXME", "XXX", "HACK", "BUG"]
                        .iter()
                        .any(|marker| {
                            body.get(..marker.len())
                                .is_some_and(|head| head.eq_ignore_ascii_case(marker))
                        })
                })
        })
        .map(|comment| comment.span().start().get())
        .collect()
}

/// Whether this kind of definition has a docstring position at all.
const fn carries_docstring(category: DefinitionCategory) -> bool {
    matches!(
        category,
        DefinitionCategory::Function
            | DefinitionCategory::Macro
            | DefinitionCategory::GenericFunction
            | DefinitionCategory::Method
            | DefinitionCategory::Variable
            | DefinitionCategory::Constant
            | DefinitionCategory::Parameter
            | DefinitionCategory::Class
            | DefinitionCategory::Struct
            | DefinitionCategory::Condition
    )
}

fn max_depth(view: &ExpressionView) -> usize {
    view.children
        .iter()
        .map(|child| 1 + max_depth(child))
        .max()
        .unwrap_or(0)
}

fn is_string_literal(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('"'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<DebtFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_debt_score_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn total(report: &FileFindings<DebtFinding>) -> u64 {
        report
            .summary
            .iter()
            .find(|(name, _)| *name == "debt_score")
            .and_then(|(_, value)| value.as_u64())
            .expect("the score is reported")
    }

    #[test]
    fn a_documented_shallow_definition_owes_nothing() {
        let report = report("(defun f (x) \"Returns X.\" x)");
        assert_eq!(total(&report), 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_missing_docstring_charges_the_lowest_weight() {
        let report = report("(defun f (x) x)");
        assert_eq!(total(&report), 1);
        assert_eq!(
            report.findings[0].contributions,
            vec!["missing-docstring=1".to_owned()]
        );
    }

    #[test]
    fn a_task_marker_inside_a_definition_charges_it() {
        let report = report("(defun f (x) \"Doc.\"\n  ;; TODO: fix\n  x)");
        assert_eq!(
            report.findings[0].contributions,
            vec!["parked-work=1".to_owned()]
        );
    }

    #[test]
    fn deep_nesting_charges_the_highest_weight() {
        let deep = "(defun f (x) \"Doc.\" (a (b (c (d (e (g (h x))))))))";
        let report = report(deep);
        assert!(
            report.findings[0]
                .contributions
                .iter()
                .any(|entry| entry.starts_with("nesting-depth=")),
            "{report:?}"
        );
    }

    #[test]
    fn nesting_below_the_threshold_charges_nothing() {
        let report = report("(defun f (x) \"Doc.\" (a (b x)))");
        assert_eq!(total(&report), 0);
    }

    #[test]
    fn every_charged_input_is_shown_beside_the_total() {
        let report = report("(defun f (x)\n  ;; TODO: fix\n  x)");
        let finding = &report.findings[0];
        assert_eq!(finding.score, 2);
        assert_eq!(
            finding.contributions,
            vec!["missing-docstring=1".to_owned(), "parked-work=1".to_owned()]
        );
    }

    #[test]
    fn a_definition_owing_nothing_is_not_listed() {
        let report = report("(defun clean (x) \"Doc.\" x)\n(defun dirty (x) x)");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].name, "dirty");
    }

    #[test]
    fn the_worst_definition_is_reported_beside_the_file_total() {
        let report = report("(defun a (x) x)\n(defun b (x)\n  ;; TODO: fix\n  x)");
        assert_eq!(total(&report), 3);
        assert_eq!(report.summary[1], ("worst_definition_score", json!(2)));
    }

    #[test]
    fn a_form_with_no_docstring_position_is_not_charged_for_lacking_one() {
        assert_eq!(total(&report("(in-package :app)")), 0);
    }

    #[test]
    fn a_file_with_no_definition_owes_nothing() {
        assert_eq!(total(&report("(print 1)")), 0);
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun a (x) x)\n(defun b (x) x)\n(defun c (x) x)");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }
}
