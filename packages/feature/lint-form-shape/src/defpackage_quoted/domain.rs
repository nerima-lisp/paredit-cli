//! Common Lisp `defpackage`-quoted-designator detection: a quoted or
//! sharp-quoted name inside a `defpackage` option clause that takes symbol or
//! package designators. `defpackage` is a macro that does *not* evaluate its
//! options, so `(:export 'foo)` names the two symbols `quote` and `foo` (or, in
//! most readers, exports a symbol literally spelled with a leading quote) rather
//! than `foo` — almost always a bug where the author reflexively quoted the name.
//! This is report-only: the fix (drop the quote) is mechanical but changes the
//! reader-level shape, so it is surfaced for the author to confirm.
//!
//! Scanned clauses are the ones whose entries are symbol/package designators:
//! `:export`, `:shadow`, `:intern`, `:import-from`, `:shadowing-import-from`,
//! `:use`, and `:nicknames`. Any entry carrying a `'`/`#'` reader prefix is
//! flagged. (`:size`/`:documentation` and unknown clauses are ignored.)
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};

/// `defpackage` option keywords whose entries are symbol/package designators.
const DESIGNATOR_CLAUSES: [&str; 7] = [
    ":export",
    ":shadow",
    ":intern",
    ":import-from",
    ":shadowing-import-from",
    ":use",
    ":nicknames",
];

/// Whether `view` carries a `'` or `#'` reader prefix (a quoted designator).
fn is_quoted(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty()
}

#[derive(Debug, Clone)]
pub struct DefpackageQuotedItem {
    pub path: PathBuf,
    /// The span of the whole `defpackage` form.
    pub span: ByteSpan,
    /// The clause keyword, lowercased (`:export`, ...).
    pub clause: String,
    /// The span of the quoted designator.
    pub designator_span: ByteSpan,
}

#[derive(Debug)]
pub struct DefpackageQuotedSummary {
    pub defpackage_form_count: usize,
    pub violations: Vec<DefpackageQuotedItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DefpackageQuotedPolicyOptions {
    fail_on_violation: bool,
}

impl DefpackageQuotedPolicyOptions {
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
pub struct DefpackageQuotedPolicy {
    pub fail_on_violation: bool,
    pub defpackage_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    defpackage_form_count: &mut usize,
    violations: &mut Vec<DefpackageQuotedItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("defpackage") {
        return;
    }
    *defpackage_form_count += 1;

    // children: [defpackage, name, option-clause...]. Scan each option clause.
    for clause in view.children.iter().skip(2) {
        if !is_paren_list(clause) {
            continue;
        }
        let Some(keyword) = list_head(clause) else {
            continue;
        };
        let lower = keyword.to_ascii_lowercase();
        if !DESIGNATOR_CLAUSES.contains(&lower.as_str()) {
            continue;
        }
        // Entries follow the clause keyword; flag any quoted designator.
        for entry in clause.children.iter().skip(1) {
            if is_quoted(entry) {
                violations.push(DefpackageQuotedItem {
                    path: path.to_path_buf(),
                    span: view.span,
                    clause: lower.clone(),
                    designator_span: entry.span,
                });
            }
        }
    }
}

/// Collects every quoted designator inside a `defpackage` across a whole file,
/// along with the total number of `defpackage` forms scanned.
pub fn collect_defpackage_quoted(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DefpackageQuotedItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut defpackage_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut defpackage_form_count, &mut violations);
        });
    }
    Ok((defpackage_form_count, violations))
}

#[must_use]
pub const fn summarize_defpackage_quoted(
    defpackage_form_count: usize,
    violations: Vec<DefpackageQuotedItem>,
) -> DefpackageQuotedSummary {
    DefpackageQuotedSummary {
        defpackage_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_defpackage_quoted_policy(
    options: DefpackageQuotedPolicyOptions,
    summary: &DefpackageQuotedSummary,
) -> DefpackageQuotedPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DefpackageQuotedPolicy {
        fail_on_violation: options.fail_on_violation(),
        defpackage_form_count: summary.defpackage_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packages(input: &str) -> (usize, Vec<DefpackageQuotedItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_defpackage_quoted(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect defpackage quoted")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_quoted_export() {
        let source = "(defpackage :app (:export 'foo 'bar))";
        let (count, violations) = packages(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].clause, ":export");
        assert_eq!(slice(source, violations[0].designator_span), "'foo");
    }

    #[test]
    fn flags_sharp_quoted_designator() {
        let (_, violations) = packages("(defpackage :app (:use #'cl))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_quoted_import_from_symbols() {
        let (_, violations) = packages("(defpackage :app (:import-from :other 'thing))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_unquoted_designators() {
        let (count, violations) = packages("(defpackage :app (:export foo bar) (:use :cl))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_designator_clause() {
        // :size takes a number; a quoted value there is out of scope.
        let (_, violations) = packages("(defpackage :app (:size 10) (:documentation \"d\"))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_clause() {
        let (_, violations) = packages("(DEFPACKAGE :app (:EXPORT 'foo))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(defpackage :app (:export 'foo))", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_defpackage_quoted(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect defpackage quoted");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = packages("(defpackage :app (:export 'foo))");
        let summary = summarize_defpackage_quoted(count, items);

        let quiet =
            evaluate_defpackage_quoted_policy(DefpackageQuotedPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_defpackage_quoted_policy(DefpackageQuotedPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
