//! Common Lisp accessor-arity detection: a standard element or keyed-lookup
//! accessor called with the wrong number of arguments. The element accessors
//! `nth`, `elt`, `nthcdr`, `svref`, `char`, and `schar` take exactly two
//! arguments; the keyed lookups `gethash` and `getf` take two or three (with
//! an optional default). A wrong count — `(gethash key)` (a very common typo
//! that forgets the table), `(nth n)`, or `(elt seq idx extra)` — is a program
//! error, caught at compile time rather than by the reader.
//!
//! Like the other arity lints, this assumes the standard bindings: a local
//! `flet`/`macrolet` that shadows one of these names with a different arity is
//! rare and unsupported.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted call (data or a template),
//! and any call with a `#+`/`#-` reader conditional or a splicing unquote
//! (`,@`) argument.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The inclusive `(min, max)` argument arity of a standard accessor, or `None`
/// if `head` is not one this rule checks.
fn expected_arity(head: &str) -> Option<(usize, usize)> {
    match head.to_ascii_lowercase().as_str() {
        "nth" | "elt" | "nthcdr" | "svref" | "char" | "schar" => Some((2, 2)),
        "gethash" | "getf" => Some((2, 3)),
        _ => None,
    }
}

/// Whether an argument's reader prefix or `#+`/`#-` marker makes the static
/// argument count unreliable.
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

fn arity_phrase(min: usize, max: usize) -> String {
    if min == max {
        format!("exactly {min}")
    } else {
        format!("{min} or {max}")
    }
}

#[derive(Debug, Clone)]
pub struct AccessorArityItem {
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The accessor as it is written in the source, so a report reproduces the
    /// spelling the reader will find at that span.
    pub operator: String,
    pub argument_count: usize,
    pub min_arity: usize,
    pub max_arity: usize,
}

impl Finding for AccessorArityItem {
    /// The rule's name, not the accessor.
    ///
    /// `operator` is the source spelling — `gethash`, `GETHASH` — so it is a
    /// per-finding `String` and cannot be a `&'static str` kind. It stays a
    /// JSON field and a text column, where a consumer can still filter on it.
    fn kind(&self) -> &'static str {
        "accessor-arity"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("expected={}", expected_arity_phrase(self)),
            format!("arguments={}", self.argument_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("argument_count", json!(self.argument_count)),
            ("min_arity", json!(self.min_arity)),
            ("max_arity", json!(self.max_arity)),
            ("expected", json!(expected_arity_phrase(self))),
        ]
    }

    /// The same sentence the `accessor-arity` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} takes {} argument(s) but has {}",
            self.operator,
            expected_arity_phrase(self),
            self.argument_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    source: &str,
    call_count: &mut usize,
    violations: &mut Vec<AccessorArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((min_arity, max_arity)) = expected_arity(head) else {
        return;
    };
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
    if !(min_arity..=max_arity).contains(&argument_count) {
        violations.push(AccessorArityItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            operator: head.to_owned(),
            argument_count,
            min_arity,
            max_arity,
        });
    }
}

/// Collects every misarity accessor call in one file, with the number of
/// accessor calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every accessor call here is well-formed"
/// for Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn collect_accessor_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AccessorArityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("call_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, source, &mut call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("call_count", json!(call_count))],
    ))
}

/// A human phrase for the expected arity of one violation, e.g. `exactly 2`.
#[must_use]
pub fn expected_arity_phrase(item: &AccessorArityItem) -> String {
    arity_phrase(item.min_arity, item.max_arity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AccessorArityItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_accessor_arity_violations(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect accessor arity violations")
    }

    /// The `(call_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<AccessorArityItem>) {
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
    fn flags_gethash_missing_the_table() {
        let (call_count, items) = violations("(gethash key)");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "gethash");
        assert_eq!(items[0].argument_count, 1);
        assert_eq!(expected_arity_phrase(&items[0]), "2 or 3");
    }

    #[test]
    fn does_not_flag_gethash_with_a_default() {
        let (_, items) = violations("(gethash key table 0)");
        assert!(items.is_empty());
    }

    #[test]
    fn flags_gethash_with_too_many_arguments() {
        let (_, items) = violations("(gethash key table default extra)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 4);
    }

    #[test]
    fn flags_nth_missing_the_list() {
        let (_, items) = violations("(nth n)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "nth");
        assert_eq!(expected_arity_phrase(&items[0]), "exactly 2");
    }

    #[test]
    fn does_not_flag_valid_binary_accessors() {
        let (call_count, items) = violations("(nth n items) (elt seq 0) (svref v i) (char s 0)");
        assert_eq!(call_count, 4);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_getf_missing_the_indicator() {
        let (_, items) = violations("(getf plist)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "getf");
    }

    #[test]
    fn skips_a_reader_conditional_argument() {
        let (call_count, items) = violations("(gethash key #+sbcl a #-sbcl b)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_call() {
        let (call_count, items) = violations("(list '(nth n))");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_call_nested_in_a_function_body() {
        let (call_count, items) = violations("(defun f (k h) (when (gethash k) 1))");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(gethash key)", Dialect::Clojure).expect("parse input");
        let report =
            collect_accessor_arity_violations(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("collect accessor arity violations");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(nth i items)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_arity() {
        let report = report("(defun f (k)\n  (gethash k))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "accessor-arity");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("gethash")),
                ("argument_count", json!(1)),
                ("min_arity", json!(2)),
                ("max_arity", json!(3)),
                ("expected", json!("2 or 3")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "op=gethash".to_owned(),
                "expected=2 or 3".to_owned(),
                "arguments=1".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "gethash takes 2 or 3 argument(s) but has 1"
        );
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(nth n)\n(nth n items)\n(elt seq 0)\n");
        assert_eq!(report.summary, vec![("call_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
