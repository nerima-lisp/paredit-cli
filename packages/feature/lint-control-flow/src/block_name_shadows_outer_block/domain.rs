//! Common Lisp shadowed-block detection: a `(block foo …)` nested inside
//! another `(block foo …)`.
//!
//! Block names are lexically scoped and shadow (CLHS 5.2 `block`), so a
//! `(return-from foo …)` written between the two exits the *inner* block. That
//! is well-defined and occasionally deliberate; what makes it worth reporting
//! is that the two spellings are identical, so the reader cannot see which
//! block an exit leaves without counting nesting levels.
//!
//! # What is required for a finding
//!
//! The inner block must actually contain a `(return-from foo …)`. Shadowing
//! with no exit under it changes nothing about the program and reads as
//! nothing worth a reader's attention, so it is left alone — this rule reports
//! the ambiguity, not the nesting.
//!
//! Nesting deeper than two is reported once per adjacent pair rather than once
//! per pair of levels: the walk stops at the first same-named block it finds,
//! and that block reports its own inner one when the engine matches it. A
//! triply-nested `foo` is therefore two findings, each naming one shadowing.
//!
//! # What this deliberately does not cover
//!
//! `(defun foo () … (block foo …))` — reusing a function's own implicit block
//! name — is not reported. It is a common and usually intentional idiom for
//! making an exit local, and the two spellings there are *not* identical: one
//! is a `defun`, the other a `block`.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, for_each_evaluated_subview_where, is_unevaluated_at, plain_name,
};

#[derive(Debug, Clone)]
pub struct BlockNameShadowsOuterBlockItem {
    /// The span of the *inner* block — the one doing the shadowing.
    pub span: ByteSpan,
    /// The name both blocks carry, normalized.
    pub block_name: String,
    /// Where the outer block of the same name begins, so a reader can find the
    /// one being shadowed.
    pub outer_span: ByteSpan,
}

impl Finding for BlockNameShadowsOuterBlockItem {
    fn kind(&self) -> &'static str {
        "block-name-shadows-outer-block"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("block={}", self.block_name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("block", json!(self.block_name)),
            ("outer_start", json!(self.outer_span.start().get())),
        ]
    }

    fn message(&self) -> String {
        message_for(&self.block_name)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(block_name: &str) -> String {
    format!(
        "this block `{block_name}` is nested inside another block `{block_name}`, so a \
         return-from {block_name} under it exits the inner one"
    )
}

/// The name of a `(block name …)`, or `None` for anything else.
fn block_name(view: &ExpressionView) -> Option<String> {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "block")) {
        return None;
    }
    view.children.get(1).and_then(plain_name)
}

/// Whether `view`'s subtree holds a `(return-from name …)` in evaluated code.
fn contains_exit_to(view: &ExpressionView, name: &str) -> bool {
    let mut found = false;
    for_each_evaluated_subview(view, |subview| {
        if found || !is_paren_list(subview) {
            return;
        }
        if list_head(subview).is_some_and(|head| symbol_is(head, "return-from")) {
            found = subview.children.get(1).and_then(plain_name).as_deref() == Some(name);
        }
    });
    found
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Reads only the matched form's own subtree.
pub fn examine_block(
    tree: &SyntaxTree,
    view: &ExpressionView,
    block_form_count: &mut usize,
    violations: &mut Vec<BlockNameShadowsOuterBlockItem>,
) {
    let Some(name) = block_name(view) else {
        return;
    };
    *block_form_count += 1;

    // The spans of the shadowing blocks, and whether each holds an exit to the
    // shared name. Spans rather than views because the walk hands out
    // references that may not outlive its own closure.
    let mut shadowing = Vec::new();
    for_each_evaluated_subview_where(
        view,
        |subview| {
            // Stop at the first same-named block: what is inside *it* is that
            // block's own finding, not this one's, so a triple nesting is two
            // findings rather than three.
            subview.span == view.span || block_name(subview).as_deref() != Some(name.as_str())
        },
        |subview| {
            if subview.span != view.span && block_name(subview).as_deref() == Some(name.as_str()) {
                shadowing.push((subview.span, contains_exit_to(subview, &name)));
            }
        },
    );
    if shadowing.is_empty() {
        return;
    }
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    for (span, has_exit) in shadowing {
        if has_exit {
            violations.push(BlockNameShadowsOuterBlockItem {
                span,
                block_name: name.clone(),
                outer_span: view.span,
            });
        }
    }
}

/// Collects every shadowed block name in one file, with the number of `block`
/// forms scanned as the denominator beside them.
pub fn build_block_name_shadows_outer_block_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<BlockNameShadowsOuterBlockItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("block_form_count", json!(0))],
        ));
    }

    let mut block_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_block(tree, subview, &mut block_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("block_form_count", json!(block_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<BlockNameShadowsOuterBlockItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_block_name_shadows_outer_block_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn names(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.block_name)
            .collect()
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_nested_block_of_the_same_name_with_an_exit() {
        assert_eq!(
            names("(block scan (block scan (return-from scan 1)))"),
            vec!["scan"]
        );
    }

    #[test]
    fn flags_a_shadowing_block_nested_deep_in_the_body() {
        assert_eq!(
            names("(block scan (dolist (x l) (when x (block scan (return-from scan x)))))"),
            vec!["scan"]
        );
    }

    /// Three levels are two adjacent shadowings, each reported by the block
    /// that is being shadowed.
    #[test]
    fn flags_each_adjacent_pair_of_a_triple_nesting_once() {
        assert_eq!(
            names("(block a (block a (block a (return-from a 1))))"),
            vec!["a", "a"]
        );
    }

    #[test]
    fn the_span_covers_the_inner_block_and_names_the_outer() {
        let report = report("(block scan (block scan (return-from scan 1)))");
        let finding = &report.findings[0];
        assert_eq!(finding.span().start().get(), 12);
        assert_eq!(finding.outer_span.start().get(), 0);
    }

    // -- near-miss negatives ------------------------------------------------

    #[test]
    fn does_not_flag_a_nested_block_with_a_different_name() {
        assert!(names("(block outer (block inner (return-from inner 1)))").is_empty());
    }

    /// Shadowing with no exit under it changes nothing a reader can trip over.
    #[test]
    fn does_not_flag_a_shadowing_block_with_no_exit_to_the_name() {
        assert!(names("(block scan (block scan (compute)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_shadowing_block_whose_only_exit_names_something_else() {
        assert!(names("(block scan (block scan (return-from other 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_two_sibling_blocks_of_the_same_name() {
        assert!(
            names(
                "(defun f () (block scan (return-from scan 1)) (block scan (return-from scan 2)))"
            )
            .is_empty()
        );
    }

    /// The deliberate exclusion: a `defun`'s implicit block is not a `block`.
    #[test]
    fn does_not_flag_a_block_reusing_its_own_defuns_name() {
        assert!(names("(defun scan () (block scan (return-from scan 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_block() {
        assert!(names("(block)").is_empty());
        assert!(names("(block (compute-name) (block (compute-name) 1))").is_empty());
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert_eq!(
            names("(CL:BLOCK Scan (BLOCK SCAN (RETURN-FROM scan 1)))"),
            vec!["scan"]
        );
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert!(names("'(block a (block a (return-from a 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert!(names("(quote (block a (block a (return-from a 1))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert!(names("'(x ,(block a (block a (return-from a 1))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert!(names("(defmacro m () `(block a (block a (return-from a 1))))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            names("(defmacro m () `(x ,(block a (block a (return-from a 1)))))"),
            vec!["a"]
        );
    }

    /// A quoted inner block is data, not a shadowing block.
    #[test]
    fn does_not_flag_a_quoted_inner_block() {
        assert!(names("(block a '(block a (return-from a 1)))").is_empty());
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_flag_a_block_inside_a_string_literal() {
        assert!(names("(block a \"(block a (return-from a 1))\")").is_empty());
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(block a (block a (return-from a 1)))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_block_name_shadows_outer_block_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("block_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_block_scanned_not_only_the_flagged_ones() {
        let report = report("(block a (block a (return-from a 1)))\n(block b 1)\n");
        assert_eq!(report.summary, vec![("block_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_block_name() {
        let report = report("(block scan\n  (block scan (return-from scan 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "block-name-shadows-outer-block");
        assert_eq!(finding.text_columns(), vec!["block=scan".to_owned()]);
        assert_eq!(finding.json_fields()[0], ("block", json!("scan")));
    }
}
