//! Common Lisp duplicate-setf-place detection: a `setq`/`psetq`/`setf`/`psetf`
//! that assigns the same *variable* more than once in a single form —
//! `(setf a 1 a 2)`, `(setq x 1 y 2 x 3)`. For `setq`/`setf` the earlier
//! assignment's value is computed and then immediately overwritten (dead work,
//! and its net effect lost); for the parallel `psetq`/`psetf` assigning the same
//! place twice has undefined consequences. Either way a repeated place is almost
//! always a copy-paste slip or a typo'd variable name.
//!
//! Only *symbol* places are compared, by exact name. A compound `setf` place
//! (`(aref a i)`) is not compared — detecting an accidental duplicate there
//! needs full structural equality and is far rarer — and a trailing unpaired
//! argument (a malformed, odd-arity form) is left to `setf-arity`.
//!
//!
//! Scope: Common Lisp only.

use std::collections::HashSet;
use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

const ASSIGNMENT_HEADS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

/// The bare variable name of a symbol place (no reader prefixes), or `None` for
/// a compound place or prefixed atom.
fn symbol_place(view: &ExpressionView) -> Option<&str> {
    view.reader_prefixes
        .is_empty()
        .then(|| atom_text(view))
        .flatten()
}

#[derive(Debug, Clone)]
pub struct DuplicateSetfPlaceItem {
    /// The span of the whole assignment form.
    pub span: ByteSpan,
    /// The operator, lowercased (`setf`/`setq`/`psetf`/`psetq`).
    pub operator: String,
    /// The variable name assigned more than once.
    pub place: String,
}

impl Finding for DuplicateSetfPlaceItem {
    /// The rule's name, not the operator: `operator` is source text lowercased
    /// per finding, and `kind` is a fixed vocabulary the interop formats turn
    /// into a rule id. It stays a `json_fields` entry, where filtering on it
    /// still works.
    fn kind(&self) -> &'static str {
        "duplicate-setf-places"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("place={}", self.place),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("place", json!(self.place)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} assigns variable {} more than once; the earlier assignment is dead",
            self.operator, self.place
        )
    }
}

pub fn examine_assignment(
    view: &ExpressionView,
    assignment_form_count: &mut usize,
    violations: &mut Vec<DuplicateSetfPlaceItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !ASSIGNMENT_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *assignment_form_count += 1;

    // Arguments are place/value pairs after the operator; report each variable
    // that is assigned a second time (once per form per duplicated name).
    let mut seen: HashSet<String> = HashSet::new();
    let mut reported: HashSet<String> = HashSet::new();
    let mut pair = view.children.iter().skip(1);
    while let (Some(place), Some(_value)) = (pair.next(), pair.next()) {
        let Some(name) = symbol_place(place) else {
            continue;
        };
        let key = name.to_ascii_lowercase();
        if !seen.insert(key.clone()) && reported.insert(key) {
            violations.push(DuplicateSetfPlaceItem {
                span: view.span,
                operator: head.to_ascii_lowercase(),
                place: name.to_owned(),
            });
        }
    }
}

/// Collects every duplicated symbol place in one file, with the number of
/// assignment forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_duplicate_setf_place_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateSetfPlaceItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("assignment_form_count", json!(0))],
        ));
    }

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, &mut assignment_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("assignment_form_count", json!(assignment_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateSetfPlaceItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_duplicate_setf_place_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate setf place report")
    }

    fn places(input: &str) -> (u64, Vec<DuplicateSetfPlaceItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "assignment_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("assignment_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_repeated_setf_place() {
        let (count, violations) = places("(setf a 1 a 2)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].place, "a");
        assert_eq!(violations[0].operator, "setf");
    }

    #[test]
    fn flags_repeat_among_other_places() {
        let (_, violations) = places("(setq x 1 y 2 x 3)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].place, "x");
    }

    #[test]
    fn reports_each_duplicated_name_once() {
        // a and b both repeat; each reported a single time.
        let (_, violations) = places("(setf a 1 b 2 a 3 b 4)");
        assert_eq!(violations.len(), 2);
        let names: Vec<&str> = violations.iter().map(|v| v.place.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn does_not_flag_distinct_places() {
        let (count, violations) = places("(setf a 1 b 2 c 3)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_pair() {
        let (_, violations) = places("(setf a 1)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_compound_place() {
        // Compound places are not compared.
        let (_, violations) = places("(setf (aref v i) 1 (aref v i) 2)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_place_names() {
        // CL symbols are case-insensitive; A and a are the same variable.
        let (_, violations) = places("(setf A 1 a 2)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_psetq() {
        let (_, violations) = places("(psetq n 1 n 2)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "psetq");
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = places("(let ((a 1) (a 2)) a)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_duplicate() {
        let (_, violations) = places("(defun f () (setf total 0 total 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(setf a 1 a 2)", Dialect::Clojure).expect("parse");
        let report =
            build_duplicate_setf_place_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build duplicate setf place report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("assignment_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(setf a 1 b 2)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_place() {
        let report = report("(defun reset ()\n  (setf total 0 total 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "duplicate-setf-places");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("setf")), ("place", json!("total"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["operator=setf".to_owned(), "place=total".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_assignment_scanned_not_only_the_flagged_ones() {
        let report = report("(setf a 1 a 2)\n(setq b 1)\n(psetq c 1 d 2)\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
