//! Common Lisp `make-hash-table`-redundant-`:test`-`eql` detection: a
//! `(make-hash-table … :test 'eql …)`. The `:test` argument of `make-hash-table`
//! *defaults to* `eql`, so an explicit `:test 'eql`/`#'eql` restates the default
//! and can be dropped with no behavioral change.
//!
//! The three eql designators are recognized: `#'eql`, `'eql`, and the explicit
//! `(function eql)` / `(quote eql)` list. Any other test (`'equal`, `'equalp`,
//! `'eq`) is a real choice and left alone. The pair may sit anywhere in the
//! keyword arguments; the fix deletes just the ` :test 'eql` pair (from the end
//! of the preceding argument through the eql designator), leaving other keyword
//! arguments byte-identical, so the rule is auto-fixable.
//!
//! This is the `make-hash-table` sibling of
//! `redundant-eql-test` (which covers the sequence/list
//! operators whose `:test` defaults to `eql`).
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// Whether `view` designates the `eql` function: `#'eql`, `'eql`, or the
/// explicit `(function eql)` / `(quote eql)` list.
fn is_eql_designator(view: &ExpressionView) -> bool {
    if let Some(text) = atom_text(view) {
        let symbol = text.get(view.symbol_offset..).unwrap_or(text);
        return symbol.eq_ignore_ascii_case("eql")
            && view.reader_prefixes.len() == 1
            && matches!(
                view.reader_prefixes[0],
                ReaderPrefix::Function | ReaderPrefix::Quote
            );
    }
    if is_paren_list(view) && view.children.len() == 2 && view.reader_prefixes.is_empty() {
        let heads_function = list_head(view)
            .is_some_and(|h| h.eq_ignore_ascii_case("function") || h.eq_ignore_ascii_case("quote"));
        if heads_function {
            return atom_text(&view.children[1]).is_some_and(|t| t.eq_ignore_ascii_case("eql"));
        }
    }
    false
}

/// Whether `view` is the `:test` keyword atom.
fn is_test_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":test"))
}

#[derive(Debug, Clone)]
pub struct MakeHashTableTestItem {
    pub path: PathBuf,
    /// The span of the whole `make-hash-table` form.
    pub span: ByteSpan,
    /// The span to delete: the ` :test 'eql` argument pair.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct MakeHashTableTestSummary {
    pub make_hash_table_form_count: usize,
    pub violations: Vec<MakeHashTableTestItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MakeHashTableTestPolicyOptions {
    fail_on_violation: bool,
}

impl MakeHashTableTestPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct MakeHashTableTestPolicy {
    pub fail_on_violation: bool,
    pub make_hash_table_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    make_hash_table_form_count: &mut usize,
    violations: &mut Vec<MakeHashTableTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("make-hash-table") {
        return;
    }
    *make_hash_table_form_count += 1;

    // Scan the keyword arguments for a `:test` immediately followed by an `eql`
    // designator. `children[0]` is the operator.
    for index in 1..view.children.len().saturating_sub(1) {
        if !is_test_keyword(&view.children[index]) {
            continue;
        }
        let value = &view.children[index + 1];
        if !is_eql_designator(value) {
            continue;
        }
        let removal_span = ByteSpan::new(view.children[index - 1].span.end(), value.span.end());
        violations.push(MakeHashTableTestItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
        });
        return;
    }
}

/// Collects every `(make-hash-table … :test 'eql …)` across a whole file, along
/// with the total number of `make-hash-table` forms scanned.
pub fn collect_make_hash_table_tests(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<MakeHashTableTestItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut make_hash_table_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                path,
                &mut make_hash_table_form_count,
                &mut violations,
            );
        });
    }
    Ok((make_hash_table_form_count, violations))
}

#[must_use]
pub const fn summarize_make_hash_table_tests(
    make_hash_table_form_count: usize,
    violations: Vec<MakeHashTableTestItem>,
) -> MakeHashTableTestSummary {
    MakeHashTableTestSummary {
        make_hash_table_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_make_hash_table_test_policy(
    options: MakeHashTableTestPolicyOptions,
    summary: &MakeHashTableTestSummary,
) -> MakeHashTableTestPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MakeHashTableTestPolicy {
        fail_on_violation: options.fail_on_violation(),
        make_hash_table_form_count: summary.make_hash_table_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<MakeHashTableTestItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_make_hash_table_tests(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect make-hash-table tests")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_quoted_eql_test() {
        let source = "(make-hash-table :test 'eql)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :test 'eql");
    }

    #[test]
    fn flags_sharp_quoted_eql_test() {
        let (_, violations) = calls("(make-hash-table :test #'eql)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(make-hash-table :size 16 :test 'eql)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :test 'eql");
    }

    #[test]
    fn does_not_flag_a_custom_test() {
        assert!(calls("(make-hash-table :test 'equal)").1.is_empty());
        assert!(calls("(make-hash-table :test #'equalp)").1.is_empty());
    }

    #[test]
    fn does_not_flag_bare_make_hash_table() {
        let (count, violations) = calls("(make-hash-table)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(MAKE-HASH-TABLE :test 'eql)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f () (make-hash-table :test 'eql))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(make-hash-table :test 'eql)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_make_hash_table_tests(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect make-hash-table tests");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(make-hash-table :test 'eql)");
        let summary = summarize_make_hash_table_tests(count, items);

        let quiet = evaluate_make_hash_table_test_policy(
            MakeHashTableTestPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_make_hash_table_test_policy(
            MakeHashTableTestPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
