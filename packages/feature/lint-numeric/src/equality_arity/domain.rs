//! Common Lisp equality-predicate-arity detection: an `eq`, `eql`, `equal`, or
//! `equalp` call with the wrong number of arguments. All four are strictly
//! binary — `(eq object-1 object-2)` — so `(eq x)` (too few) or `(eql a b c)`
//! (too many) is a program error, caught at compile time rather than by the
//! reader.
//!
//! Scoped to these four general equality predicates on purpose: the numeric and
//! character/string comparisons (`=`, `<`, `char=`, `string=`, …) are variadic
//! (`(< a b c)` chains, `(= a)` is vacuously true), so they are not checked
//! here.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted call (data or a template),
//! and any call with a `#+`/`#-` reader conditional or a splicing unquote
//! (`,@`) argument — e.g. `(eq x #+sbcl y #-sbcl z)` is a valid feature-portable
//! comparison whose written three-token shape is not a real arity error.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

const EQUALITY_HEADS: [&str; 4] = ["eq", "eql", "equal", "equalp"];
const EXPECTED_ARGUMENTS: usize = 2;

/// Whether a child form can change how many argument forms actually reach the
/// evaluator, making a static arity count unreliable: a `,@` splice or Clojure
/// reader conditional (a prefix), or a Common Lisp `#+`/`#-` conditional (an
/// atom whose text begins `#+`/`#-`).
fn is_arity_ambiguous(view: &ExpressionView) -> bool {
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

#[derive(Debug, Clone)]
pub struct EqualityArityItem {
    pub span: ByteSpan,
    /// The operator exactly as it was written, so its source casing survives.
    pub operator: String,
    pub argument_count: usize,
}

impl Finding for EqualityArityItem {
    /// Which of the four equality predicates was miscalled, normalized to the
    /// spelling this rule matched on.
    ///
    /// A consumer chasing a misarity call cares which predicate it is — the
    /// four are separate functions with separate call sites — and can select
    /// one without parsing JSON. `operator` keeps the source casing that this
    /// discards.
    fn kind(&self) -> &'static str {
        EQUALITY_HEADS
            .iter()
            .find(|name| self.operator.eq_ignore_ascii_case(name))
            .copied()
            .unwrap_or("equality-arity")
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("arguments={}", self.argument_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("argument_count", json!(self.argument_count)),
        ]
    }

    /// The same sentence the `equality-arity` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} takes exactly {EXPECTED_ARGUMENTS} arguments but has {}",
            self.operator, self.argument_count
        )
    }
}

pub fn examine_call(
    view: &ExpressionView,
    call_count: &mut usize,
    violations: &mut Vec<EqualityArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EQUALITY_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted/unquoted call is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument makes the written arity unreliable.
    if view.children.iter().skip(1).any(is_arity_ambiguous) {
        return;
    }
    *call_count += 1;

    let argument_count = view.children.len() - 1;
    if argument_count != EXPECTED_ARGUMENTS {
        violations.push(EqualityArityItem {
            span: view.span,
            operator: head.to_owned(),
            argument_count,
        });
    }
}

/// Collects every misarity equality-predicate call in one file, with the number
/// of such calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every call is binary" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_equality_arity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EqualityArityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("call_count", json!(0))],
        ));
    }

    let mut call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, &mut call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("call_count", json!(call_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EqualityArityItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_equality_arity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build equality arity report")
    }

    /// The `(call_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<EqualityArityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_eq_with_too_few_arguments() {
        let (call_count, items) = violations("(eq x)");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "eq");
        assert_eq!(items[0].argument_count, 1);
    }

    #[test]
    fn flags_eql_with_too_many_arguments() {
        let (_, items) = violations("(eql a b c)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "eql");
        assert_eq!(items[0].argument_count, 3);
    }

    #[test]
    fn flags_equal_and_equalp() {
        let (_, one) = violations("(equal a)");
        assert_eq!(one.len(), 1);
        let (_, two) = violations("(equalp a b c d)");
        assert_eq!(two.len(), 1);
    }

    #[test]
    fn does_not_flag_a_binary_call() {
        let (call_count, items) = violations("(eq a b)");
        assert_eq!(call_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_variadic_numeric_comparisons() {
        let (call_count, items) = violations("(= a b c) (< x)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_argument() {
        let (call_count, items) = violations("(eq x #+sbcl y #-sbcl z)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_call() {
        let (call_count, items) = violations("(list '(eq x))");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_call_nested_in_a_function_body() {
        let (call_count, items) = violations("(defun f (x) (when (eq x) 1))");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(eq x)", Dialect::Clojure).expect("parse input");
        let report = build_equality_arity_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build equality arity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eq a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_arity() {
        let report = report("(defun f (x)\n  (eq x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eq");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("eq")), ("argument_count", json!(1))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=eq".to_owned(), "arguments=1".to_owned()]
        );
        assert_eq!(finding.message(), "eq takes exactly 2 arguments but has 1");
    }

    /// The `kind` normalizes; the `operator` field does not, so the source
    /// spelling is still recoverable from the report.
    #[test]
    fn a_shouted_operator_normalizes_only_in_the_kind() {
        let (_, items) = violations("(EQUALP a)");
        assert_eq!(items[0].kind(), "equalp");
        assert_eq!(items[0].operator, "EQUALP");
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(eq x)\n(eq a b)\n");
        assert_eq!(report.summary, vec![("call_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
