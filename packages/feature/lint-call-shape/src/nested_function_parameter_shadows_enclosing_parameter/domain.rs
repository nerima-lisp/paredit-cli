//! A local `flet`/`labels` function, or a nested `defun`, whose parameter
//! reuses the name of a parameter of the function that encloses it.
//!
//! ```lisp
//! (defun draw (window)
//!   (flet ((paint (window) (fill-background window)))
//!     (paint (main-window))))
//! ```
//!
//! Inside `paint`, `window` is *not* `draw`'s `window` — but nothing at the
//! point of use says so, and the two are one character apart in the reader's
//! head. Renaming the inner one costs nothing and removes the question.
//!
//! # Not a duplicate of the two rules next to it
//!
//! - `duplicate-parameters` is strictly *intra*-lambda-list: `(defun f (x y x))`,
//!   one lambda list naming the same variable twice, which the standard forbids
//!   outright.
//! - `inspect shadowed-bindings` tracks two binding sources — a top-level
//!   definition's own lambda list, and `let`/`let*` bindings — and its own
//!   module documentation says that "bindings introduced by
//!   `flet`/`labels`/inline `lambda` parameter lists are not tracked as scopes
//!   here". Its scope collection is top-level-only, so a nested definition is
//!   never a scope in it either.
//!
//! This rule is exactly the case both decline: two *different* lambda lists, in
//! a nesting relationship, in the same function.
//!
//! # The guard that keeps the recursive-helper idiom quiet
//!
//! ```lisp
//! (defun reverse-tree (tree)
//!   (labels ((walk (tree acc) …))
//!     (walk tree nil)))
//! ```
//!
//! Here the inner `tree` is *deliberately* the outer `tree`: the helper is
//! called with it, in that very position, and the reused name is what says so.
//! Reporting this would report the single most common shape in recursive Common
//! Lisp, so it is suppressed whenever the enclosing form contains either
//!
//! - a call `(inner outer-name …)` passing the shadowed variable in the
//!   shadowing parameter's own position, or
//! - any *reference* to the local function that is not a direct call — `#'walk`,
//!   `'walk`, `(function walk)` — because then its call sites cannot all be
//!   seen, and reporting on the ones that can is reporting on a guess.
//!
//! # What this rule does not attempt
//!
//! - It says nothing about an inline `(lambda (x) …)` reusing an enclosing
//!   parameter name. That is common enough in `mapcar`-style code to be its own
//!   judgement, and this rule keeps to the definition forms it was asked for.
//! - It says nothing about a parameter shadowing a `let` binding, a global, or
//!   a package-level name. Those are `shadowed-bindings` and
//!   `package-level-shadowing`.
//! - It does not read a destructuring pattern as a name.
//! - Scope is Common Lisp only: `flet`, `labels` and the `&`-keyword lambda-list
//!   vocabulary are the CLHS ones.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    DescentStep, count_occurrences_ignoring_case, definition_lambda_list, descend_to,
    descent_is_unevaluated, is_top_level, normalized_symbol, required_parameter_name,
    required_parameters, root_child_containing, root_child_span_containing,
};

/// The definition forms that establish an enclosing parameter scope on the path
/// down to a nested definition.
const ENCLOSING_DEFINITION_HEADS: &[&str] = &["defun", "defmacro", "defmethod", "lambda"];

/// The forms whose child 1 is a list of local function definitions.
const LOCAL_FUNCTION_FORMS: &[&str] = &["flet", "labels"];

/// One reported shadowing.
#[derive(Debug, Clone)]
pub struct NestedParameterShadowItem {
    /// The span of the *inner* parameter — the one to rename.
    pub span: ByteSpan,
    /// The parameter name, normalized.
    pub parameter: String,
    /// The nested function that declares it.
    pub inner_function: String,
    /// The enclosing function whose parameter it hides.
    pub outer_function: String,
}

impl Finding for NestedParameterShadowItem {
    fn kind(&self) -> &'static str {
        "nested-function-parameter-shadows-enclosing-parameter"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("parameter={}", self.parameter)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("parameter", json!(self.parameter)),
            ("inner_function", json!(self.inner_function)),
            ("outer_function", json!(self.outer_function)),
        ]
    }

    fn message(&self) -> String {
        message(&self.parameter, &self.inner_function, &self.outer_function)
    }
}

/// The one sentence both the report and the lint rule print.
#[must_use]
pub fn message(parameter: &str, inner_function: &str, outer_function: &str) -> String {
    format!(
        "parameter `{parameter}` of `{inner_function}` hides the `{parameter}` parameter of the \
         enclosing `{outer_function}`; inside `{inner_function}` the name no longer means what a \
         reader of `{outer_function}` expects"
    )
}

/// One function's required parameters, in order.
#[derive(Debug, Clone)]
struct ParameterScope {
    function: String,
    /// `(name, required position)` for each parameter this module can name.
    parameters: Vec<(String, usize)>,
}

impl ParameterScope {
    fn names(&self) -> impl Iterator<Item = &str> {
        self.parameters.iter().map(|(name, _)| name.as_str())
    }
}

/// Reads the parameter scope of a `defun`/`defmacro`/`defmethod`/`lambda`.
fn definition_scope(view: &ExpressionView) -> Option<ParameterScope> {
    let head = list_head(view)?;
    if !symbol_in(head, ENCLOSING_DEFINITION_HEADS) {
        return None;
    }
    let specialized = symbol_in(head, &["defmethod"]);
    let lambda_list = definition_lambda_list(view, head)?;
    let function = if symbol_in(head, &["lambda"]) {
        "lambda".to_owned()
    } else {
        view.children
            .get(1)
            .and_then(normalized_symbol)
            .unwrap_or_else(|| "the enclosing definition".to_owned())
    };
    Some(ParameterScope {
        function,
        parameters: named_parameters(lambda_list, specialized),
    })
}

/// Reads the parameter scope of one `(name (params…) body…)` local definition.
fn local_definition_scope(view: &ExpressionView) -> Option<ParameterScope> {
    if !is_paren_list(view) || view.children.len() < 2 {
        return None;
    }
    let function = normalized_symbol(&view.children[0])?;
    let lambda_list = &view.children[1];
    if !is_paren_list(lambda_list) {
        return None;
    }
    Some(ParameterScope {
        function,
        parameters: named_parameters(lambda_list, false),
    })
}

/// The required parameters of a lambda list that this module can name, with
/// their positions. A destructuring pattern contributes no name and no gap:
/// the position is the parameter's index in the required list either way.
fn named_parameters(lambda_list: &ExpressionView, specialized: bool) -> Vec<(String, usize)> {
    required_parameters(lambda_list)
        .into_iter()
        .enumerate()
        .filter_map(|(position, parameter)| {
            required_parameter_name(parameter, specialized).map(|name| (name, position))
        })
        .collect()
}

/// Whether the node at `index` of the descent is a local function definition —
/// that is, sits directly inside an `flet`/`labels` binding list.
fn is_local_definition(steps: &[DescentStep<'_>], index: usize) -> bool {
    index >= 2
        && steps[index - 2].next_child == Some(1)
        && list_head(steps[index - 2].view)
            .is_some_and(|head| symbol_in(head, LOCAL_FUNCTION_FORMS))
}

/// Every parameter scope enclosing the last step of `steps`, outermost first.
fn enclosing_scopes(steps: &[DescentStep<'_>]) -> Vec<ParameterScope> {
    let mut scopes = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        if index + 1 == steps.len() {
            break;
        }
        if let Some(scope) = definition_scope(step.view) {
            scopes.push(scope);
        } else if is_local_definition(steps, index) {
            if let Some(scope) = local_definition_scope(step.view) {
                scopes.push(scope);
            }
        }
    }
    scopes
}

/// Whether the enclosing form shows that the reused name is *deliberate*.
///
/// Two ways: a call to the nested function passing the shadowed variable in the
/// shadowing parameter's own position, or any reference to the nested function
/// that is not a direct call, which means its call sites cannot all be seen.
///
/// Reads the one already-materialized top-level form, and only for a candidate
/// that would otherwise be reported.
fn shadowing_is_deliberate(
    top_level: &ExpressionView,
    function: &str,
    parameter: &str,
    position: usize,
) -> bool {
    let mut deliberate = false;
    for_each_subview(top_level, |node| {
        if deliberate {
            return;
        }
        // `#'walk`, `'walk`: the function escapes, so its call sites are not
        // all visible here.
        if node.children.is_empty()
            && !node.reader_prefixes.is_empty()
            && normalized_symbol(node).as_deref() == Some(function)
        {
            deliberate = true;
            return;
        }
        let Some(head) = list_head(node) else {
            return;
        };
        // `(function walk)`.
        if symbol_in(head, &["function"])
            && node.children.get(1).and_then(normalized_symbol).as_deref() == Some(function)
        {
            deliberate = true;
            return;
        }
        // `(walk tree nil)` with `tree` in the shadowing parameter's position.
        if symbol_in(head, &[function])
            && node
                .children
                .get(position + 1)
                .and_then(normalized_symbol)
                .as_deref()
                == Some(parameter)
        {
            deliberate = true;
        }
    });
    deliberate
}

/// The nested definitions a matched node declares, and the denominator it
/// contributes.
fn nested_definitions(view: &ExpressionView, head: &str) -> Vec<ParameterScope> {
    if symbol_in(head, LOCAL_FUNCTION_FORMS) {
        return view
            .children
            .get(1)
            .map(|bindings| {
                bindings
                    .children
                    .iter()
                    .filter_map(local_definition_scope)
                    .collect()
            })
            .unwrap_or_default();
    }
    definition_scope(view).into_iter().collect()
}

/// The inner parameter node whose span a finding points at.
fn parameter_span(
    view: &ExpressionView,
    head: &str,
    function: &str,
    position: usize,
) -> Option<ByteSpan> {
    let lambda_list = if symbol_in(head, LOCAL_FUNCTION_FORMS) {
        view.children
            .get(1)?
            .children
            .iter()
            .find(|definition| {
                definition
                    .children
                    .first()
                    .and_then(normalized_symbol)
                    .as_deref()
                    == Some(function)
            })?
            .children
            .get(1)?
    } else {
        definition_lambda_list(view, head)?
    };
    required_parameters(lambda_list)
        .get(position)
        .map(|parameter| parameter.span)
}

pub fn examine_nested_definition(
    tree: &SyntaxTree,
    view: &ExpressionView,
    nested_definition_count: &mut usize,
    violations: &mut Vec<NestedParameterShadowItem>,
) {
    if !is_paren_list(view) {
        return;
    }
    let Some(head) = list_head(view) else {
        return;
    };
    let is_local_form = symbol_in(head, LOCAL_FUNCTION_FORMS);
    if !is_local_form && !symbol_in(head, &["defun"]) {
        return;
    }
    // A top-level `defun` is not a nested definition. Answering that costs a
    // binary search over the top level and allocates nothing, which is what
    // keeps a file of ordinary definitions from materializing anything.
    if !is_local_form && is_top_level(tree, view.span) {
        return;
    }

    let inners = nested_definitions(view, head);
    *nested_definition_count += inners.len();
    if inners.is_empty() {
        return;
    }

    // A name can only shadow something declared *outside* this form, so it has
    // to be spelled outside it. Counting that in the bytes of the enclosing
    // top-level form costs a fraction of building that form's
    // `ExpressionView`, and rules out every `flet` whose parameters are its
    // own — which is nearly all of them. A mention in a string or a comment
    // clears the guard, which is the harmless direction: the descent then runs
    // and answers as it would have anyway.
    let Some(top_span) = root_child_span_containing(tree, view.span) else {
        return;
    };
    let top_slice = top_span.slice(tree.source());
    let own_slice = view.span.slice(tree.source());
    let named_outside = |name: &str| {
        count_occurrences_ignoring_case(top_slice, name)
            > count_occurrences_ignoring_case(own_slice, name)
    };
    if !inners.iter().any(|inner| {
        inner
            .parameters
            .iter()
            .any(|(name, _)| named_outside(name.as_str()))
    }) {
        return;
    }

    let Some(top_level) = root_child_containing(tree, view.span) else {
        return;
    };
    let steps = descend_to(&top_level, view.span);
    if steps.last().is_none_or(|last| last.view.span != view.span) {
        return;
    }
    if descent_is_unevaluated(&steps) {
        return;
    }
    let outer_scopes = enclosing_scopes(&steps);
    if outer_scopes.is_empty() {
        return;
    }

    for inner in &inners {
        for (parameter, position) in &inner.parameters {
            // Innermost enclosing scope wins: that is the binding the reader
            // would otherwise have expected the name to mean.
            let Some(outer) = outer_scopes
                .iter()
                .rev()
                .find(|scope| scope.names().any(|name| name == parameter))
            else {
                continue;
            };
            if shadowing_is_deliberate(&top_level, &inner.function, parameter, *position) {
                continue;
            }
            let Some(span) = parameter_span(view, head, &inner.function, *position) else {
                continue;
            };
            violations.push(NestedParameterShadowItem {
                span,
                parameter: parameter.clone(),
                inner_function: inner.function.clone(),
                outer_function: outer.function.clone(),
            });
        }
    }
}

/// Collects every nested-definition parameter shadowing in one file, with the
/// number of nested definitions scanned as the denominator beside them.
pub fn build_nested_parameter_shadow_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedParameterShadowItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("nested_definition_count", json!(0))],
        ));
    }

    let mut nested_definition_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_nested_definition(tree, subview, &mut nested_definition_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("nested_definition_count", json!(nested_definition_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NestedParameterShadowItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_parameter_shadow_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<NestedParameterShadowItem> {
        report(input).findings
    }

    fn shadowed(input: &str) -> Vec<String> {
        findings(input)
            .iter()
            .map(|item| item.parameter.clone())
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_an_flet_parameter_hiding_an_enclosing_one() {
        let items = findings(
            "(defun draw (window)\n  (flet ((paint (window) (fill-background window)))\n    (paint (main-window))))",
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].parameter, "window");
        assert_eq!(items[0].inner_function, "paint");
        assert_eq!(items[0].outer_function, "draw");
    }

    #[test]
    fn flags_a_labels_parameter_the_same_way() {
        assert_eq!(
            shadowed(
                "(defun draw (window)\n  (labels ((paint (window) (fill window)))\n    (paint (main-window))))"
            ),
            ["window"]
        );
    }

    #[test]
    fn flags_a_nested_defun_parameter() {
        let items = findings("(defun outer (x)\n  (defun inner (x) (* x 2))\n  (inner (compute)))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].inner_function, "inner");
        assert_eq!(items[0].outer_function, "outer");
    }

    #[test]
    fn the_reported_span_is_the_inner_parameter() {
        let source =
            "(defun draw (window)\n  (flet ((paint (window) (fill window)))\n    (paint (main))))";
        let items = findings(source);
        let span = items[0].span;
        assert_eq!(span.slice(source), "window");
        // The *inner* one: it starts after the outer lambda list ends.
        assert!(span.start().get() > source.find("(window)").expect("outer list") + 1);
    }

    #[test]
    fn an_flet_inside_an_flet_reports_against_the_innermost_enclosing_scope() {
        let items = findings(
            "(defun top (v)\n  (flet ((middle (v) (flet ((inner (v) v)) (inner (compute)))))\n    (middle (start))))",
        );
        // `middle`'s `v` hides `top`'s; `inner`'s `v` hides `middle`'s.
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.outer_function == "top"));
        assert!(items.iter().any(|item| item.outer_function == "middle"));
    }

    #[test]
    fn a_lambda_parameter_can_be_the_enclosing_scope() {
        assert_eq!(
            shadowed(
                "(defvar *f*\n  (lambda (row)\n    (flet ((clean (row) (trim row)))\n      (clean (fetch)))))"
            ),
            ["row"]
        );
    }

    #[test]
    fn a_defmethod_specializer_parameter_counts_as_an_enclosing_scope() {
        assert_eq!(
            shadowed(
                "(defmethod draw ((window frame))\n  (flet ((paint (window) window))\n    (paint (main))))"
            ),
            ["window"]
        );
    }

    #[test]
    fn only_the_shadowing_parameter_is_reported() {
        let items =
            findings("(defun f (a b)\n  (flet ((g (b c) (list b c)))\n    (g (compute) a)))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].parameter, "b");
    }

    // -- the guard that keeps the recursive-helper idiom quiet ---------------

    #[test]
    fn an_aliasing_call_makes_the_reuse_deliberate() {
        assert!(
            findings("(defun reverse-tree (tree)\n  (labels ((walk (tree acc) (list tree acc)))\n    (walk tree nil)))")
                .is_empty()
        );
    }

    #[test]
    fn an_alias_in_the_wrong_position_is_still_reported() {
        // `walk`'s parameter 0 is `tree`, but the call passes `tree` second.
        assert_eq!(
            shadowed(
                "(defun reverse-tree (tree)\n  (labels ((walk (tree acc) (list tree acc)))\n    (walk nil tree)))"
            ),
            ["tree"]
        );
    }

    #[test]
    fn a_functional_reference_suppresses_the_finding() {
        for reference in ["#'walk", "(function walk)", "'walk"] {
            assert!(
                findings(&format!(
                    "(defun each (tree)\n  (labels ((walk (tree) tree))\n    (mapcar {reference} (children))))"
                ))
                .is_empty(),
                "{reference} escapes, so the call sites cannot all be seen"
            );
        }
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn a_different_parameter_name_is_not_shadowing() {
        assert!(
            findings(
                "(defun draw (window)\n  (flet ((paint (canvas) canvas))\n    (paint (main))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_top_level_defun_has_no_enclosing_scope() {
        assert!(findings("(defun f (x) x)\n(defun g (x) x)\n").is_empty());
    }

    #[test]
    fn an_flet_outside_any_definition_has_no_enclosing_scope() {
        assert!(findings("(flet ((g (x) x)) (g 1))").is_empty());
    }

    /// `duplicate-parameters`' subject, not this rule's.
    #[test]
    fn a_lambda_list_repeating_a_name_is_a_different_rule() {
        assert!(findings("(defun f (x y x) x)").is_empty());
    }

    /// `shadowed-bindings`' subject, not this rule's.
    #[test]
    fn a_let_binding_hiding_a_parameter_is_a_different_rule() {
        assert!(findings("(defun f (x) (let ((x 1)) x))").is_empty());
    }

    /// An inline lambda is deliberately out of scope.
    #[test]
    fn an_inline_lambda_reusing_a_parameter_name_is_not_reported() {
        assert!(findings("(defun f (x) (mapcar (lambda (x) (* x 2)) xs))").is_empty());
    }

    #[test]
    fn a_destructuring_pattern_is_not_read_as_a_name() {
        assert!(findings("(defmacro m ((a b))\n  (flet ((g ((a b)) a))\n    (g nil)))").is_empty());
    }

    /// A realistic, correct file.
    #[test]
    fn idiomatic_code_is_silent() {
        let source = "(defun flatten (tree)\n  (labels ((walk (tree acc)\n             (cond ((null tree) acc)\n                   ((atom tree) (cons tree acc))\n                   (t (walk (car tree) (walk (cdr tree) acc))))))\n    (walk tree nil)))\n\n\
             (defun render (window canvas)\n  (flet ((clear (target) (fill-rectangle target 0 0))\n         (present (target) (swap-buffers target)))\n    (clear canvas)\n    (draw window canvas)\n    (present canvas)))\n\n\
             (defun tally (rows)\n  (let ((total 0))\n    (dolist (row rows total)\n      (incf total (row-value row)))))\n";
        assert!(findings(source).is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    const SHADOWING: &str = "(defun draw (window) (flet ((paint (window) window)) (paint (main))))";

    #[test]
    fn a_hard_quoted_definition_is_data() {
        assert!(findings(&format!("'{SHADOWING}")).is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings(&format!("(quote {SHADOWING})")).is_empty());
    }

    #[test]
    fn a_quasiquoted_definition_without_an_unquote_is_data() {
        assert!(findings(&format!("`{SHADOWING}")).is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(findings(&format!("'(x ,{SHADOWING})")).is_empty());
    }

    #[test]
    fn an_unquoted_definition_inside_a_quasiquote_is_code_again() {
        assert_eq!(findings(&format!("`(x ,{SHADOWING})")).len(), 1);
    }

    #[test]
    fn a_definition_spelled_only_inside_a_string_is_never_a_form() {
        assert!(findings("(defun f () (format nil \"(flet ((g (x) x)) x)\"))").is_empty());
    }

    // -- dialects and denominators -------------------------------------------

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(SHADOWING, Dialect::EmacsLisp).expect("parse");
        let report =
            build_nested_parameter_shadow_report(Path::new("t.el"), Dialect::EmacsLisp, &tree)
                .expect("report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_nested_definition_scanned() {
        let report =
            report("(defun f (a)\n  (flet ((g (x) x) (h (y) y))\n    (list (g 1) (h 2))))");
        assert_eq!(report.summary, vec![("nested_definition_count", json!(2))]);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_parameter() {
        let report =
            report("(defun draw (window)\n  (flet ((paint (window) window))\n    (paint (main))))");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "nested-function-parameter-shadows-enclosing-parameter"
        );
        assert_eq!(finding.text_columns(), vec!["parameter=window".to_owned()]);
        assert!(finding.message().contains("hides the `window` parameter"));
    }
}
