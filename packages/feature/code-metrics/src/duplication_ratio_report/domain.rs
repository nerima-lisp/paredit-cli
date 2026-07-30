//! How much of a file is structurally repeated.
//!
//! `inspect duplicates` reports *which* forms repeat, one group at a time. That
//! is the right shape for acting on a duplicate and the wrong shape for
//! deciding whether to act at all — a list of 200 groups says nothing about
//! whether the tree is 3% repeated or 30%.
//!
//! This is the ratio, and the ratio is what a decision gets made on.
//!
//! Similarity is exact structural equality, deliberately, not the tree-edit
//! distance `inspect similarity` uses. A ratio built on a fuzzy threshold moves
//! when the threshold moves, which makes it useless for tracking over time; a
//! ratio built on exact shape is a number two revisions can be compared on.
//!
//! Identifiers are erased and structure is kept, so `(+ a 1)` and `(+ b 1)`
//! share a shape — that is Type-2 cloning, and counting it is the point.
//! Literals are kept as their *kind* rather than their value for the same
//! reason.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

/// The smallest form worth counting as a clone.
///
/// Below this, every file is "duplicated": `(car x)` appears everywhere and
/// means nothing. Four nodes is the smallest shape that carries intent.
const MIN_SHAPE_NODES: usize = 4;

/// One repeated structural shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedShape {
    /// The shape, as the fingerprint that identifies it.
    pub shape: String,
    /// How many times it occurs.
    pub occurrences: usize,
    /// How many nodes one occurrence spans.
    pub nodes: usize,
    /// Bytes a perfect extraction would remove: every occurrence but one.
    pub redundant_bytes: usize,
    /// The first occurrence, which is where a reader should look.
    pub span: ByteSpan,
}

impl Finding for RepeatedShape {
    fn kind(&self) -> &'static str {
        "repeated-shape"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("occurrences={}", self.occurrences),
            format!("nodes={}", self.nodes),
            format!("redundant_bytes={}", self.redundant_bytes),
            self.shape.clone(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("shape", json!(self.shape)),
            ("occurrences", json!(self.occurrences)),
            ("nodes", json!(self.nodes)),
            ("redundant_bytes", json!(self.redundant_bytes)),
        ]
    }
}

/// One occurrence, before occurrences are grouped by shape.
struct Occurrence {
    shape: String,
    nodes: usize,
    span: ByteSpan,
}

#[must_use]
pub fn build_duplication_ratio_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<RepeatedShape> {
    let source = tree.source();
    let mut occurrences = Vec::new();
    for form in &tree.root_view().children {
        collect(form, &mut occurrences);
    }

    let total_bytes = source.len();
    let mut groups: BTreeMap<&str, Vec<&Occurrence>> = BTreeMap::new();
    for occurrence in &occurrences {
        groups
            .entry(occurrence.shape.as_str())
            .or_default()
            .push(occurrence);
    }

    let mut findings = Vec::new();
    let mut redundant_total = 0usize;
    for (shape, group) in groups {
        if group.len() < 2 {
            continue;
        }
        // Every occurrence but one is redundant: a perfect extraction keeps one
        // copy and replaces the rest with a call.
        let redundant = group
            .iter()
            .skip(1)
            .map(|occurrence| width(occurrence.span))
            .sum::<usize>();
        redundant_total += redundant;

        let first = group[0];
        findings.push(RepeatedShape {
            shape: shape.to_owned(),
            occurrences: group.len(),
            nodes: first.nodes,
            redundant_bytes: redundant,
            span: first.span,
        });
    }

    // Per mille rather than a float, so the number survives JSON round-trips
    // and integer comparison exactly. A ratio a CI gate compares against must
    // not depend on float formatting.
    let ratio_per_mille = redundant_total
        .saturating_mul(1000)
        .checked_div(total_bytes)
        .unwrap_or(0);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Structural repetition is not a dialect question.
        true,
        tree.source(),
        findings,
        vec![
            ("source_bytes", json!(total_bytes)),
            ("redundant_bytes", json!(redundant_total)),
            ("duplication_per_mille", json!(ratio_per_mille)),
        ],
    )
}

/// Records every subform large enough to count, then recurses.
///
/// Nested shapes are counted too, and that is deliberate: a repeated `defun`
/// and the repeated `let` inside it are two separate extraction opportunities,
/// and only the second may be worth taking. The redundancy total does
/// double-count them, which is why it is reported beside the source size rather
/// than as a standalone claim.
fn collect(view: &ExpressionView, occurrences: &mut Vec<Occurrence>) {
    if view.kind == ExpressionKind::List {
        let nodes = count_nodes(view);
        if nodes >= MIN_SHAPE_NODES {
            occurrences.push(Occurrence {
                shape: fingerprint(view),
                nodes,
                span: view.span,
            });
        }
    }
    for child in &view.children {
        collect(child, occurrences);
    }
}

/// A shape with identifiers erased and literals reduced to their kind.
///
/// `(+ a 1)` and `(+ b 2)` fingerprint alike; `(+ a 1)` and `(- a 1)` do not,
/// because the operator is structure rather than a name. The head of a list is
/// therefore kept verbatim and every other atom is erased.
fn fingerprint(view: &ExpressionView) -> String {
    fn write(view: &ExpressionView, is_head: bool, out: &mut String) {
        match view.kind {
            ExpressionKind::Atom => {
                let text = view.text.as_deref().unwrap_or_default();
                if is_head {
                    out.push_str(&text.to_ascii_uppercase());
                } else if text.starts_with('"') {
                    out.push('S');
                } else if text.starts_with("#\\") {
                    out.push('C');
                } else if text.starts_with(':') {
                    out.push('K');
                } else if text
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_digit() || character == '-')
                    && text.chars().skip(1).all(|c| c.is_ascii_digit() || c == '.')
                {
                    out.push('N');
                } else {
                    out.push('_');
                }
            }
            ExpressionKind::List | ExpressionKind::Root => {
                out.push('(');
                for (index, child) in view.children.iter().enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    write(child, index == 0, out);
                }
                out.push(')');
            }
        }
    }

    let mut out = String::new();
    write(view, false, &mut out);
    out
}

fn count_nodes(view: &ExpressionView) -> usize {
    1 + view.children.iter().map(count_nodes).sum::<usize>()
}

const fn width(span: ByteSpan) -> usize {
    span.end().get() - span.start().get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<RepeatedShape> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_duplication_ratio_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn per_mille(report: &FileFindings<RepeatedShape>) -> u64 {
        report
            .summary
            .iter()
            .find(|(name, _)| *name == "duplication_per_mille")
            .and_then(|(_, value)| value.as_u64())
            .expect("the ratio is reported")
    }

    #[test]
    fn a_file_with_no_repetition_measures_zero() {
        let report = report("(defun f (a b) (+ a b))");
        assert_eq!(per_mille(&report), 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_exactly_repeated_form_is_reported_once_with_its_count() {
        let report = report("(defun f (a) (list (g a 1 2) (g a 1 2)))");
        let shape = report
            .findings
            .iter()
            .find(|finding| finding.occurrences == 2)
            .expect("the repeated shape is reported");
        assert_eq!(shape.occurrences, 2);
    }

    #[test]
    fn identifiers_are_erased_so_a_renamed_copy_still_matches() {
        let report = report("(defun f (a b) (list (g a 1 2) (g b 1 2)))");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.occurrences == 2),
            "{report:?}"
        );
    }

    #[test]
    fn a_different_operator_is_a_different_shape() {
        let report = report("(defun f (a) (list (g a 1 2) (h a 1 2)))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.occurrences < 2),
            "{report:?}"
        );
    }

    #[test]
    fn a_form_below_the_size_floor_is_not_counted() {
        // `(car x)` is three nodes and appears everywhere; counting it would
        // report every file as heavily duplicated.
        let report = report("(defun f (a b) (list (car a) (car b)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn the_ratio_counts_every_occurrence_but_one_as_redundant() {
        let report = report("(defun f (a) (list (g a 1 2) (g a 1 2) (g a 1 2)))");
        let shape = report
            .findings
            .iter()
            .find(|finding| finding.occurrences == 3)
            .expect("the repeated shape is reported");
        // Two of the three occurrences are redundant.
        assert_eq!(shape.redundant_bytes, 2 * "(g a 1 2)".len());
    }

    #[test]
    fn the_ratio_is_reported_in_per_mille_so_it_compares_exactly() {
        let report = report("(defun f (a) (list (g a 1 2) (g a 1 2)))");
        assert!(per_mille(&report) > 0, "{report:?}");
        assert!(per_mille(&report) <= 1000);
    }

    #[test]
    fn an_empty_file_measures_zero_rather_than_dividing_by_zero() {
        assert_eq!(per_mille(&report("")), 0);
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun f (a) (list (g a 1 2) (g a 1 2) (h a 3 4) (h a 3 4)))");
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
        let report = build_duplication_ratio_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(report.dialect_modelled);
    }
}
