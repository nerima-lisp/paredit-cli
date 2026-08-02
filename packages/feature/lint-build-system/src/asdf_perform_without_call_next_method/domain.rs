//! A primary `asdf:perform` method on `load-op`/`compile-op` that replaces the
//! standard method instead of extending it.
//!
//! `perform` is where ASDF actually compiles and loads a component. A *primary*
//! method — one with no qualifier — does not run alongside the standard method,
//! it runs **instead of** it, and only `call-next-method` gets the standard
//! behaviour back. So
//!
//! ```lisp
//! (defmethod perform ((op compile-op) (c cl-source-file))
//!   (my-instrumented-compile c))
//! ```
//!
//! removes ASDF's own compilation of every Lisp source file in the image,
//! including its output-file bookkeeping and its `*compile-file-warnings*`
//! handling, for every system built afterwards. The method composes with
//! nothing.
//!
//! # Scope, and why it is this narrow
//!
//! Overriding `perform` is a normal, supported thing to do. Nearly every use of
//! it is correct, and the rule is built to stay silent on all of them:
//!
//! - **Qualified methods are excluded entirely.** `:before` and `:after`
//!   methods must *not* call `call-next-method` — doing so is undefined
//!   behaviour — so requiring it there would be wrong. `:around` methods
//!   genuinely should, and that is already
//!   `paredit-feature-lint-object-system`'s
//!   `around-method-missing-call-next-method`, which is generic-agnostic and
//!   qualifier-gated to `:around`. Skipping every qualifier keeps this rule
//!   from emitting a second finding on the same form.
//! - **Only `load-op` and `compile-op`.** `test-op` is the operation almost
//!   every `.asd` in the wild specializes, and its standard method deliberately
//!   does nothing, so replacing it is the entire point.
//! - **Only the standard component classes whose `perform` does the work**:
//!   `component`, `source-file`, `file-component`, `cl-source-file`. A method on
//!   a *user-defined* component class — `my-grovel-file`, `protobuf-source-file`
//!   — is how ASDF is meant to be extended: there is no interesting next method
//!   to call, and flagging it would fire on every extension in the ecosystem.
//!   `static-file` is excluded for the same reason in the other direction: its
//!   standard `perform` is already a no-op.
//! - **An `(eql …)` specializer is not a class** and is skipped. It names one
//!   specific system, which is the `.asd`-local idiom
//!   `(defmethod perform ((o test-op) (c (eql (find-system "app")))) …)`.
//! - **An unspecialized parameter is skipped.** `(defmethod perform ((o load-op) c) …)`
//!   specializes the component on `t`, which is broader still, but "the author
//!   wrote no specializer" is too weak a signal to report on.
//!
//! # Further deliberate limits
//!
//! - `call-next-method` is looked for **anywhere in evaluated body code**,
//!   including inside a conditional that may not run. A method that calls it on
//!   one branch composes; deciding whether that branch is reachable is not this
//!   rule's question.
//! - **No fix.** Where in the body the call belongs, and whether its value
//!   should be returned, is a human decision.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, symbol_name};

#[derive(Debug, Clone)]
pub struct AsdfPerformWithoutCallNextMethodItem {
    /// The span of the whole `(defmethod perform …)` form: the missing call
    /// could go anywhere in its body, so no narrower span is the place it is
    /// missing from.
    pub span: ByteSpan,
    /// The operation class the method specializes on.
    pub operation: String,
    /// The component class it specializes on.
    pub component: String,
}

impl Finding for AsdfPerformWithoutCallNextMethodItem {
    fn kind(&self) -> &'static str {
        "asdf-perform-without-call-next-method"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operation={}", self.operation),
            format!("component={}", self.component),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operation", json!(self.operation)),
            ("component", json!(self.component)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "primary perform method on ({} {}) never calls call-next-method; it replaces \
             ASDF's standard {} of every such component rather than extending it",
            self.operation, self.component, self.operation
        )
    }
}

/// The generic function this rule is about.
const PERFORM: &str = "perform";

/// The local function whose absence is the finding.
const CALL_NEXT_METHOD: &str = "call-next-method";

/// The two ASDF operations whose standard `perform` does the build work.
///
/// `test-op` is deliberately absent: its standard method is a no-op, so
/// replacing it is the documented way to declare how a system is tested, and it
/// is what nearly every `.asd` in the wild does.
const BUILD_OPERATIONS: [&str; 2] = ["load-op", "compile-op"];

/// The standard ASDF component classes whose `perform` for those operations
/// does real work, and therefore the only ones where replacing it silently
/// removes behaviour.
///
/// `static-file` is excluded: its standard `perform` already does nothing.
/// User-defined component classes are excluded by construction — a class not in
/// this list is somebody's extension point, not ASDF's.
const STANDARD_COMPONENT_CLASSES: [&str; 4] = [
    "component",
    "source-file",
    "file-component",
    "cl-source-file",
];

/// The **ASDF** class a required parameter specializes on, or `None`.
///
/// `(op load-op)` and `(op asdf:load-op)` both answer `load-op`. Three shapes
/// answer `None`, each for its own reason:
///
/// - a bare `op` — no specializer at all, i.e. `t`;
/// - `(c (eql (find-system "x")))` — a specializer *form*, not a named class;
/// - `(c my-lib:source-file)` — a class in somebody else's package.
///
/// That last one is not hypothetical. Class names are compared in the
/// normalized spelling, which strips the package qualifier so that
/// `asdf:cl-source-file` and `cl-source-file` are one class — and stripping it
/// unconditionally also made a *user's* class named `source-file` or
/// `component` match ASDF's. An adversarial review found exactly that:
/// `(defmethod perform ((o compile-op) (c my-lib:source-file)) …)` was flagged
/// as though it specialized ASDF's. Requiring the qualifier to be absent or to
/// name ASDF's own packages closes it without losing the qualified spelling
/// this rule has to accept.
fn specializer(parameter: &ExpressionView) -> Option<String> {
    // Two shapes are rejected by this one line rather than by guards of their
    // own, and mutation testing is why: an explicit `is_paren_list(parameter)`
    // check and an explicit `is_paren_list(specializer)` check both used to sit
    // here, and removing either changed no behaviour and broke no test.
    //
    // - A bare `c` (unspecialized, i.e. `t`) is an atom, and an atom has no
    //   `children[1]`.
    // - `(c (eql (find-system "x")))` specializes on a *form*, and a list has
    //   no atom text.
    let raw = atom_text(parameter.children.get(1)?)?;
    if !names_an_asdf_package(raw) {
        return None;
    }
    symbol_name(parameter.children.get(1)?)
}

/// Whether a symbol's package qualifier, if it has one, is ASDF's.
///
/// Unqualified counts: a `.asd` file is read in `ASDF-USER`, which uses `ASDF`,
/// so `cl-source-file` there *is* `asdf:cl-source-file`. `asdf:`, `asdf::` and
/// the internal `asdf/…:` package family all count. Anything else is another
/// library's class that happens to share a common word.
fn names_an_asdf_package(symbol: &str) -> bool {
    let Some((qualifier, _)) = symbol.rsplit_once(':') else {
        return true;
    };
    let qualifier = qualifier.trim_end_matches(':').to_ascii_lowercase();
    qualifier.is_empty()
        || qualifier == "asdf"
        || qualifier == "uiop"
        || qualifier.starts_with("asdf/")
}

/// The lambda list of a `defmethod`, and how many qualifiers preceded it.
///
/// CLOS qualifiers are non-list objects, so the first `(…)` after the generic
/// function's name is the lambda list and everything between is a qualifier.
fn method_lambda_list(view: &ExpressionView) -> Option<(usize, &ExpressionView)> {
    let mut index = 2;
    while let Some(child) = view.children.get(index) {
        if is_paren_list(child) {
            return Some((index - 2, child));
        }
        index += 1;
    }
    None
}

/// Whether any evaluated form in `body` mentions `call-next-method`, as a call
/// or as a `#'call-next-method` reference.
fn calls_next_method(body: &[ExpressionView]) -> bool {
    let mut found = false;
    for form in body {
        for_each_evaluated_subview(form, |view| {
            if !found && symbol_name(view).is_some_and(|name| name == CALL_NEXT_METHOD) {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// `perform_method_count` counts only the methods this rule has an opinion
/// about — primary `perform` methods on a build operation and a standard
/// component class. A `test-op` method or one on a user-defined class is not in
/// the denominator, because a rate that included them would be a rate over a
/// population this rule makes no claim about.
pub fn examine_defmethod(
    view: &ExpressionView,
    perform_method_count: &mut usize,
    violations: &mut Vec<AsdfPerformWithoutCallNextMethodItem>,
) {
    if !is_paren_list(view) || list_head(view).is_none_or(|head| !symbol_is(head, "defmethod")) {
        return;
    }
    let Some(name) = view.children.get(1) else {
        return;
    };
    if symbol_name(name).is_none_or(|text| text != PERFORM) {
        return;
    }
    let Some((qualifier_count, lambda_list)) = method_lambda_list(view) else {
        return;
    };
    // `:before`/`:after` must not call it; `:around` is
    // `around-method-missing-call-next-method`'s subject.
    if qualifier_count > 0 {
        return;
    }
    let Some(operation) = lambda_list.children.first().and_then(specializer) else {
        return;
    };
    if !BUILD_OPERATIONS.contains(&operation.as_str()) {
        return;
    }
    let Some(component) = lambda_list.children.get(1).and_then(specializer) else {
        return;
    };
    if !STANDARD_COMPONENT_CLASSES.contains(&component.as_str()) {
        return;
    }
    *perform_method_count += 1;

    // Everything after the lambda list: declarations, a docstring and the body.
    // Searching all of it is harmless — neither a declaration nor a docstring
    // can contain a `call-next-method` form — and avoids a second guess about
    // where the body starts.
    let body = view.children.get(qualifier_count + 3..).unwrap_or_default();
    if calls_next_method(body) {
        return;
    }
    violations.push(AsdfPerformWithoutCallNextMethodItem {
        span: view.span,
        operation,
        component,
    });
}

/// The operations this rule reads, for tests that must not restate them.
#[must_use]
pub const fn build_operations() -> [&'static str; 2] {
    BUILD_OPERATIONS
}

/// The component classes this rule reads, for tests that must not restate them.
#[must_use]
pub const fn standard_component_classes() -> [&'static str; 4] {
    STANDARD_COMPONENT_CLASSES
}

/// Collects every non-composing primary `perform` method in one file, with the
/// number of in-scope `perform` methods scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every such method composes" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_asdf_perform_without_call_next_method_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AsdfPerformWithoutCallNextMethodItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("perform_method_count", json!(0))],
        ));
    }

    let mut perform_method_count = 0;
    let mut violations = Vec::new();
    for_each_evaluated_subview(&tree.root_view(), |view| {
        examine_defmethod(view, &mut perform_method_count, &mut violations);
    });

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("perform_method_count", json!(perform_method_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AsdfPerformWithoutCallNextMethodItem> {
        // `parse_with_dialect`, never the legacy `SyntaxTree::parse`.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_asdf_perform_without_call_next_method_report(
            Path::new("app.asd"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build asdf-perform-without-call-next-method report")
    }

    fn methods(input: &str) -> (u64, Vec<AsdfPerformWithoutCallNextMethodItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "perform_method_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("perform_method_count in the summary");
        (count, report.findings)
    }

    // --- positive

    #[test]
    fn flags_a_primary_perform_that_replaces_the_standard_compilation() {
        let (count, violations) = methods(
            "(defmethod perform ((op compile-op) (c cl-source-file))\n\
             \x20 (my-instrumented-compile c))\n",
        );
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operation, "compile-op");
        assert_eq!(violations[0].component, "cl-source-file");
    }

    #[test]
    fn flags_both_build_operations_on_every_standard_component_class() {
        for operation in build_operations() {
            for component in standard_component_classes() {
                let (_, violations) = methods(&format!(
                    "(defmethod perform ((op {operation}) (c {component})) (do-it c))"
                ));
                assert_eq!(
                    violations.len(),
                    1,
                    "not flagged for ({operation} {component})"
                );
            }
        }
    }

    #[test]
    fn flags_the_qualified_spellings_of_perform_and_its_specializers() {
        let (_, violations) = methods(
            "(defmethod asdf:perform ((op asdf:load-op) (c asdf:cl-source-file)) (do-it c))",
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operation, "load-op");
    }

    // --- near-miss negatives

    #[test]
    fn does_not_flag_a_method_that_calls_the_next_method() {
        let (count, violations) = methods(
            "(defmethod perform ((op compile-op) (c cl-source-file))\n\
             \x20 (note-start c)\n\
             \x20 (call-next-method))\n",
        );
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_call_on_one_branch_is_enough() {
        let (_, violations) = methods(
            "(defmethod perform ((op load-op) (c cl-source-file))\n\
             \x20 (if (skip-p c) nil (call-next-method)))\n",
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn a_function_reference_to_call_next_method_counts() {
        let (_, violations) = methods(
            "(defmethod perform ((op load-op) (c cl-source-file))\n\
             \x20 (funcall #'call-next-method))\n",
        );
        assert!(violations.is_empty());
    }

    /// `:before`/`:after` methods must *not* call it, and `:around` is
    /// `around-method-missing-call-next-method`'s subject.
    #[test]
    fn does_not_flag_a_qualified_method() {
        for qualifier in [":before", ":after", ":around"] {
            let (count, violations) = methods(&format!(
                "(defmethod perform {qualifier} ((op load-op) (c cl-source-file)) (log-it c))"
            ));
            assert_eq!(count, 0, "counted a {qualifier} method");
            assert!(violations.is_empty(), "flagged a {qualifier} method");
        }
    }

    /// The operation restriction has to be load-bearing on its own, not only
    /// in combination with the component restriction. Mutation testing caught
    /// this: with only the `(eql …)`-specialized `test-op` case below, removing
    /// the `BUILD_OPERATIONS` check broke no test, because the component check
    /// was already rejecting that input.
    #[test]
    fn does_not_flag_a_non_build_operation_on_a_standard_component_class() {
        for operation in ["test-op", "load-source-op", "prepare-op", "process-op"] {
            let (count, violations) = methods(&format!(
                "(defmethod perform ((o {operation}) (c cl-source-file)) (do-it c))"
            ));
            assert_eq!(count, 0, "counted a {operation} method");
            assert!(violations.is_empty(), "flagged a {operation} method");
        }
    }

    /// The single most common `perform` method in the ecosystem. Its standard
    /// method does nothing, so replacing it is the point.
    #[test]
    fn does_not_flag_a_test_op_method_on_a_specific_system() {
        let (count, violations) = methods(
            "(defmethod perform ((o test-op) (c (eql (find-system \"app\"))))\n\
             \x20 (symbol-call :fiveam :run! :app-tests))\n",
        );
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// Extending ASDF with a component class of your own is exactly how it is
    /// meant to be extended; there is no interesting next method to call.
    #[test]
    fn does_not_flag_a_method_on_a_user_defined_component_class() {
        let (count, violations) = methods(
            "(defclass grovel-file (cl-source-file) ())\n\
             (defmethod perform ((op compile-op) (c grovel-file))\n\
             \x20 (run-groveller c))\n",
        );
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// FP-4, from the adversarial review. Class names are compared in the
    /// normalized spelling so that `asdf:cl-source-file` matches
    /// `cl-source-file` — and that stripping made a *user's* class named
    /// `source-file` or `component` match ASDF's too. These are the exact
    /// inputs that were wrongly flagged.
    #[test]
    fn does_not_flag_a_component_class_from_another_librarys_package() {
        for parameter in [
            "my-lib:source-file",
            "my-build::component",
            "protobuf:module",
            "grovel:cl-source-file",
        ] {
            let (count, violations) = methods(&format!(
                "(defmethod perform ((o compile-op) (c {parameter})) (do-it c))"
            ));
            assert_eq!(count, 0, "counted foreign class `{parameter}`");
            assert!(violations.is_empty(), "flagged foreign class `{parameter}`");
        }
    }

    #[test]
    fn still_accepts_asdfs_own_qualified_spellings() {
        for parameter in [
            "cl-source-file",
            "asdf:cl-source-file",
            "asdf::cl-source-file",
            "asdf/component:component",
        ] {
            let (count, violations) = methods(&format!(
                "(defmethod perform ((o compile-op) (c {parameter})) (do-it c))"
            ));
            assert_eq!(count, 1, "did not check ASDF class `{parameter}`");
            assert_eq!(violations.len(), 1, "did not flag ASDF class `{parameter}`");
        }
    }

    /// The same qualifier rule applies to the operation parameter.
    #[test]
    fn does_not_flag_an_operation_class_from_another_librarys_package() {
        let (count, violations) =
            methods("(defmethod perform ((o my-build:compile-op) (c cl-source-file)) (do-it c))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_static_file_whose_standard_perform_is_already_a_no_op() {
        let (count, violations) =
            methods("(defmethod perform ((op compile-op) (c static-file)) nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_eql_specialized_component() {
        let (count, violations) =
            methods("(defmethod perform ((op load-op) (c (eql (find-system \"app\")))) (boot))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_unspecialized_component_parameter() {
        // Specialized on `t`, which is broader still — but "no specializer
        // written" is too weak a signal to report on.
        let (count, violations) = methods("(defmethod perform ((op load-op) c) (boot c))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_different_generic_function() {
        let (count, violations) = methods(
            "(defmethod output-files ((op compile-op) (c cl-source-file)) (list \"x.fasl\"))",
        );
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_defgeneric_or_a_plain_call() {
        let (count, violations) = methods("(defgeneric perform (op c))\n(perform op c)\n");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_method_with_no_lambda_list() {
        let (count, violations) = methods("(defmethod perform)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    // --- quote/quasiquote negatives (the five shapes)

    const OFFENDER: &str = "(defmethod perform ((op load-op) (c cl-source-file)) (boot c))";

    #[test]
    fn a_hard_quoted_defmethod_is_list_data_and_is_not_flagged() {
        let (count, violations) = methods(&format!("'{OFFENDER}"));
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_list_data_and_is_not_flagged() {
        let (count, violations) = methods(&format!("(quote {OFFENDER})"));
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_still_list_data() {
        let (count, violations) = methods(&format!("'(a ,{OFFENDER})"));
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_list_data() {
        let (count, violations) = methods(&format!("`{OFFENDER}"));
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn an_unquoted_defmethod_inside_a_backquote_is_code_and_is_flagged() {
        let (count, violations) = methods(&format!("`(a ,{OFFENDER})"));
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// The body search is quote-aware too: a `call-next-method` that only
    /// appears inside quoted data is not a call, so the method still does not
    /// compose.
    #[test]
    fn a_quoted_call_next_method_in_the_body_does_not_count_as_calling_it() {
        let (_, violations) = methods(
            "(defmethod perform ((op load-op) (c cl-source-file))\n\
             \x20 (warn \"use ~S\" '(call-next-method)))\n",
        );
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn an_unquoted_call_next_method_inside_a_backquote_does_count() {
        let (_, violations) = methods(
            "(defmethod perform ((op load-op) (c cl-source-file))\n\
             \x20 (eval `(progn ,(call-next-method))))\n",
        );
        assert!(violations.is_empty());
    }

    // --- string-literal negative

    #[test]
    fn a_defmethod_inside_a_string_literal_is_one_atom_and_is_not_a_form() {
        let (count, violations) = methods("(format t \"(defmethod perform ((op load-op) c))\")");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// A docstring naming the call is not a call.
    #[test]
    fn call_next_method_named_only_in_a_docstring_does_not_count() {
        let (_, violations) = methods(
            "(defmethod perform ((op load-op) (c cl-source-file))\n\
             \x20 \"Deliberately does not call-next-method.\"\n\
             \x20 (boot c))\n",
        );
        assert_eq!(violations.len(), 1);
    }

    // --- envelope

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(OFFENDER, Dialect::Clojure).expect("parse");
        let report = build_asdf_perform_without_call_next_method_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("perform_method_count", json!(0))]);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_fields() {
        let report = report(&format!("\n{OFFENDER}\n"));
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "asdf-perform-without-call-next-method");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operation", json!("load-op")),
                ("component", json!("cl-source-file"))
            ]
        );
        assert!(finding.message().contains("never calls call-next-method"));
    }

    #[test]
    fn the_summary_counts_every_in_scope_method_not_only_the_flagged_ones() {
        let report = report(
            "(defmethod perform ((op load-op) (c cl-source-file)) (call-next-method))\n\
             (defmethod perform ((op compile-op) (c cl-source-file)) (boot c))\n\
             (defmethod perform ((o test-op) (c (eql (find-system \"app\")))) (run))\n",
        );
        assert_eq!(report.summary, vec![("perform_method_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
