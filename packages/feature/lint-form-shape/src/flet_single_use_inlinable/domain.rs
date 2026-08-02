//! Common Lisp single-use-local-function detection: an `flet`/`labels` that
//! defines one local function and whose entire body is one call to it.
//!
//! ```text
//! (flet ((double (y) (* y 2)))
//!   (double n))            ; == (let ((y n)) (* y 2))
//! ```
//!
//! The local function exists to be called once, in tail position, with no other
//! reference to its name anywhere. Naming it buys nothing the call site does
//! not already say.
//!
//! # Why the shape is this narrow
//!
//! "Called exactly once, in tail position, trivially inlinable" is three
//! judgements, and the cheapest way to be *sure* of all three is to require the
//! `flet` body to be exactly the single call. Then:
//!
//! - **Tail position** is true by construction — there is nowhere else to be.
//! - **Called exactly once** and **not recursive** and **not passed as
//!   `#'name`** all collapse into one arithmetic check: the name occurs exactly
//!   twice in the whole `flet` form, once at its definition and once as the
//!   call's head. A recursive `labels`, a `#'double` handed to `mapcar`, a
//!   second call, and a `(double …)` inside the local function's own body each
//!   add a third occurrence and each silence the rule.
//! - **Shadowing** needs no separate check for the same reason. Inside an
//!   `flet`, every `(double …)` refers to the local function, so counting
//!   occurrences within the form is counting exactly the references that matter;
//!   and a local body that calls the *global* `double` is itself a third
//!   occurrence, so the rule declines rather than guessing which one is meant.
//!
//! # What this rule deliberately does not flag
//!
//! - **More than one local function**, or an `flet` body of more than one form
//!   (which includes any body with a `declare` or a docstring).
//! - **A local function with any lambda-list keyword** (`&optional`, `&rest`,
//!   `&key`, `&aux`) or a non-symbol parameter. Binding those is not the
//!   trivial substitution the rule claims.
//! - **A call whose argument count differs from the parameter count**, which is
//!   a different defect and not this one's to report.
//! - **A local function with an empty body.**
//! - **A form reached only as quoted data.**
//!
//! Report only, emphatically. The rewrite is a `let` over the parameters, and
//! getting it right means preserving argument evaluation order, avoiding
//! capture of the argument expressions' free variables, and deciding whether
//! the author wanted the name for documentation. That is a judgement call.
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
    bindable_variable_name, count_symbol_occurrences, for_each_evaluated_subview, normalized_symbol,
};

/// The two local-function binders. `macrolet` is deliberately absent: a local
/// macro's single use is not inlinable by substitution.
pub const LOCAL_FUNCTION_HEADS: [&str; 2] = ["flet", "labels"];

#[derive(Debug, Clone)]
pub struct FletSingleUseInlinableItem {
    /// The span of the whole `(flet …)` / `(labels …)` form.
    pub span: ByteSpan,
    /// The span of the single call that is the whole body.
    pub call_span: ByteSpan,
    /// The local function's name, normalized.
    pub function: String,
    /// The binder as written, normalized — `flet` or `labels`.
    pub binder: String,
}

impl Finding for FletSingleUseInlinableItem {
    fn kind(&self) -> &'static str {
        "flet-single-use-inlinable"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.function.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("function", json!(self.function)),
            ("binder", json!(self.binder)),
            (
                "call_span",
                json!({
                    "start": self.call_span.start().get(),
                    "end": self.call_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} defines {} and its whole body is one tail call to it; \
             the local function can be inlined at the call site",
            self.binder, self.function
        )
    }
}

/// A local function definition read far enough to say its name and how many
/// arguments it takes, or `None` when its lambda list is not a plain,
/// keyword-free list of symbols.
fn simple_local_function(definition: &ExpressionView) -> Option<(String, usize)> {
    if !is_paren_list(definition) || !definition.reader_prefixes.is_empty() {
        return None;
    }
    // `(name (params…) body…)` — a body is required, or there is nothing to
    // inline.
    if definition.children.len() < 3 {
        return None;
    }
    let name = bindable_variable_name(&definition.children[0])?;
    let parameters = &definition.children[1];
    if !is_paren_list(parameters) || !parameters.reader_prefixes.is_empty() {
        return None;
    }
    // Every parameter must be a plain symbol: a `&optional`/`&rest`/`&key`
    // marker, a defaulted `(p init)` spec, or anything else means binding the
    // arguments is not a positional substitution.
    if !parameters
        .children
        .iter()
        .all(|parameter| bindable_variable_name(parameter).is_some())
    {
        return None;
    }
    Some((name, parameters.children.len()))
}

/// Examines one node. Shared with the lint suite's rule.
///
/// Cheapest predicate first: head comparison, then the binding list's *length*,
/// then the body's length, then the one definition's shape. The whole-form
/// occurrence count — the only part proportional to the form's size — runs last
/// and only once every structural check has already passed.
pub fn examine(
    view: &ExpressionView,
    single_binding_form_count: &mut usize,
    violations: &mut Vec<FletSingleUseInlinableItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &LOCAL_FUNCTION_HEADS) {
        return;
    }
    // `(flet (DEFINITION) CALL)` — exactly one definition, exactly one body
    // form. A `declare` or a docstring makes the body longer and is declined.
    if view.children.len() != 3 {
        return;
    }
    let bindings = &view.children[1];
    if !is_paren_list(bindings) || bindings.children.len() != 1 {
        return;
    }
    let Some((name, parameter_count)) = simple_local_function(&bindings.children[0]) else {
        return;
    };
    *single_binding_form_count += 1;

    // The body must be a call to the local function itself.
    let body = &view.children[2];
    if !is_paren_list(body) || !body.reader_prefixes.is_empty() {
        return;
    }
    let Some(called) = body.children.first() else {
        return;
    };
    if normalized_symbol(called).as_deref() != Some(name.as_str()) {
        return;
    }
    if body.children.len() != parameter_count + 1 {
        return;
    }
    // Definition site plus the call's head, and nothing else: no recursion, no
    // `#'name`, no second call, no call to a shadowed global.
    if count_symbol_occurrences(view, &name) != 2 {
        return;
    }

    violations.push(FletSingleUseInlinableItem {
        span: view.span,
        call_span: body.span,
        function: name,
        binder: paredit_core_syntax::view_query::unqualified(head).to_ascii_lowercase(),
    });
}

/// Collects every single-use local function in one file, with the number of
/// one-definition `flet`/`labels` forms scanned as the denominator beside them.
pub fn build_flet_single_use_inlinable_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FletSingleUseInlinableItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("single_binding_form_count", json!(0))],
        ));
    }

    let mut single_binding_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut single_binding_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![(
            "single_binding_form_count",
            json!(single_binding_form_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<FletSingleUseInlinableItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_flet_single_use_inlinable_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_single_use_flet() {
        let violations = report("(flet ((double (y) (* y 2))) (double n))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].function, "double");
        assert_eq!(violations[0].binder, "flet");
    }

    #[test]
    fn flags_a_single_use_labels() {
        let violations = report("(labels ((double (y) (* y 2))) (double n))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binder, "labels");
    }

    #[test]
    fn flags_a_nullary_local_function() {
        assert_eq!(report("(flet ((go-on () (f))) (go-on))").findings.len(), 1);
    }

    #[test]
    fn the_call_span_covers_the_call() {
        let source = "(flet ((double (y) (* y 2))) (double n))";
        let violations = report(source).findings;
        let span = violations[0].call_span;
        assert_eq!(&source[span.start().get()..span.end().get()], "(double n)");
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(
            report("(CL:FLET ((Double (y) (* y 2))) (DOUBLE n))")
                .findings
                .len(),
            1
        );
    }

    // -- the three named traps ------------------------------------------------

    /// Recursion adds a third occurrence.
    #[test]
    fn does_not_flag_a_recursive_labels() {
        assert!(
            report("(labels ((down (y) (if (zerop y) 0 (down (1- y))))) (down n))")
                .findings
                .is_empty()
        );
    }

    /// `#'name` adds a third occurrence.
    #[test]
    fn does_not_flag_a_name_passed_as_a_function_object() {
        assert!(
            report("(flet ((double (y) (* y 2))) (mapcar #'double xs))")
                .findings
                .is_empty()
        );
    }

    /// A local body that calls the shadowed global adds a third occurrence, so
    /// the rule declines rather than guessing which `double` is meant.
    #[test]
    fn does_not_flag_a_local_that_calls_the_global_it_shadows() {
        assert!(
            report("(flet ((double (y) (cl-user::double y))) (double n))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_two_calls() {
        assert!(
            report("(flet ((double (y) (* y 2))) (+ (double a) (double b)))")
                .findings
                .is_empty()
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_two_local_functions() {
        assert!(
            report("(flet ((a (y) y) (b (y) y)) (a n))")
                .findings
                .is_empty()
        );
    }

    /// The call is not in tail position: the body's single form is the `list`,
    /// not the call.
    #[test]
    fn does_not_flag_a_call_that_is_not_the_whole_body() {
        assert!(
            report("(flet ((double (y) (* y 2))) (list (double n)))")
                .findings
                .is_empty()
        );
    }

    /// The discriminating case for the one-body-form guard: the call comes
    /// *first* and something follows it, so its value is discarded and it is
    /// not in tail position at all. A guard written as "at least one body form"
    /// reads `children[2]`, finds the call, and reports — which is a false
    /// positive, and the reason the check is `!= 3` rather than `< 3`.
    #[test]
    fn does_not_flag_a_call_followed_by_another_body_form() {
        assert!(
            report("(flet ((double (y) (* y 2))) (double n) (cleanup))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_body_of_more_than_one_form() {
        assert!(
            report("(flet ((double (y) (* y 2))) (setup) (double n))")
                .findings
                .is_empty()
        );
        assert!(
            report("(flet ((double (y) (* y 2))) (declare (optimize speed)) (double n))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_lambda_list_with_keywords() {
        for lambda_list in ["(y &optional z)", "(&rest ys)", "(y &key k)", "(y &aux a)"] {
            let source = format!("(flet ((f {lambda_list} (g y))) (f n))");
            assert!(report(&source).findings.is_empty(), "{lambda_list}");
        }
    }

    #[test]
    fn does_not_flag_a_lambda_list_with_a_defaulted_parameter() {
        assert!(report("(flet ((f ((y 1)) y)) (f n))").findings.is_empty());
    }

    #[test]
    fn does_not_flag_an_argument_count_that_does_not_match() {
        assert!(
            report("(flet ((double (y) (* y 2))) (double a b))")
                .findings
                .is_empty()
        );
        assert!(
            report("(flet ((double (y) (* y 2))) (double))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_local_function_with_no_body() {
        assert!(
            report("(flet ((double (y))) (double n))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_body_that_calls_something_else() {
        assert!(
            report("(flet ((double (y) (* y 2))) (triple n))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_macrolet() {
        assert!(
            report("(macrolet ((m (y) `(* ,y 2))) (m n))")
                .findings
                .is_empty()
        );
    }

    // -- the five quote shapes ------------------------------------------------

    const CANONICAL: &str = "(flet ((double (y) (* y 2))) (double n))";

    #[test]
    fn plain_code_fires() {
        assert!(fires(CANONICAL));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires(&format!("'{CANONICAL}")));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires(&format!("(quote {CANONICAL})")));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires(&format!("'(y ,{CANONICAL})")));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert!(fires(&format!("`(y ,{CANONICAL})")));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires(&format!("(defmacro m () `{CANONICAL})")));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        let source = format!("(format t \"{CANONICAL}\")");
        assert!(report(&source).findings.is_empty());
        assert!(!fires(&source));
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CANONICAL, Dialect::EmacsLisp).expect("parse");
        let report =
            build_flet_single_use_inlinable_report(Path::new("app.el"), Dialect::EmacsLisp, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_readable_single_definition_form() {
        let report = report(
            "(flet ((a (y) y)) (a n))\n\
             (flet ((b (y) y)) (list (b n)))\n\
             (flet ((c (y) y) (d (y) y)) (c n))\n",
        );
        assert_eq!(
            report.summary,
            vec![("single_binding_form_count", json!(2))]
        );
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_function() {
        let report = report(&format!("(defun f (n)\n  {CANONICAL})\n"));
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "flet-single-use-inlinable");
        assert_eq!(finding.text_columns(), vec!["double".to_owned()]);
    }
}
