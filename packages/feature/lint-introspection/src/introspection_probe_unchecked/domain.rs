//! `introspection-probe-unchecked` detection: a probe whose not-found answer is
//! `nil`, applied directly by `funcall`/`apply`.
//!
//! `(funcall (find-symbol (string-upcase op) :app) request)` looks a symbol up
//! and calls it in one expression. When the lookup fails, `find-symbol` returns
//! `nil` and the program calls `nil`, which is an `undefined-function` error
//! several frames away from the thing that actually went wrong — a mistyped
//! name, a package that was not loaded, a plugin that is not installed.
//!
//! # Why this anchors on the consumer and not on the probe
//!
//! "Was this value checked?" is a question about where the value *goes*. A rule
//! anchored on `find-symbol` would see one node and would have to look upward
//! for the check, which a per-node predicate cannot do. Anchoring on
//! `funcall`/`apply` turns it into a question about a single form: the probe's
//! result is in the function position of the very call that consumes it, so
//! there is no point at which it *could* have been checked.
//!
//! This is also why the correct idiom is silent by construction. Every one of
//!
//! ```lisp
//! (let ((f (find-symbol name :app))) (when f (funcall f x)))
//! (if-let ((f (resolve sym))) (apply f args) (default))
//! (funcall (or (find-symbol name :app) #'reject) x)
//! (progn (check-type f function) (funcall f x))
//! ```
//!
//! puts something other than a probe call in the function position, so none of
//! them matches. Probing and *then checking* is the right thing to do; this
//! rule is only about the case where no check is possible.
//!
//! # Which probes, and why so few
//!
//! Only probes whose reference text says the not-found answer is `nil`. The
//! obvious additions are wrong, and [`crate::support::nil_returning_probes`]
//! records why for each: `find-class` signals unless `errorp` is explicitly
//! `nil`, `symbol-function` signals `undefined-function`, and `fboundp` *is*
//! the check.
//!
//! # And only a dynamically-named probe
//!
//! `(funcall (macro-function 'when) form env)` names its subject outright, and
//! whether `when` is a macro is not in doubt. Only a probe whose own name
//! argument is computed — the "probe a dynamically-named definition" case — is
//! reported.
//!
//! # Known limit
//!
//! A probe repeated inside its own check —
//! `(when (find-symbol n :app) (funcall (find-symbol n :app) x))` — is reported,
//! because the inner call is judged on its own and the surrounding `when` is
//! ancestor context this rule does not have. Re-probing rather than binding the
//! first result is unusual, and the alternative is a whole-tree walk this
//! package's cost rules forbid.
//!
//! # Relation to `check-then-act`
//!
//! None. `paredit-feature-lint-safety`'s `check-then-act` anchors on
//! `unless`/`when`/`if`/`cond` and is about a shared *place* written after being
//! tested. It shares no head, no span and no subject with this rule.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    apply_operators, call_operator, calls_any, for_each_evaluated_subview, is_source_visible_name,
    is_unevaluated_at, nil_returning_probes,
};

/// One probe applied without a check.
#[derive(Debug, Clone)]
pub struct IntrospectionProbeUncheckedItem {
    /// The span of the whole `(funcall …)` / `(apply …)` form.
    pub span: ByteSpan,
    /// The operator that applies the value: `funcall` or `apply`.
    pub consumer: String,
    /// The probe whose `nil` reaches it: `find-symbol`, `macro-function`,
    /// `intern-soft`, `resolve`, or `ns-resolve`.
    pub probe: String,
}

impl Finding for IntrospectionProbeUncheckedItem {
    fn kind(&self) -> &'static str {
        "introspection-probe-unchecked"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("consumer={}", self.consumer),
            format!("probe={}", self.probe),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("consumer", json!(self.consumer)),
            ("probe", json!(self.probe)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} applies the result of {} directly; {} answers nil when the name is not found, so \
             a missing definition becomes a call to nil instead of a checked branch",
            self.consumer, self.probe, self.probe
        )
    }
}

/// Whether a form is a `(funcall …)` / `(apply …)` in this dialect.
///
/// Used by the report walk to count the denominator, and cheap enough to be
/// asked of every evaluated node.
#[must_use]
pub fn is_application_form(view: &ExpressionView, dialect: Dialect) -> bool {
    calls_any(view, apply_operators(dialect))
}

/// Examines one application form.
///
/// Ordered cheapest-first: the structural tests are pointer derefs and allocate
/// nothing, and [`is_unevaluated_at`] — the only part that touches the tree —
/// runs last, once the finding is otherwise certain.
#[must_use]
pub fn examine(
    tree: &SyntaxTree,
    view: &ExpressionView,
    dialect: Dialect,
) -> Option<IntrospectionProbeUncheckedItem> {
    if !is_application_form(view, dialect) {
        return None;
    }
    // The *function* position, and only it. A probe among the arguments is a
    // value being passed, not a callable being invoked.
    let applied = view.children.get(1)?;
    if !calls_any(applied, nil_returning_probes(dialect)) {
        return None;
    }
    // A probe that names its subject outright is not a dynamically-named one.
    let probed_name = applied.children.get(1)?;
    if is_source_visible_name(probed_name) {
        return None;
    }

    // The template case: `` `(funcall (find-symbol ,n) x) `` is a list being
    // built.
    if is_unevaluated_at(tree, view.span) {
        return None;
    }

    Some(IntrospectionProbeUncheckedItem {
        span: view.span,
        consumer: call_operator(view)?,
        probe: call_operator(applied)?,
    })
}

/// Collects every unchecked probe application in one file, with the number of
/// evaluated application forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_introspection_probe_unchecked_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IntrospectionProbeUncheckedItem>> {
    if !matches!(
        dialect,
        Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Clojure
    ) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("application_form_count", json!(0))],
        ));
    }

    let mut application_form_count = 0_usize;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            if !is_application_form(subview, dialect) {
                return;
            }
            application_form_count += 1;
            if let Some(item) = examine(tree, subview, dialect) {
                violations.push(item);
            }
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("application_form_count", json!(application_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::for_each_subview;

    /// Runs `examine` on the first application form in the source, found by an
    /// *unfiltered* walk so that quoted occurrences reach it too — which is
    /// exactly what the engine's dispatch does.
    fn examined(input: &str, dialect: Dialect) -> Option<IntrospectionProbeUncheckedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let root = tree.root_view();
        let mut found = None;
        let mut seen = false;
        for_each_subview(&root, |view| {
            if seen || !is_application_form(view, dialect) {
                return;
            }
            seen = true;
            found = examine(&tree, view, dialect);
        });
        found
    }

    fn probe(input: &str, dialect: Dialect) -> Option<String> {
        examined(input, dialect).map(|item| item.probe)
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_funcall_of_a_find_symbol_lookup() {
        let found = examined(
            "(funcall (find-symbol (string-upcase op) :app) request)",
            Dialect::CommonLisp,
        )
        .expect("a finding");
        assert_eq!(found.consumer, "funcall");
        assert_eq!(found.probe, "find-symbol");
    }

    #[test]
    fn flags_apply_as_well_as_funcall() {
        assert_eq!(
            probe("(apply (find-symbol name :app) args)", Dialect::CommonLisp),
            Some("find-symbol".to_owned())
        );
    }

    #[test]
    fn flags_a_macro_function_lookup_of_a_computed_name() {
        assert_eq!(
            probe(
                "(funcall (macro-function name) form env)",
                Dialect::CommonLisp
            ),
            Some("macro-function".to_owned())
        );
    }

    #[test]
    fn flags_intern_soft_in_emacs_lisp() {
        assert_eq!(
            probe("(funcall (intern-soft name) arg)", Dialect::EmacsLisp),
            Some("intern-soft".to_owned())
        );
    }

    #[test]
    fn flags_resolve_and_ns_resolve_in_clojure() {
        assert_eq!(
            probe("(apply (resolve sym) args)", Dialect::Clojure),
            Some("resolve".to_owned())
        );
        assert_eq!(
            probe("(apply (ns-resolve ns sym) args)", Dialect::Clojure),
            Some("ns-resolve".to_owned())
        );
    }

    #[test]
    fn reads_the_head_case_insensitively_and_through_a_package_qualifier() {
        assert_eq!(
            probe("(CL:FUNCALL (CL:FIND-SYMBOL name) x)", Dialect::CommonLisp),
            Some("find-symbol".to_owned())
        );
    }

    #[test]
    fn the_message_names_the_consumer_and_the_probe() {
        let found =
            examined("(funcall (find-symbol name) x)", Dialect::CommonLisp).expect("a finding");
        assert_eq!(
            found.message(),
            "funcall applies the result of find-symbol directly; find-symbol answers nil when the \
             name is not found, so a missing definition becomes a call to nil instead of a \
             checked branch"
        );
        assert_eq!(found.kind(), "introspection-probe-unchecked");
    }

    // -- the checked idioms, which are the whole false-positive risk ---------

    /// The four spellings of "probe, then check". None of them puts a probe
    /// call in the function position, so none of them can match.
    #[test]
    fn does_not_flag_a_probe_whose_result_is_checked() {
        for source in [
            "(let ((f (find-symbol name :app))) (when f (funcall f x)))",
            "(let ((f (find-symbol name :app))) (if f (funcall f x) (default)))",
            "(funcall (or (find-symbol name :app) #'reject) x)",
            "(let ((f (find-symbol name :app))) (check-type f function) (funcall f x))",
            "(let ((f (find-symbol name :app))) (assert f) (funcall f x))",
        ] {
            assert_eq!(probe(source, Dialect::CommonLisp), None, "for {source}");
        }
        assert_eq!(
            probe(
                "(if-let [f (resolve sym)] (apply f args) (default))",
                Dialect::Clojure
            ),
            None
        );
        assert_eq!(
            probe(
                "(when-let ((f (intern-soft name))) (funcall f x))",
                Dialect::EmacsLisp
            ),
            None
        );
    }

    // -- near-miss negatives -------------------------------------------------

    /// The three probes that do *not* answer not-found with `nil`. Adding any
    /// of them would be reporting a sentinel that never arrives.
    #[test]
    fn does_not_flag_a_probe_that_signals_instead_of_returning_nil() {
        assert_eq!(
            probe("(funcall (symbol-function name) x)", Dialect::CommonLisp),
            None
        );
        assert_eq!(
            probe("(funcall (find-class name) x)", Dialect::CommonLisp),
            None
        );
        assert_eq!(
            probe("(funcall (fboundp name) x)", Dialect::CommonLisp),
            None
        );
    }

    /// A probe that names its subject outright: not a dynamically-named
    /// definition.
    #[test]
    fn does_not_flag_a_probe_of_a_name_the_source_shows() {
        assert_eq!(
            probe(
                "(funcall (macro-function 'when) form env)",
                Dialect::CommonLisp
            ),
            None
        );
        assert_eq!(
            probe(
                "(funcall (find-symbol \"HANDLE\" :app) x)",
                Dialect::CommonLisp
            ),
            None
        );
        assert_eq!(
            probe("(apply (resolve 'my-fn) args)", Dialect::Clojure),
            None
        );
        assert_eq!(
            probe("(funcall (intern-soft \"my-fn\") x)", Dialect::EmacsLisp),
            None
        );
    }

    /// The function position, and only it.
    #[test]
    fn does_not_flag_a_probe_passed_as_an_argument() {
        assert_eq!(
            probe(
                "(funcall #'register (find-symbol name :app))",
                Dialect::CommonLisp
            ),
            None
        );
        assert_eq!(
            probe("(apply #'register (list (resolve sym)))", Dialect::Clojure),
            None
        );
    }

    #[test]
    fn does_not_flag_an_ordinary_application() {
        assert_eq!(
            probe("(funcall handler request)", Dialect::CommonLisp),
            None
        );
        assert_eq!(probe("(funcall #'run x)", Dialect::CommonLisp), None);
        assert_eq!(probe("(funcall)", Dialect::CommonLisp), None);
    }

    /// Clojure has no `funcall`, so a `(funcall …)` in a `.clj` file is some
    /// project's own function and must not be read as an application.
    #[test]
    fn does_not_claim_a_spelling_the_dialect_lacks() {
        assert_eq!(probe("(funcall (resolve sym) x)", Dialect::Clojure), None);
        // `resolve` is Clojure's; in Common Lisp it is not a probe this rule
        // knows.
        assert_eq!(
            probe("(funcall (resolve sym) x)", Dialect::CommonLisp),
            None
        );
        // `intern-soft` is Emacs Lisp's.
        assert_eq!(
            probe("(funcall (intern-soft name) x)", Dialect::CommonLisp),
            None
        );
    }

    #[test]
    fn an_unmodelled_dialect_is_left_alone() {
        assert_eq!(probe("(funcall (find-symbol n) x)", Dialect::Scheme), None);
    }

    // -- the five quote shapes, plus the macro template ----------------------

    const SHAPE: &str = "(funcall (find-symbol name) x)";

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert_eq!(probe(&format!("'{SHAPE}"), Dialect::CommonLisp), None);
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert_eq!(
            probe(&format!("(quote {SHAPE})"), Dialect::CommonLisp),
            None
        );
    }

    #[test]
    fn does_not_flag_an_unescaped_backquote() {
        assert_eq!(probe(&format!("`{SHAPE}"), Dialect::CommonLisp), None);
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert_eq!(probe(&format!("'(a ,{SHAPE})"), Dialect::CommonLisp), None);
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            probe(&format!("`(a ,{SHAPE})"), Dialect::CommonLisp),
            Some("find-symbol".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_backquoted_macro_template() {
        assert_eq!(
            probe(
                "(defmacro dispatch (n)\n  `(funcall (find-symbol ,n :app) request))",
                Dialect::CommonLisp
            ),
            None
        );
    }

    #[test]
    fn does_not_flag_a_form_written_inside_a_string_literal() {
        assert_eq!(
            probe(
                "(defparameter *doc* \"(funcall (find-symbol name) x)\")",
                Dialect::CommonLisp
            ),
            None
        );
    }

    // -- the report ----------------------------------------------------------

    fn report(input: &str, dialect: Dialect) -> FileFindings<IntrospectionProbeUncheckedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        build_introspection_probe_unchecked_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
    }

    #[test]
    fn the_denominator_counts_every_evaluated_application_form_scanned() {
        let built = report(
            "(funcall (find-symbol a) x)\n(funcall handler y)\n(apply #'f args)\n'(funcall (find-symbol b) z)\n",
            Dialect::CommonLisp,
        );
        assert_eq!(built.summary, vec![("application_form_count", json!(3))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let built = report(
            "(defun dispatch (op)\n  (funcall (find-symbol (string-upcase op) :app) op))\n",
            Dialect::CommonLisp,
        );
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.text_columns(),
            vec![
                "consumer=funcall".to_owned(),
                "probe=find-symbol".to_owned()
            ]
        );
        assert_eq!(
            finding.json_fields(),
            vec![
                ("consumer", json!("funcall")),
                ("probe", json!("find-symbol")),
            ]
        );
    }

    #[test]
    fn a_non_modelled_dialect_is_reported_as_unmodelled() {
        let built = report("(funcall (find-symbol n) x)", Dialect::Scheme);
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
        assert_eq!(built.summary, vec![("application_form_count", json!(0))]);
    }

    #[test]
    fn every_modelled_dialect_is_reported_as_modelled() {
        assert!(report("(funcall f x)", Dialect::CommonLisp).dialect_modelled);
        assert!(report("(funcall f x)", Dialect::EmacsLisp).dialect_modelled);
        assert!(report("(apply f args)", Dialect::Clojure).dialect_modelled);
    }
}
