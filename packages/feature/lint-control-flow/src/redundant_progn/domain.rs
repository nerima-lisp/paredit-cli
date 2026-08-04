//! Common Lisp redundant-`progn` detection: a `(progn …)` whose body is a
//! single form (`(progn X)` ≡ `X`) or empty (`(progn)` ≡ `nil`). `progn`
//! establishes no binding or control context — it only evaluates its body forms
//! in order and returns the last value — so a one-form or zero-form progn is
//! pure wrapping noise, common in macro-expanded or machine-generated code.
//!
//! Only these two unambiguous shapes are flagged. A progn with two or more body
//! forms is meaningful (it sequences side effects) and is never flagged. A
//! reader conditional (`#+`/`#-`) as the sole body element is left alone: it may
//! expand to zero or one form depending on the build, so the static single-form
//! count does not reflect its evaluated arity.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct RedundantPrognItem {
    /// The span of the whole `(progn …)` form (what a fix would replace).
    pub span: ByteSpan,
    /// The number of body forms (0 for `(progn)`, 1 for `(progn X)`).
    pub body_form_count: usize,
    /// The span of the single body form, or `None` for an empty progn (whose
    /// meaning is `nil`). Lets a fix substitute the exact inner source text.
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// the replacement, and the command has never printed it.
    pub inner_span: Option<ByteSpan>,
}

impl Finding for RedundantPrognItem {
    /// The rule's own name. The two shapes this rule flags are told apart by
    /// `body_form_count`, which is a number and so cannot be the tag.
    fn kind(&self) -> &'static str {
        "redundant-progn"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("body_form_count={}", self.body_form_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("body_form_count", json!(self.body_form_count))]
    }

    /// The same sentence the `redundant-progn` lint rule writes, including its
    /// split between the empty and the single-form shape, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        let detail = if self.body_form_count == 0 {
            "an empty progn is nil"
        } else {
            "progn wraps a single form; it is equivalent to that form"
        };
        format!("redundant progn: {detail}")
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom in a progn body does not
/// represent one evaluated form. Mirrors the guard used by the arity lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// A `,@`-spliced body form, which is the same problem as a reader conditional
/// in the other direction: `` `(progn ,@forms) `` wraps however many forms
/// `forms` turns out to hold, which is zero or two just as easily as one.
///
/// It is also unwritable as a rewrite even when it *is* one form —
/// `` `,@forms `` is not a well-formed backquote expression and SBCL refuses to
/// read it — so this exemption removes a finding whose fix had no valid output.
fn is_spliced(view: &ExpressionView) -> bool {
    view.reader_prefixes
        .contains(&ReaderPrefix::UnquoteSplicing)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_progn(
    view: &ExpressionView,
    progn_form_count: &mut usize,
    violations: &mut Vec<RedundantPrognItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("progn"))
    {
        return;
    }
    *progn_form_count += 1;

    // children[0] is the `progn` head; the remainder is the body.
    let body = &view.children[1..];
    let body_form_count = body.len();

    match body {
        // (progn) — an empty body evaluates to nil.
        [] => violations.push(RedundantPrognItem {
            span: view.span,
            body_form_count,
            inner_span: None,
        }),
        // (progn X) — a single body form; the progn is a no-op wrapper. A lone
        // reader conditional is exempt (its evaluated arity is build-dependent),
        // and so is a lone `,@` splice (its arity is expansion-dependent).
        [only] if !is_reader_conditional(only) && !is_spliced(only) => {
            violations.push(RedundantPrognItem {
                span: view.span,
                body_form_count,
                inner_span: Some(only.span),
            });
        }
        _ => {}
    }
}

/// Collects every redundant progn (empty, or wrapping a single form) in one
/// file, with the number of progn forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant progn here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_progn_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantPrognItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("progn_form_count", json!(0))],
        ));
    }

    let mut progn_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_progn(subview, &mut progn_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("progn_form_count", json!(progn_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantPrognItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_progn_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant progn report")
    }

    /// The `(progn_form_count, violations)` pair the report is built from.
    fn progns(input: &str) -> (u64, Vec<RedundantPrognItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "progn_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("progn_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_progn_wrapping_a_single_form() {
        let (count, violations) = progns("(progn (foo))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 1);
        assert!(violations[0].inner_span.is_some());
    }

    #[test]
    fn flags_a_progn_wrapping_a_single_atom() {
        let (_, violations) = progns("(progn x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 1);
    }

    #[test]
    fn flags_an_empty_progn() {
        let (_, violations) = progns("(progn)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 0);
        assert!(violations[0].inner_span.is_none());
    }

    #[test]
    fn inner_span_covers_only_the_body_form() {
        let (_, violations) = progns("(progn (foo bar))");
        let inner = violations[0].inner_span.expect("inner span");
        // The recorded inner span isolates `(foo bar)`, not the whole progn.
        assert!(inner.start().get() > violations[0].span.start().get());
        assert!(inner.end().get() < violations[0].span.end().get());
    }

    #[test]
    fn does_not_flag_a_progn_with_two_forms() {
        let (count, violations) = progns("(progn (setup) (run))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_with_many_forms() {
        let (_, violations) = progns("(progn a b c d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = progns("(PROGN x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_redundant_progn() {
        // Both the outer (single body form) and inner progn are redundant.
        let (_, violations) = progns("(progn (progn x))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional_body() {
        // (progn #+sbcl x) statically has one body form, but the reader
        // conditional may vanish at read time, so the arity is build-dependent.
        let (_, violations) = progns("(progn #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_when_a_reader_conditional_precedes_a_real_form() {
        // With two body elements this is not the single-form shape at all.
        let (_, violations) = progns("(progn #+sbcl x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_prog1_or_prog2() {
        let (count, violations) = progns("(prog1 x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(progn x)", Dialect::Clojure).expect("parse");
        let report = build_redundant_progn_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant progn report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("progn_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(progn a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_body_form_count() {
        let report = report("(defun f ()\n  (progn (compute)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-progn");
        assert_eq!(finding.json_fields(), vec![("body_form_count", json!(1))]);
        assert_eq!(finding.text_columns(), vec!["body_form_count=1".to_owned()]);
    }

    /// The empty and single-form shapes share one `kind`, so the sentence has
    /// to be what tells them apart.
    #[test]
    fn the_message_distinguishes_the_empty_progn_from_the_single_form_one() {
        assert_eq!(
            report("(progn)").findings[0].message(),
            "redundant progn: an empty progn is nil"
        );
        assert_eq!(
            report("(progn x)").findings[0].message(),
            "redundant progn: progn wraps a single form; it is equivalent to that form"
        );
    }

    #[test]
    fn the_summary_counts_every_progn_scanned_not_only_the_flagged_ones() {
        let report = report("(progn x)\n(progn a b)\n(progn)\n");
        assert_eq!(report.summary, vec![("progn_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
