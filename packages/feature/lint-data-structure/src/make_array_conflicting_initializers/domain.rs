//! `make-array-conflicting-initializers` detection: a `make-array` call
//! supplying both `:initial-element` and `:initial-contents`.
//!
//! CLHS `make-array` says of `:initial-contents`:
//!
//! > The consequences are undefined if both `:initial-element` and
//! > `:initial-contents` are supplied.
//!
//! In practice implementations do not leave it undefined so much as refuse it.
//! SBCL 2.6.0:
//!
//! ```text
//! === EXTRA: make-array :initial-element AND :initial-contents ===
//! ERROR: Can't specify both :INITIAL-ELEMENT and :INITIAL-CONTENTS
//! ```
//!
//! That is a hard error, raised where the call runs. It is worth a lint anyway,
//! and for exactly the reason the error is easy to miss: `make-array` calls sit
//! in initialization paths and in rarely-taken branches, so the first time this
//! runs may be in production rather than in a test.
//!
//! # Why this has no false positives worth speaking of
//!
//! The whole judgement is local to one form: two keywords in one plist. There
//! is no name resolution, no cross-form correlation, no "probably". The only
//! way to be wrong is to misread the plist, and the two ways to do *that* are
//! both tested below:
//!
//! - a keyword in a **value** position (`:initial-element :initial-contents`
//!   stores a keyword; it does not supply contents), handled by the plist
//!   stride; and
//! - a `make-array` inside `'(…)`, which is a list, not a call.
//!
//! # Scope
//!
//! Common Lisp only. `make-array` is not a Clojure, Scheme or Fennel operator.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{self, has_keyword};

#[derive(Debug, Clone)]
pub struct MakeArrayConflictingInitializersItem {
    /// The span of the whole `make-array` call: the conflict is between two of
    /// its arguments, so neither one alone is the site.
    pub span: ByteSpan,
}

impl Finding for MakeArrayConflictingInitializersItem {
    fn kind(&self) -> &'static str {
        "make-array-conflicting-initializers"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec!["conflict=initial-element+initial-contents".to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("conflict", json!("initial-element+initial-contents"))]
    }

    fn message(&self) -> String {
        "this make-array supplies both :initial-element and :initial-contents, which per CLHS \
         make-array has undefined consequences and which SBCL refuses outright (\"Can't specify \
         both :INITIAL-ELEMENT and :INITIAL-CONTENTS\"): keep :initial-contents to spell the \
         elements out, or :initial-element to fill every cell with one value"
            .to_owned()
    }
}

///
/// Both keyword scans are local reads over this form's own operand list, so the
/// quote check — the only thing here that descends from the root — runs solely
/// for a call that already supplies both.
pub fn examine_make_array_conflicting_initializers(
    tree: &SyntaxTree,
    view: &ExpressionView,
    make_array_form_count: &mut usize,
    violations: &mut Vec<MakeArrayConflictingInitializersItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "make-array")) {
        return;
    }
    *make_array_form_count += 1;

    // `(make-array dimensions &key …)`: the plist starts after the dimensions.
    if !has_keyword(view, 2, "initial-element") || !has_keyword(view, 2, "initial-contents") {
        return;
    }
    if support::locate(tree, view.span).is_none_or(|site| site.quoted) {
        return;
    }
    violations.push(MakeArrayConflictingInitializersItem { span: view.span });
}

/// Collects every conflicting `make-array` in one file, with the number of
/// `make-array` forms scanned as the denominator beside them.
pub fn build_make_array_conflicting_initializers_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MakeArrayConflictingInitializersItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("make_array_form_count", json!(0))],
        ));
    }

    let mut make_array_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let Some(view) = support::top_level_view(tree, index) else {
            continue;
        };
        let mut stack = vec![&view];
        while let Some(node) = stack.pop() {
            examine_make_array_conflicting_initializers(
                tree,
                node,
                &mut make_array_form_count,
                &mut violations,
            );
            stack.extend(node.children.iter());
        }
    }
    violations.sort_by_key(|item| item.span.start().get());

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("make_array_form_count", json!(make_array_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MakeArrayConflictingInitializersItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_make_array_conflicting_initializers_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<MakeArrayConflictingInitializersItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_call_supplying_both() {
        let found = violations("(make-array 3 :initial-element 0 :initial-contents '(1 2 3))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_them_in_either_order() {
        let found = violations("(make-array 3 :initial-contents '(1 2 3) :initial-element 0)");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn folds_the_case_the_reader_would_fold() {
        let found = violations("(make-array 3 :INITIAL-ELEMENT 0 :Initial-Contents '(1 2 3))");
        assert_eq!(found.len(), 1);
    }

    // ---- each initializer alone is the normal, correct call ----

    #[test]
    fn does_not_flag_initial_element_alone() {
        assert!(violations("(make-array 3 :initial-element 0)").is_empty());
        assert!(violations("(make-array 3 :initial-element 0 :adjustable t)").is_empty());
    }

    #[test]
    fn does_not_flag_initial_contents_alone() {
        assert!(violations("(make-array 3 :initial-contents '(1 2 3))").is_empty());
    }

    #[test]
    fn does_not_flag_a_call_with_no_initializer() {
        assert!(violations("(make-array 3)").is_empty());
        assert!(violations("(make-array '(2 3) :element-type 'double-float)").is_empty());
    }

    /// The plist stride is what gets this right: `:initial-element` is the key
    /// and `:initial-contents` is the *value* it stores, not a second key.
    #[test]
    fn does_not_read_a_keyword_in_a_value_position_as_a_key() {
        let found = violations("(make-array 3 :initial-element :initial-contents)");
        assert!(
            found.is_empty(),
            "storing the keyword :initial-contents in every cell supplies no contents"
        );
    }

    #[test]
    fn does_not_flag_a_make_array_written_inside_quoted_data() {
        let found =
            violations("(setf form '(make-array 3 :initial-element 0 :initial-contents '(1 2 3)))");
        assert!(found.is_empty(), "a quoted make-array allocates nothing");
    }

    #[test]
    fn does_not_flag_a_make_array_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defmacro m () `(make-array 3 :initial-element 0 :initial-contents '(1 2 3)))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn flags_a_nested_call_inside_a_larger_form() {
        let found = violations(
            "(defun f () (let ((v (make-array 2 :initial-element 0 :initial-contents '(1 2)))) v))",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn the_denominator_counts_every_make_array_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(make-array 3 :initial-element 0)\n\
             (make-array 3 :initial-element 0 :initial-contents '(1 2 3))\n\
             (make-array 4)",
        );
        assert_eq!(scanned.summary, vec![("make_array_form_count", json!(3))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(make-array 3)", Dialect::Clojure).expect("parse");
        let built = build_make_array_conflicting_initializers_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_names_both_keywords_and_gives_both_repairs() {
        let built = report("(make-array 3 :initial-element 0 :initial-contents '(1 2 3))");
        let message = built.findings[0].message();
        assert!(message.contains(":initial-element"), "{message}");
        assert!(message.contains(":initial-contents"), "{message}");
    }
}
