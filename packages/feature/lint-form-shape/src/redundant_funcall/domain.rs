//! Common Lisp redundant-`funcall` detection: `(funcall #'FN ARGS…)` where the
//! function argument is a sharp-quoted plain symbol. `(funcall #'foo a b)` and
//! `(foo a b)` resolve `foo` through the exact same lexical function namespace —
//! a `flet`/`labels` binding if one is in scope, otherwise the global function —
//! so the `funcall` and the `#'` are pure ceremony. This shape is common after a
//! higher-order helper is inlined or a macro is expanded mechanically.
//!
//! Only the sharp-quoted *symbol* form is flagged, because that is the only one
//! whose direct-call rewrite is guaranteed equivalent:
//!
//!   - `#'foo` reads as an atom `foo` carrying [`ReaderPrefix::Function`]; a
//!     direct `(foo …)` uses the identical lexical resolution. Equivalent.
//!   - `(funcall fn …)` with a *variable* `fn` (no sharp-quote) genuinely
//!     dispatches on a runtime value and is never flagged.
//!   - `(funcall #'(lambda …) …)` sharp-quotes a *list*, not a symbol, so it is
//!     left alone (unwrapping a lambda is a different transformation).
//!   - `(funcall 'foo …)` (ordinary quote) resolves `foo`'s *global* function at
//!     call time and is **not** equivalent to `(foo …)` under a local `flet`, so
//!     only the sharp-quote (`#'`) form is matched.
//!
//! The fix deletes the `funcall ` head and the `#'` prefix in one contiguous
//! cut — from the `funcall` symbol up to the callee symbol — leaving the callee
//! and every argument byte-identical. That makes the rule auto-fixable.
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
    ByteOffset, ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix,
    SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The sharp-quoted symbol name of `view` (`#'foo` → `foo`), or `None` when
/// `view` is not an atom carrying exactly the `#'` function prefix over a
/// non-empty symbol. Requiring the prefix list to be *exactly* `[Function]`
/// rejects combinations like `,#'foo` whose rewrite is not a plain deletion.
fn sharp_quoted_symbol(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    if view.reader_prefixes.as_slice() != [ReaderPrefix::Function] {
        return None;
    }
    let symbol = atom_text(view)?.get(view.symbol_offset..)?;
    (!symbol.is_empty()).then_some(symbol)
}

#[derive(Debug, Clone)]
pub struct RedundantFuncallItem {
    /// The span of the whole `(funcall #'FN …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The callee symbol name (`foo` for `#'foo`).
    pub callee: String,
    /// The bytes a fix deletes: from the `funcall` head through the `#'` prefix,
    /// i.e. everything up to the callee symbol. Deleting this turns
    /// `(funcall #'foo a b)` into `(foo a b)`.
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to make
    /// the cut, and the command never printed it.
    pub removal_span: ByteSpan,
}

impl Finding for RedundantFuncallItem {
    /// The rule's own name. The callee is a symbol read out of the source, not
    /// a tag from a closed set, so it stays a JSON field.
    fn kind(&self) -> &'static str {
        "redundant-funcall"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("callee={}", self.callee)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("callee", json!(self.callee))]
    }

    /// The same sentence the `redundant-funcall` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "funcall of #'{} is a direct call; (funcall #'{} …) is ({} …)",
            self.callee, self.callee, self.callee
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_funcall(
    view: &ExpressionView,
    source: &str,
    funcall_form_count: &mut usize,
    violations: &mut Vec<RedundantFuncallItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("funcall") {
        return;
    }
    *funcall_form_count += 1;

    // children[0] is `funcall`; children[1] is the function argument.
    if view.children.len() < 2 {
        return;
    }
    let funcall_head = &view.children[0];
    let function_arg = &view.children[1];
    let Some(callee) = sharp_quoted_symbol(function_arg) else {
        return;
    };

    // Delete `funcall ` and the `#'`: from the head start to the callee symbol.
    let callee_symbol_start =
        ByteOffset::new(function_arg.span.start().get() + function_arg.symbol_offset);
    violations.push(RedundantFuncallItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        callee: callee.to_owned(),
        removal_span: ByteSpan::new(funcall_head.span.start(), callee_symbol_start),
    });
}

/// Collects every redundant `(funcall #'symbol …)` in one file, with the number
/// of `funcall` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant funcall here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_funcall_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantFuncallItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("funcall_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut funcall_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_funcall(subview, source, &mut funcall_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("funcall_form_count", json!(funcall_form_count))],
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

    fn report(input: &str) -> FileFindings<RedundantFuncallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_funcall_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant funcall report")
    }

    /// The `(funcall_form_count, violations)` pair the report is built from.
    fn funcalls(input: &str) -> (u64, Vec<RedundantFuncallItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "funcall_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("funcall_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_sharp_quoted_symbol_with_args() {
        let (count, violations) = funcalls("(funcall #'foo a b)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn flags_sharp_quoted_symbol_with_no_args() {
        let (_, violations) = funcalls("(funcall #'foo)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn removal_span_covers_funcall_and_the_sharp_quote() {
        // Deleting the removal span must turn the form into `(foo a b)`.
        let source = "(funcall #'foo a b)";
        let (_, violations) = funcalls(source);
        let removal = violations[0].removal_span;
        let (start, end) = (removal.start().get(), removal.end().get());
        let mut rewritten = String::from(source);
        rewritten.replace_range(start..end, "");
        assert_eq!(rewritten, "(foo a b)");
    }

    #[test]
    fn does_not_flag_a_variable_function_argument() {
        let (count, violations) = funcalls("(funcall fn a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_sharp_quoted_lambda() {
        let (_, violations) = funcalls("(funcall #'(lambda (x) x) y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_ordinary_quoted_symbol() {
        // '(foo) resolves the *global* function, not equivalent under flet.
        let (_, violations) = funcalls("(funcall 'foo a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = funcalls("(apply #'foo args)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_funcall_head() {
        let (_, violations) = funcalls("(FUNCALL #'foo x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn finds_a_nested_redundant_funcall() {
        let (_, violations) = funcalls("(mapcar (lambda (g) (funcall #'h g)) xs)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "h");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(funcall #'foo a)", Dialect::Clojure).expect("parse");
        let report = build_redundant_funcall_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant funcall report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("funcall_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(funcall fn a)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_callee() {
        let report = report("(defun run (x)\n  (funcall #'process x))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "redundant-funcall");
        assert_eq!(finding.json_fields(), vec![("callee", json!("process"))]);
        assert_eq!(finding.text_columns(), vec!["callee=process".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_funcall_scanned_not_only_the_flagged_ones() {
        let report = report("(funcall #'foo a)\n(funcall fn b)\n");
        assert_eq!(report.summary, vec![("funcall_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
