//! Common Lisp declared-return-arity detection: a `(declaim (ftype (function
//! (…) (values …)) f))` that promises more values than the `defun` written
//! immediately below it literally returns.
//!
//! ```text
//! (declaim (ftype (function (integer) (values integer integer)) f))
//! (defun f (x) (values x))          ; promises two, returns one
//! ```
//!
//! SBCL compiles that pair with
//! `caught WARNING: Derived type of (#:G0) is (VALUES INTEGER &OPTIONAL),
//! conflicting with the declared function return type
//! (VALUES INTEGER INTEGER &REST T)`. The declaration is a promise the compiler
//! is entitled to optimize against, so violating it is undefined behaviour at
//! low safety, not a style point.
//!
//! # Only "too few", never "too many" — verified, not assumed
//!
//! A `(values a b)` return type carries an implicit `&rest t` tail. SBCL is
//! *silent* on
//!
//! ```text
//! (declaim (ftype (function (integer) (values integer)) g))
//! (defun g (x) (values x x x))      ; three values, one declared: legal
//! ```
//!
//! so returning surplus values is conforming and reporting it would be a
//! guaranteed false positive. This rule fires only when the function returns
//! **fewer** values than the declaration names.
//!
//! # Disjoint from `the-arity`
//!
//! [`crate::the_arity`] is about the `the` special form having something other
//! than exactly two arguments — a malformed `(the …)`. It never reads a
//! `(values …)` type at all and never looks at `declaim`. The two share no
//! head and no question.
//!
//! # Scope, and what it costs
//!
//! Only the top-level `defun` written **immediately after** the `declaim` is
//! examined. That is the dominant idiom, and it is what keeps the rule linear:
//! locating the enclosing top-level form is a binary search over
//! `root_children` (a span read per step, no allocation) and exactly one
//! neighbouring form is then materialized. A rule that scanned the file for the
//! named `defun` would make a file of N declaims cost N × file, which is the
//! shape that has twice made a single rule most of a lint run in this
//! repository.
//!
//! A `declaim` separated from its `defun` by anything at all is therefore a
//! documented false negative.
//!
//! # What this rule deliberately does not flag
//!
//! - **A declared return type that is not a literal `(values …)`.** A bare
//!   `integer`, a `*`, or a computed type says nothing this rule can check;
//!   CLHS's equivalence between `type` and `(values type)` has enough edge
//!   cases that reading only the explicit spelling is the honest scope.
//! - **A `(values …)` containing `&optional`, `&rest` or `*`.** Those make the
//!   declared arity a range, and SBCL confirms `(values integer &optional
//!   integer)` accepts a one-value return.
//! - **A function that returns via a non-literal path.** The `defun`'s last
//!   body form must literally be `(values …)`; an `if`, a `cond`, a call, or a
//!   bare expression is not something this reader will count values in.
//! - **A `declaim` that is not itself a top-level form** — one inside an
//!   `eval-when`, say — since "the next top-level form" would then mean the
//!   wrong thing.
//! - **A `values` shadowed by a local `flet`/`labels`.** Shadowing `cl:values`
//!   is a package-lock violation in every conforming implementation.
//! - **A form reached only as quoted data.**
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    bindable_variable_name, child_call, for_each_evaluated_subview, is_lambda_list_marker,
    normalized_symbol, root_child_index_containing, root_child_view,
};

#[derive(Debug, Clone)]
pub struct FtypeValuesArityMismatchItem {
    /// The span of the whole `(declaim …)` form.
    pub span: ByteSpan,
    /// The span of the `defun`'s final `(values …)` form.
    pub returned_span: ByteSpan,
    /// The function's name, normalized.
    pub function: String,
    /// How many values the `ftype` declaration names.
    pub declared_value_count: usize,
    /// How many the `defun`'s final literal `(values …)` produces.
    pub returned_value_count: usize,
}

impl Finding for FtypeValuesArityMismatchItem {
    fn kind(&self) -> &'static str {
        "ftype-values-arity-mismatch"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!(
            "{} declared={} returned={}",
            self.function, self.declared_value_count, self.returned_value_count
        )]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("function", json!(self.function)),
            ("declared_value_count", json!(self.declared_value_count)),
            ("returned_value_count", json!(self.returned_value_count)),
            (
                "returned_span",
                json!({
                    "start": self.returned_span.start().get(),
                    "end": self.returned_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "ftype declares {} returning {} values but its defun's final form returns {}; \
             the declaration is a promise the compiler optimizes against",
            self.function, self.declared_value_count, self.returned_value_count
        )
    }
}

/// The number of values a `(values …)` *type specifier* fixes, or `None` when
/// it fixes a range rather than a number.
///
/// `&optional`, `&rest` and `*` all turn the declared arity into a range, and
/// SBCL accepts a short return against every one of them.
fn fixed_declared_arity(values_type: &ExpressionView) -> Option<usize> {
    for element in values_type.children.iter().skip(1) {
        if is_lambda_list_marker(element) {
            return None;
        }
        if normalized_symbol(element).is_some_and(|text| text == "*") {
            return None;
        }
    }
    Some(values_type.children.len() - 1)
}

/// The `(values …)` return type of an `(ftype (function (args) return) …)`
/// spec, when it has one written literally.
fn declared_values_type(ftype: &ExpressionView) -> Option<&ExpressionView> {
    let function_type = child_call(ftype, 1, "function")?;
    let return_type = function_type.children.get(2)?;
    (is_paren_list(return_type)
        && return_type.reader_prefixes.is_empty()
        && list_head(return_type).is_some_and(|head| symbol_in(head, &["values"])))
    .then_some(return_type)
}

/// The number of values a `defun`'s final body form literally returns, when
/// that form is a `(values …)` call.
fn literal_final_value_count(defun: &ExpressionView) -> Option<(usize, ByteSpan)> {
    // `(defun name (params) body…)`: a body is required.
    if defun.children.len() < 4 {
        return None;
    }
    let last = defun.children.last()?;
    if !is_paren_list(last) || !last.reader_prefixes.is_empty() {
        return None;
    }
    // `is_none_or` rather than `!…is_some_and`: nix's clippy is newer than the
    // local one and rejects the negated form.
    if list_head(last).is_none_or(|head| !symbol_in(head, &["values"])) {
        return None;
    }
    Some((last.children.len() - 1, last.span))
}

/// The `defun` at `index`, if the form there defines `name`.
fn defun_named(tree: &SyntaxTree, index: usize, name: &str) -> Option<ExpressionView> {
    let view = root_child_view(tree, index)?;
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    if list_head(&view).is_none_or(|head| !symbol_in(head, &["defun"])) {
        return None;
    }
    let matches = view
        .children
        .get(1)
        .and_then(normalized_symbol)
        .is_some_and(|found| found == name);
    matches.then_some(view)
}

/// Cheapest predicate first, and the ordering is load-bearing: the head
/// comparison and the whole `ftype`/`function`/`(values …)` shape are read out
/// of the matched node alone, and only a `declaim` that survives all of that
/// pays for the binary search and the one neighbouring form.
pub fn examine(
    tree: &SyntaxTree,
    view: &ExpressionView,
    ftype_declaration_count: &mut usize,
    violations: &mut Vec<FtypeValuesArityMismatchItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["declaim"]) {
        return;
    }

    // Read every `(ftype …)` spec's shape first: all local, no tree access.
    let mut candidates: Vec<(String, usize)> = Vec::new();
    for index in 1..view.children.len() {
        let Some(ftype) = child_call(view, index, "ftype") else {
            continue;
        };
        let Some(values_type) = declared_values_type(ftype) else {
            continue;
        };
        let Some(declared) = fixed_declared_arity(values_type) else {
            continue;
        };
        for name in ftype
            .children
            .iter()
            .skip(2)
            .filter_map(bindable_variable_name)
        {
            candidates.push((name, declared));
        }
    }
    if candidates.is_empty() {
        return;
    }

    // Only now touch the tree. The declaim must itself be a top-level form, or
    // "the next top-level form" names something unrelated to it.
    let Some(index) = root_child_index_containing(tree, view.span) else {
        return;
    };
    if root_child_view(tree, index).is_none_or(|form| form.span != view.span) {
        return;
    }
    let Some(next_index) = index.checked_add(1) else {
        return;
    };
    if next_index >= tree.root_children().len() {
        return;
    }

    for (name, declared) in candidates {
        let Some(defun) = defun_named(tree, next_index, &name) else {
            continue;
        };
        let Some((returned, returned_span)) = literal_final_value_count(&defun) else {
            continue;
        };
        // Every pair this rule could have judged, whichever way it went.
        *ftype_declaration_count += 1;
        // Surplus values are conforming (the implicit `&rest t`); only a short
        // return violates the promise.
        if returned >= declared {
            continue;
        }
        violations.push(FtypeValuesArityMismatchItem {
            span: view.span,
            returned_span,
            function: name,
            declared_value_count: declared,
            returned_value_count: returned,
        });
    }
}

/// Collects every `declaim`ed `ftype` in one file whose declared return arity
/// exceeds what the `defun` below it literally returns, with the number of
/// checkable declaration/definition pairs as the denominator beside them.
pub fn build_ftype_values_arity_mismatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FtypeValuesArityMismatchItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("ftype_declaration_count", json!(0))],
        ));
    }

    let mut ftype_declaration_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(tree, subview, &mut ftype_declaration_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("ftype_declaration_count", json!(ftype_declaration_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<FtypeValuesArityMismatchItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_ftype_values_arity_mismatch_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(&tree, view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    const MISMATCH: &str = "(declaim (ftype (function (integer) (values integer integer)) f))\n\
                            (defun f (x) (values x))\n";

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_short_return() {
        let violations = report(MISMATCH).findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].function, "f");
        assert_eq!(violations[0].declared_value_count, 2);
        assert_eq!(violations[0].returned_value_count, 1);
    }

    #[test]
    fn flags_a_declared_pair_against_an_empty_values() {
        let violations = report(
            "(declaim (ftype (function () (values integer integer)) f))\n(defun f () (values))\n",
        )
        .findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].returned_value_count, 0);
    }

    #[test]
    fn flags_each_name_a_single_declaim_covers() {
        // Only the immediately following defun is examined, so a two-name
        // declaim can produce at most the one finding for that neighbour.
        let violations = report(
            "(declaim (ftype (function () (values integer integer)) f g))\n(defun f () (values 1))\n",
        )
        .findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].function, "f");
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(
            report(
                "(CL:DECLAIM (CL:FTYPE (CL:FUNCTION (integer) (CL:VALUES integer integer)) F))\n\
                 (CL:DEFUN F (x) (CL:VALUES x))\n"
            )
            .findings
            .len(),
            1
        );
    }

    // -- the direction that must never fire -----------------------------------

    /// SBCL is silent on a surplus return: `(values a b)` carries an implicit
    /// `&rest t`. Reporting it would be a guaranteed false positive.
    #[test]
    fn does_not_flag_a_surplus_return() {
        assert!(
            report("(declaim (ftype (function (integer) (values integer)) g))\n(defun g (x) (values x x x))\n")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_matching_arity() {
        assert!(
            report("(declaim (ftype (function (integer) (values integer integer)) f))\n(defun f (x) (values x x))\n")
                .findings
                .is_empty()
        );
    }

    // -- range-valued declarations --------------------------------------------

    /// SBCL accepts a one-value return against `(values integer &optional
    /// integer)`, so the declared arity is a range and nothing can be said.
    #[test]
    fn does_not_flag_an_optional_or_rest_or_star_values_type() {
        for declared in [
            "(values integer &optional integer)",
            "(values integer &rest t)",
            "(values integer *)",
        ] {
            let source = format!(
                "(declaim (ftype (function (integer) {declared}) f))\n(defun f (x) (values x))\n"
            );
            assert!(report(&source).findings.is_empty(), "{declared}");
        }
    }

    #[test]
    fn does_not_flag_a_return_type_that_is_not_a_literal_values() {
        for declared in ["integer", "*", "(or integer null)"] {
            let source = format!(
                "(declaim (ftype (function (integer) {declared}) f))\n(defun f (x) (values x))\n"
            );
            assert!(report(&source).findings.is_empty(), "{declared}");
        }
    }

    // -- the non-literal-return guard -----------------------------------------

    #[test]
    fn does_not_flag_a_function_returning_via_a_non_literal_path() {
        for body in [
            "(if p (values x) (values x x))",
            "(compute x)",
            "x",
            "(cond (p (values x)))",
        ] {
            let source = format!(
                "(declaim (ftype (function (integer) (values integer integer)) f))\n\
                 (defun f (x) {body})\n"
            );
            assert!(report(&source).findings.is_empty(), "{body}");
        }
    }

    #[test]
    fn does_not_flag_a_defun_with_no_body() {
        assert!(
            report(
                "(declaim (ftype (function (integer) (values integer integer)) f))\n(defun f (x))\n"
            )
            .findings
            .is_empty()
        );
    }

    // -- the adjacency bound --------------------------------------------------

    #[test]
    fn does_not_flag_a_defun_that_is_not_the_next_top_level_form() {
        assert!(
            report(
                "(declaim (ftype (function (integer) (values integer integer)) f))\n\
                 (defun unrelated () 1)\n\
                 (defun f (x) (values x))\n"
            )
            .findings
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_declaim_with_no_following_form() {
        assert!(
            report("(declaim (ftype (function (integer) (values integer integer)) f))\n")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_next_form_defining_a_different_name() {
        assert!(
            report(
                "(declaim (ftype (function (integer) (values integer integer)) f))\n\
                 (defun other (x) (values x))\n"
            )
            .findings
            .is_empty()
        );
    }

    /// A `declaim` nested inside another top-level form has no "next top-level
    /// form" of its own.
    #[test]
    fn does_not_flag_a_declaim_inside_an_eval_when() {
        assert!(
            report(
                "(eval-when (:compile-toplevel)\n  \
                 (declaim (ftype (function (integer) (values integer integer)) f)))\n\
                 (defun f (x) (values x))\n"
            )
            .findings
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_declaim_carrying_no_ftype() {
        assert!(
            report("(declaim (optimize (speed 3)))\n(defun f (x) (values x))\n")
                .findings
                .is_empty()
        );
        assert!(
            report("(declaim (type integer *x*))\n(defun f (x) (values x))\n")
                .findings
                .is_empty()
        );
    }

    // -- the five quote shapes ------------------------------------------------

    #[test]
    fn plain_code_fires() {
        assert!(fires(MISMATCH));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires(
            "'(declaim (ftype (function (integer) (values integer integer)) f))\n\
             (defun f (x) (values x))\n"
        ));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires(
            "(quote (declaim (ftype (function (integer) (values integer integer)) f)))\n\
             (defun f (x) (values x))\n"
        ));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires(
            "'(y ,(declaim (ftype (function (integer) (values integer integer)) f)))\n\
             (defun f (x) (values x))\n"
        ));
    }

    /// The one shape that is code again — and the declaim is then nested inside
    /// the quasiquoted form rather than top-level, so the adjacency guard makes
    /// it silent anyway. Asserting the *combination* is the point: neither the
    /// quote check nor the top-level check may be dropped.
    #[test]
    fn an_unquoted_declaim_inside_a_quasiquote_is_not_top_level() {
        assert!(!fires(
            "`(y ,(declaim (ftype (function (integer) (values integer integer)) f)))\n\
             (defun f (x) (values x))\n"
        ));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires(
            "(defmacro m () `(declaim (ftype (function () (values integer integer)) f)))\n\
             (defun f () (values 1))\n"
        ));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        let source = "(format t \"(declaim (ftype (function () (values a b)) f))\")\n\
                      (defun f () (values 1))\n";
        assert!(report(source).findings.is_empty());
        assert!(!fires(source));
    }

    // -- cost ------------------------------------------------------------------

    /// The regression that matters: a per-declaim scan of the file would make N
    /// declaims cost N × file. The budget is hundreds of times the linear cost,
    /// so only an asymptotic regression trips it.
    #[test]
    fn many_declaims_do_not_cost_the_file_each() {
        let source: String = (0..2000)
            .map(|index| {
                format!(
                    "(declaim (ftype (function (integer) (values integer integer)) f{index}))\n\
                     (defun f{index} (x) (values x))\n"
                )
            })
            .collect();
        let started = std::time::Instant::now();
        let findings = report(&source).findings;
        let elapsed = started.elapsed();
        assert_eq!(findings.len(), 2000);
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "2000 declaims took {elapsed:?}; the rule is scanning the file per declaim"
        );
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(MISMATCH, Dialect::Clojure).expect("parse");
        let report =
            build_ftype_values_arity_mismatch_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_checkable_pair_not_only_the_flagged_ones() {
        let report = report(
            "(declaim (ftype (function () (values integer integer)) a))\n\
             (defun a () (values 1))\n\
             (declaim (ftype (function () (values integer)) b))\n\
             (defun b () (values 1))\n",
        );
        assert_eq!(report.summary, vec![("ftype_declaration_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_counts() {
        let report = report(MISMATCH);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 1);
        assert_eq!(finding.kind(), "ftype-values-arity-mismatch");
        assert!(finding.text_columns()[0].contains("declared=2"));
    }
}
