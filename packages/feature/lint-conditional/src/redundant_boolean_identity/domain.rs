//! Common Lisp redundant-boolean-identity detection: a boolean operator whose
//! operand list contains its *identity element*, which contributes nothing —
//! `t` in an `and` (`(and a t b)` is `(and a b)`) or `nil` in an `or`
//! (`(or a nil b)` is `(or a b)`). The identity operand is evaluated for no
//! effect and just clutters the form.
//!
//! This is the complement of [`crate::dead_boolean_operand::domain`],
//! which handles the *dominant* elements (`nil` in `and`, `t` in `or`) that make
//! later operands dead. Here the constant changes nothing and is simply dropped.
//!
//! The operators differ about the *last* operand:
//!
//!   - `or`: `nil` is removable at any position — an exhausted `or` yields `nil`
//!     anyway, so a trailing `nil` is redundant too.
//!   - `and`: `t` is removable only when it is *not* the last operand. A trailing
//!     `t` is `and`'s return value (`(and a t)` yields `t`, not `a`), so it must
//!     be kept.
//!
//! Only a bare `t`/`nil` symbol counts (a reader-prefixed atom is left alone),
//! and a single-operand form (`(and t)`, `(or nil)`) is left to
//! `single-operand-boolean`. The fix reconstructs `(op kept…)` from the surviving
//! operands' source, collapsing to the bare identity (`t`/`nil`) when every
//! operand was removed, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The canonical operator name (`and`/`or`) and its identity element (`t`/`nil`)
/// for a boolean head, or `None` otherwise.
fn boolean_identity(head: &str) -> Option<(&'static str, &'static str)> {
    if head.eq_ignore_ascii_case("and") {
        Some(("and", "t"))
    } else if head.eq_ignore_ascii_case("or") {
        Some(("or", "nil"))
    } else {
        None
    }
}

/// Whether `view` is the bare literal `identity` symbol (no reader prefixes).
fn is_identity_literal(view: &ExpressionView, identity: &str) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(identity))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantBooleanIdentityItem {
    /// The span of the whole `(and …)`/`(or …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator, lowercased (`and` or `or`).
    pub operator: &'static str,
    /// The removed identity element (`t` for `and`, `nil` for `or`).
    pub identity: &'static str,
    /// The spans of the operands to keep, in order (empty means the form
    /// collapses to the bare identity).
    ///
    /// The rewrite's input, not the report's: the lint rule slices them to
    /// rebuild the form, and neither the old renderer nor this one prints them.
    pub kept_spans: Vec<ByteSpan>,
}

impl Finding for RedundantBooleanIdentityItem {
    /// The operator, so an `and` finding and an `or` finding are separable
    /// without parsing JSON.
    ///
    /// It is a legitimate tag rather than data: `boolean_identity` normalises
    /// the head to one of two `&'static str`s, so `(AND …)` and `(and …)` land
    /// in the same bucket. The two are different cleanups — `and` drops a `t`,
    /// `or` drops a `nil` — and a consumer filtering on one is asking a real
    /// question.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("identity={}", self.identity),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("identity", json!(self.identity)),
        ]
    }

    /// The same sentence the `redundant-boolean-identity` lint rule writes, so
    /// a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        format!(
            "{} has a redundant {} operand; drop it",
            self.operator, self.identity
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    source: &str,
    boolean_form_count: &mut usize,
    violations: &mut Vec<RedundantBooleanIdentityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((operator, identity)) = boolean_identity(head) else {
        return;
    };
    *boolean_form_count += 1;

    let operands = &view.children[1..];
    // A single-operand form is single-operand-boolean's job; an empty form is
    // the bare identity already.
    if operands.len() < 2 {
        return;
    }
    if operands.iter().any(is_reader_conditional) {
        return;
    }

    let last_index = operands.len() - 1;
    let mut kept = Vec::new();
    let mut removed_any = false;
    for (index, operand) in operands.iter().enumerate() {
        // `t` in `and` is kept when it is the last operand (the return value);
        // `nil` in `or` is always removable.
        let removable =
            is_identity_literal(operand, identity) && (operator == "or" || index != last_index);
        if removable {
            removed_any = true;
        } else {
            kept.push(operand.span);
        }
    }

    if removed_any {
        violations.push(RedundantBooleanIdentityItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            operator,
            identity,
            kept_spans: kept,
        });
    }
}

/// Collects every boolean form with a redundant identity operand in one file,
/// with the number of `and`/`or` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant identity here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_boolean_identity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantBooleanIdentityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("boolean_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut boolean_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, source, &mut boolean_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("boolean_form_count", json!(boolean_form_count))],
    ))
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

    fn report(input: &str) -> FileFindings<RedundantBooleanIdentityItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_boolean_identity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant boolean identity report")
    }

    /// The `(boolean_form_count, violations)` pair the report is built from.
    fn booleans(input: &str) -> (u64, Vec<RedundantBooleanIdentityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "boolean_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("boolean_form_count in the summary");
        (count, report.findings)
    }

    fn kept<'a>(source: &'a str, item: &RedundantBooleanIdentityItem) -> Vec<&'a str> {
        item.kept_spans
            .iter()
            .map(|s| &source[s.start().get()..s.end().get()])
            .collect()
    }

    #[test]
    fn removes_middle_t_from_and() {
        let source = "(and a t b)";
        let (count, violations) = booleans(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
        assert_eq!(kept(source, &violations[0]), vec!["a", "b"]);
    }

    #[test]
    fn removes_nil_from_or_at_any_position() {
        let source = "(or a nil)";
        let (_, violations) = booleans(source);
        assert_eq!(kept(source, &violations[0]), vec!["a"]);

        let source2 = "(or nil a)";
        let (_, v2) = booleans(source2);
        assert_eq!(kept(source2, &v2[0]), vec!["a"]);
    }

    #[test]
    fn keeps_trailing_t_in_and() {
        // (and a t) yields t, not a — the trailing t is the return value.
        let (_, violations) = booleans("(and a t)");
        assert!(violations.is_empty());
    }

    #[test]
    fn or_with_all_nil_collapses_to_empty_kept() {
        let (_, violations) = booleans("(or nil nil)");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].kept_spans.is_empty());
    }

    #[test]
    fn does_not_flag_dominant_elements() {
        // nil in and and t in or are dead-boolean-operand's job, not this rule's.
        assert!(booleans("(and a nil b)").1.is_empty());
        assert!(booleans("(or a t b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_single_operand() {
        assert!(booleans("(and t)").1.is_empty());
        assert!(booleans("(or nil)").1.is_empty());
    }

    #[test]
    fn does_not_flag_without_identity_operand() {
        let (count, violations) = booleans("(and a b c)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_operator_and_identity() {
        let (_, violations) = booleans("(AND x T y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(kept("(AND x T y)", &violations[0]), vec!["x", "y"]);
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = booleans("(when (or ready nil) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(and a t b)", Dialect::Clojure).expect("parse");
        let report =
            build_redundant_boolean_identity_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build redundant boolean identity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("boolean_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(and a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_identity() {
        let report = report("(defun ok? (a b)\n  (and a t b))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "and");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("and")), ("identity", json!("t"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["operator=and".to_owned(), "identity=t".to_owned()]
        );
    }

    /// The operator is normalised before it is stored, so it survives as a tag
    /// whatever the source's casing.
    #[test]
    fn the_kind_is_case_normalised() {
        assert_eq!(report("(AND x T y)").findings[0].kind(), "and");
        assert_eq!(report("(OR x NIL y)").findings[0].kind(), "or");
    }

    #[test]
    fn the_summary_counts_every_boolean_scanned_not_only_the_flagged_ones() {
        let report = report("(and a t b)\n(and a b)\n(or c nil d)\n");
        assert_eq!(report.summary, vec![("boolean_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
