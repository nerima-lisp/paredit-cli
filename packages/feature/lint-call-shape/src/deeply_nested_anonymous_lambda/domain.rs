//! Three or more anonymous `lambda`s nested inside one another with nothing
//! named in between.
//!
//! `(lambda (x) (lambda (y) (lambda (z) …)))` has no name anywhere for a reader
//! to hold on to: every intermediate step is spelled only by its position, so
//! following the data flow means re-reading the whole expression each time. One
//! level of that is ordinary (`(mapcar (lambda (x) …) xs)`), two is common in
//! higher-order code, and three is where the shape stops paying for itself.
//!
//! # What breaks the chain
//!
//! A *named* binding, because the point of the rule is the missing name. The
//! chain restarts at:
//!
//! - a definition form — `defun`, `defmacro`, `defmethod`, `defvar`, and the
//!   `cl-` spellings Emacs Lisp uses — since a lambda in its body belongs to
//!   that definition, not to whatever lexically encloses it;
//! - an assignment: `(setq handler (lambda …))` names the lambda `handler`;
//! - the binding list of `let`, `let*`, `flet`, `labels`, `macrolet` and their
//!   Emacs Lisp counterparts, since every lambda there is the value of a
//!   *named* binding.
//!
//! So `(lambda (x) (let ((step (lambda (y) …))) …))` is two chains of one, not
//! one chain of two, and is never reported.
//!
//! # What this rule does not attempt
//!
//! - It does not judge the *length* or the *complexity* of a lambda. A single
//!   forty-line lambda is somebody else's rule.
//! - It does not follow a lambda passed to a function and returned from it.
//!   Only lexical nesting is visible here.
//! - It reports the *innermost* lambda of a chain, once. A chain deeper than
//!   the threshold is one finding, not one per qualifying prefix; two sibling
//!   chains under a common outer lambda are two findings, because they are two
//!   places to read.
//! - Dialect scope is Common Lisp and Emacs Lisp, where `lambda`, `let`,
//!   `let*`, `flet`/`cl-flet`, `labels`/`cl-labels` and `defun` all mean what
//!   the chain logic assumes. Clojure's `fn`, Scheme's named `let` and
//!   Racket's `define` have different enough binding shapes that guessing at
//!   them would be a guess.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    count_occurrences_ignoring_case, descend_to, descent_is_unevaluated,
    for_each_evaluated_positioned, root_child_containing, root_child_span_containing,
};

/// How many levels of anonymous nesting are acceptable by default. Three is the
/// first level that is reported, so the knob is one less than that.
pub const DEFAULT_MAX_NESTING: usize = 2;

/// The dialects this rule models.
pub const MODELLED_DIALECTS: [Dialect; 2] = [Dialect::CommonLisp, Dialect::EmacsLisp];

/// Forms that give what follows them a name, so a lambda underneath one starts
/// a fresh chain.
///
/// Common Lisp and Emacs Lisp spellings both, which is safe in one table: a
/// name that means nothing in one dialect simply never occurs there, and a
/// wider table only ever makes the rule quieter.
const NAMING_FORMS: &[&str] = &[
    "defun",
    "defmacro",
    "defmethod",
    "defgeneric",
    "defvar",
    "defparameter",
    "defconstant",
    "defsetf",
    "define-compiler-macro",
    "define-symbol-macro",
    "defsubst",
    "defalias",
    "fset",
    "cl-defun",
    "cl-defmacro",
    "cl-defmethod",
    "cl-defgeneric",
    "setq",
    "setf",
    "psetq",
    "psetf",
];

/// Forms whose child 1 is a list of *named* bindings.
const BINDING_FORMS: &[&str] = &[
    "let",
    "let*",
    "letrec",
    "flet",
    "labels",
    "macrolet",
    "symbol-macrolet",
    "cl-flet",
    "cl-flet*",
    "cl-labels",
    "cl-macrolet",
    "cl-symbol-macrolet",
    "cl-letf",
    "cl-letf*",
];

/// One reported chain: the innermost lambda, and how deep the chain reaching it
/// is.
#[derive(Debug, Clone)]
pub struct DeeplyNestedLambdaItem {
    /// The span of the innermost `(lambda …)` of the chain.
    pub span: ByteSpan,
    /// How many anonymous lambdas enclose this one, counting it.
    pub nesting_depth: usize,
    /// The depth this run allowed.
    pub threshold: usize,
}

impl Finding for DeeplyNestedLambdaItem {
    fn kind(&self) -> &'static str {
        "deeply-nested-anonymous-lambda"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("nesting_depth={}", self.nesting_depth)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("nesting_depth", json!(self.nesting_depth)),
            ("threshold", json!(self.threshold)),
        ]
    }

    fn message(&self) -> String {
        message(self.nesting_depth, self.threshold)
    }
}

/// The one sentence both the report and the lint rule print.
#[must_use]
pub fn message(nesting_depth: usize, threshold: usize) -> String {
    format!(
        "{nesting_depth} anonymous lambdas nested with no name in between, more than the \
         {threshold} allowed; naming the intermediate steps (a `let`, an `flet`) gives a reader \
         something to hold on to"
    )
}

/// Whether a node is an anonymous `(lambda …)`.
///
/// A reader prefix does not change the answer: `#'(lambda …)` is the same
/// anonymous function written for a compiler that wants the `function` wrapper.
#[must_use]
pub fn is_anonymous_lambda(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| symbol_in(head, &["lambda"]))
}

/// Whether the edge from `parent` into its child at `index` leaves the chain,
/// because what is under it has a name.
fn edge_leaves_chain(parent: &ExpressionView, index: usize) -> bool {
    let Some(head) = list_head(parent) else {
        return false;
    };
    symbol_in(head, NAMING_FORMS) || (index == 1 && symbol_in(head, BINDING_FORMS))
}

/// Whether some anonymous lambda strictly inside `view` continues `view`'s own
/// chain.
///
/// This is what makes a chain produce one finding rather than one per prefix:
/// only the lambda with nothing chained below it is reported.
///
/// Costs `view`'s own subtree, which the dispatcher has already materialized —
/// never the file.
#[must_use]
pub fn continues_below(view: &ExpressionView) -> bool {
    let mut found = false;
    for_each_evaluated_positioned(view, |parent, node| {
        if parent.is_some_and(|(enclosing, index)| edge_leaves_chain(enclosing, index)) {
            // A named binding: whatever lambda is under it starts its own
            // chain, so it neither counts here nor is looked into.
            return false;
        }
        if node.span != view.span && is_anonymous_lambda(node) {
            found = true;
        }
        true
    });
    found
}

/// What the descent to one lambda says about it.
#[derive(Debug, Clone, Copy)]
struct ChainContext {
    nesting_depth: usize,
    unevaluated: bool,
}

/// Counts the anonymous lambdas enclosing `target`, and answers the quote
/// question, in one descent.
///
/// Both questions are answered from the same walk on purpose: each is a
/// [`descend_to`] of its own otherwise, and the descent is the only part of
/// this rule that is not proportional to the matched node's own subtree.
fn chain_context_at(
    tree: &SyntaxTree,
    target: ByteSpan,
    max_nesting: usize,
) -> Option<ChainContext> {
    // A chain of N needs N lambdas spelled inside one top-level form. Counting
    // them in the *bytes* of that form costs a fraction of building its
    // `ExpressionView`, and rules out almost every match in a file where
    // lambdas are used one at a time. A mention inside a string or a comment
    // clears the guard, which is the harmless direction: the descent then runs
    // and answers as it would have anyway.
    let span = root_child_span_containing(tree, target)?;
    if count_occurrences_ignoring_case(span.slice(tree.source()), "lambda") <= max_nesting {
        return None;
    }
    let top_level = root_child_containing(tree, target)?;
    let steps = descend_to(&top_level, target);
    if steps.last()?.view.span != target {
        return None;
    }

    let mut nesting_depth = 0;
    for step in &steps {
        if is_anonymous_lambda(step.view) {
            nesting_depth += 1;
        }
        if step
            .next_child
            .is_some_and(|index| edge_leaves_chain(step.view, index))
        {
            nesting_depth = 0;
        }
    }

    Some(ChainContext {
        nesting_depth,
        unevaluated: descent_is_unevaluated(&steps),
    })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// `lambda` through the single dispatch pass instead of walking the tree again.
///
/// `max_nesting` is the deepest chain that is *not* reported.
pub fn examine_lambda(
    tree: &SyntaxTree,
    view: &ExpressionView,
    max_nesting: usize,
    lambda_form_count: &mut usize,
    violations: &mut Vec<DeeplyNestedLambdaItem>,
) {
    if !is_anonymous_lambda(view) {
        return;
    }
    *lambda_form_count += 1;

    // Cheapest first, by a wide margin: `chain_context_at` opens with a byte
    // scan of the enclosing top-level form and returns `None` unless that form
    // spells `lambda` often enough for a chain to exist at all, which in a file
    // that uses lambdas one at a time is never. Only then is the subtree walk
    // that decides "is this the innermost one" worth running.
    let Some(context) = chain_context_at(tree, view.span, max_nesting) else {
        return;
    };
    if continues_below(view) {
        return;
    }
    if context.nesting_depth <= max_nesting || context.unevaluated {
        return;
    }

    violations.push(DeeplyNestedLambdaItem {
        span: view.span,
        nesting_depth: context.nesting_depth,
        threshold: max_nesting,
    });
}

/// Collects every over-nested anonymous lambda chain in one file, with the
/// number of `lambda` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no over-nested lambda here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_deeply_nested_anonymous_lambda_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DeeplyNestedLambdaItem>> {
    build_report_with_threshold(path, dialect, tree, DEFAULT_MAX_NESTING)
}

/// [`build_deeply_nested_anonymous_lambda_report`] at a caller-chosen
/// threshold, which is what the rule's own tests vary.
pub fn build_report_with_threshold(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    max_nesting: usize,
) -> LintResult<FileFindings<DeeplyNestedLambdaItem>> {
    if !MODELLED_DIALECTS.contains(&dialect) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("lambda_form_count", json!(0))],
        ));
    }

    let mut lambda_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        paredit_core_syntax::view_query::for_each_subview(&view, |subview| {
            examine_lambda(
                tree,
                subview,
                max_nesting,
                &mut lambda_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("lambda_form_count", json!(lambda_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_in(input: &str, dialect: Dialect) -> FileFindings<DeeplyNestedLambdaItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_deeply_nested_anonymous_lambda_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<DeeplyNestedLambdaItem> {
        report_in(input, Dialect::CommonLisp).findings
    }

    fn depths(input: &str) -> Vec<usize> {
        findings(input)
            .iter()
            .map(|item| item.nesting_depth)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_three_nested_anonymous_lambdas() {
        assert_eq!(
            depths("(lambda (x) (lambda (y) (lambda (z) (+ x y z))))"),
            [3]
        );
    }

    #[test]
    fn reports_the_innermost_lambda_once_however_deep_the_chain() {
        let items = findings("(lambda (a) (lambda (b) (lambda (c) (lambda (d) d))))");
        assert_eq!(items.len(), 1, "one chain is one finding");
        assert_eq!(items[0].nesting_depth, 4);
    }

    #[test]
    fn the_reported_span_is_the_innermost_lambda() {
        let source = "(lambda (x) (lambda (y) (lambda (z) z)))";
        let items = findings(source);
        assert_eq!(items[0].span.slice(source), "(lambda (z) z)");
    }

    #[test]
    fn two_sibling_chains_are_two_findings() {
        let items =
            findings("(lambda (x) (list (lambda (y) (lambda (a) a)) (lambda (z) (lambda (b) b))))");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].nesting_depth, 3);
        assert_eq!(items[1].nesting_depth, 3);
    }

    #[test]
    fn a_chain_inside_a_defun_body_counts_from_the_defun() {
        assert_eq!(
            depths("(defun compose3 () (lambda (f) (lambda (g) (lambda (x) x))))"),
            [3]
        );
    }

    #[test]
    fn a_sharp_quoted_lambda_still_counts() {
        assert_eq!(
            depths("#'(lambda (x) #'(lambda (y) #'(lambda (z) z)))"),
            [3]
        );
    }

    #[test]
    fn the_head_is_case_folded_and_package_qualifiers_are_ignored() {
        assert_eq!(depths("(LAMBDA (x) (cl:lambda (y) (LAMBDA (z) z)))"), [3]);
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn two_nested_lambdas_are_not_reported() {
        assert!(findings("(lambda (x) (lambda (y) (+ x y)))").is_empty());
    }

    #[test]
    fn one_lambda_is_not_reported() {
        assert!(findings("(mapcar (lambda (x) (* x x)) numbers)").is_empty());
    }

    /// The guard the whole rule is about: the middle step has a name.
    #[test]
    fn a_let_bound_intermediate_lambda_breaks_the_chain() {
        assert!(
            findings("(lambda (x) (let ((step (lambda (y) (lambda (z) z)))) (funcall step x)))")
                .is_empty()
        );
    }

    #[test]
    fn an_flet_bound_intermediate_breaks_the_chain() {
        assert!(
            findings("(lambda (x) (flet ((step (y) (lambda (z) (lambda (w) w)))) (step x)))")
                .is_empty()
        );
    }

    #[test]
    fn an_assignment_breaks_the_chain() {
        assert!(findings("(lambda (x) (setq handler (lambda (y) (lambda (z) z))))").is_empty());
    }

    #[test]
    fn a_nested_defun_breaks_the_chain() {
        assert!(findings("(lambda (x) (defun helper () (lambda (y) (lambda (z) z))))").is_empty());
    }

    /// Three lambdas in a file, none inside another.
    #[test]
    fn sibling_lambdas_do_not_accumulate() {
        assert!(findings("(list (lambda (x) x) (lambda (y) y) (lambda (z) z))").is_empty());
    }

    /// A realistic higher-order pipeline: two levels, named where it matters.
    #[test]
    fn idiomatic_higher_order_code_is_silent() {
        let source = "(defun make-adder (n)\n  (lambda (x) (+ x n)))\n\n\
             (defun apply-twice (f)\n  (lambda (x) (funcall f (funcall f x))))\n\n\
             (defun transform (rows)\n  (mapcar (lambda (row)\n            (remove-if (lambda (cell) (null cell)) row))\n          rows))\n";
        assert!(findings(source).is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn a_hard_quoted_chain_is_data() {
        assert!(findings("'(lambda (x) (lambda (y) (lambda (z) z)))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings("(quote (lambda (x) (lambda (y) (lambda (z) z))))").is_empty());
    }

    #[test]
    fn a_quasiquoted_chain_without_an_unquote_is_data() {
        assert!(findings("`(lambda (x) (lambda (y) (lambda (z) z)))").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(findings("'(a ,(lambda (x) (lambda (y) (lambda (z) z))))").is_empty());
    }

    #[test]
    fn an_unquoted_chain_inside_a_quasiquote_is_code_again() {
        assert_eq!(
            depths("`(a ,(lambda (x) (lambda (y) (lambda (z) z))))"),
            [3]
        );
    }

    #[test]
    fn a_chain_spelled_only_inside_a_string_is_never_a_form() {
        assert!(findings("(format nil \"(lambda (x) (lambda (y) (lambda (z) z)))\")").is_empty());
    }

    // -- thresholds and dialects ---------------------------------------------

    #[test]
    fn the_threshold_moves_what_is_reported() {
        let tree = SyntaxTree::parse_with_dialect(
            "(lambda (x) (lambda (y) (lambda (z) z)))",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let strict =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 1)
                .expect("report");
        assert_eq!(strict.findings.len(), 1);
        let lenient =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 3)
                .expect("report");
        assert!(lenient.findings.is_empty());
    }

    #[test]
    fn emacs_lisp_is_modelled_with_its_own_binding_spellings() {
        let flagged = report_in(
            "(lambda (x) (lambda (y) (lambda (z) z)))",
            Dialect::EmacsLisp,
        );
        assert!(flagged.dialect_modelled);
        assert_eq!(flagged.findings.len(), 1);

        let named = report_in(
            "(lambda (x) (cl-flet ((step (y) (lambda (z) (lambda (w) w)))) (step x)))",
            Dialect::EmacsLisp,
        );
        assert!(named.findings.is_empty());
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let report = report_in("(fn [x] (fn [y] (fn [z] z)))", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("lambda_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_lambda_scanned_not_only_the_flagged_ones() {
        let report = report_in(
            "(lambda (x) (lambda (y) (lambda (z) z)))",
            Dialect::CommonLisp,
        );
        assert_eq!(report.summary, vec![("lambda_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_depth() {
        let report = report_in(
            "(defun f ()\n  (lambda (a) (lambda (b) (lambda (c) c))))\n",
            Dialect::CommonLisp,
        );
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "deeply-nested-anonymous-lambda");
        assert_eq!(finding.text_columns(), vec!["nesting_depth=3".to_owned()]);
        assert!(finding.message().contains("3 anonymous lambdas"));
    }
}
