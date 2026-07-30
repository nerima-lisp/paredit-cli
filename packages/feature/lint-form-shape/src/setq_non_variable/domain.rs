//! Common Lisp setq-non-variable detection: a `setq` or `psetq` whose place is
//! not a plain variable symbol. Unlike `setf`, which assigns to a *place form*
//! (`(setf (car x) 5)`), `setq`/`psetq` require each place to be a symbol
//! naming a variable. A list place (`(setq (car x) 5)` — `setf` was meant), a
//! literal (`(setq 5 x)`), a constant (`(setq nil 1)`, `(setq :k 1)`), or a
//! quoted symbol (`(setq 'x 5)`) is a program error, caught at macroexpansion
//! rather than by the reader.
//!
//! Only the place positions (the even-indexed arguments) are inspected. Forms
//! whose place/value pairing is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted `setq`, and any form with a `#+`/`#-`
//! reader conditional or `,@` splice argument, which shifts the pairing.
//!
//! Complements [`crate::setf_arity::domain`] (which checks the argument
//! *count*) by checking each place's *validity*.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

const SETQ_HEADS: [&str; 2] = ["setq", "psetq"];

/// Whether an argument's reader prefix or `#+`/`#-` marker makes the static
/// place/value pairing unreliable.
fn is_pairing_ambiguous(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

fn is_number_literal(text: &str) -> bool {
    text.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
    }) && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

/// Whether a place form is not a valid `setq` variable: a quoted form, a list,
/// a literal (number/string/character), or a constant (`nil`, `t`, a keyword).
fn place_is_invalid(view: &ExpressionView) -> bool {
    // A quoted/quasiquoted/unquoted place (`'x`) is not a variable.
    if !view.reader_prefixes.is_empty() {
        return true;
    }
    let Some(text) = atom_text(view) else {
        // A list place — `setf` territory, not `setq`.
        return true;
    };
    if is_number_literal(text) || text.starts_with('"') || text.starts_with("#\\") {
        return true;
    }
    // A constant variable cannot be assigned: nil, t, or a keyword.
    text.eq_ignore_ascii_case("t")
        || text.eq_ignore_ascii_case("nil")
        || (text.len() > 1 && text.starts_with(':') && text.as_bytes()[1] != b':')
}

#[derive(Debug, Clone)]
pub struct SetqNonVariableItem {
    /// The span of the offending *place*, not of the whole assignment form.
    pub span: ByteSpan,
    /// The operator as it is spelled in the source, so its case survives.
    /// Data rather than a tag: `SETQ` and `setq` are the same operator but not
    /// the same string, which is why this is not the finding's `kind`.
    pub operator: String,
    pub place: String,
}

impl Finding for SetqNonVariableItem {
    /// Fixed: the operator is source-cased data rather than a closed set of
    /// tags, and there is only one class of finding here.
    fn kind(&self) -> &'static str {
        "setq-non-variable"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("place={}", self.place),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("place", json!(self.place)),
        ]
    }

    /// The same sentence the `setq-non-variable` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("{} place {} is not a variable", self.operator, self.place)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_setq(
    view: &ExpressionView,
    assignment_form_count: &mut usize,
    violations: &mut Vec<SetqNonVariableItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !SETQ_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted setq is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument shifts the place/value pairing.
    if view.children.iter().skip(1).any(is_pairing_ambiguous) {
        return;
    }
    *assignment_form_count += 1;

    // Places are the even-indexed arguments: children 1, 3, 5, ...
    for place in view.children.iter().skip(1).step_by(2) {
        if place_is_invalid(place) {
            violations.push(SetqNonVariableItem {
                span: place.span,
                operator: head.to_owned(),
                place: render_expression(place),
            });
        }
    }
}

/// Collects every `setq`/`psetq` place that is not a variable in one file, with
/// the number of `setq`/`psetq` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every place here is a variable" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_setq_non_variable_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SetqNonVariableItem>> {
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
            examine_setq(subview, &mut assignment_form_count, &mut violations);
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

    fn report(input: &str) -> FileFindings<SetqNonVariableItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_setq_non_variable_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build setq non variable report")
    }

    /// The `(assignment_form_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<SetqNonVariableItem>) {
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
    fn flags_a_list_place() {
        let (form_count, items) = violations("(setq (car x) 5)");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "setq");
        assert_eq!(items[0].place, "(car x)");
    }

    #[test]
    fn flags_a_literal_place() {
        let (_, items) = violations("(setq 5 x)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].place, "5");
    }

    #[test]
    fn flags_a_constant_place() {
        let (_, nil_place) = violations("(setq nil 1)");
        assert_eq!(nil_place.len(), 1);
        let (_, keyword_place) = violations("(setq :k 1)");
        assert_eq!(keyword_place.len(), 1);
    }

    #[test]
    fn flags_a_quoted_place() {
        let (_, items) = violations("(setq 'x 5)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_bad_place_among_good_ones() {
        let (_, items) = violations("(setq x 1 (car y) 2 z 3)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].place, "(car y)");
    }

    #[test]
    fn does_not_flag_ordinary_variables() {
        let (form_count, items) = violations("(setq x 1 y 2)");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_special_variable() {
        let (_, items) = violations("(setq *global* 1)");
        assert!(items.is_empty());
    }

    #[test]
    fn flags_a_psetq_place() {
        let (_, items) = violations("(psetq (aref v i) 1)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "psetq");
    }

    #[test]
    fn skips_a_reader_conditional_form() {
        let (form_count, items) = violations("(setq #+sbcl x #-sbcl y 5)");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_setq_form() {
        let (form_count, items) = violations("(list '(setq 5 x))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_setq_nested_in_a_function_body() {
        let (form_count, items) = violations("(defun f () (setq (car y) 1))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(setq 5 x)", Dialect::Clojure).expect("parse input");
        let report = build_setq_non_variable_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build setq non variable report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("assignment_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(setq x 1)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_place() {
        let report = report("(defun f (y)\n  (psetq (aref v i) 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "setq-non-variable");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("psetq")), ("place", json!("(aref v i)")),]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=psetq".to_owned(), "place=(aref v i)".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_setq_scanned_not_only_the_flagged_ones() {
        let report = report("(setq 5 x)\n(setq y 1)\n(setq (car z) 2)\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
