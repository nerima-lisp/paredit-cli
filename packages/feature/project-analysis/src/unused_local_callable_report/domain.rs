//! Unused local-callable detection: a Common Lisp `flet`/`labels` binding
//! whose name is never referenced anywhere it could legally be called.
//!
//! `flet` bindings can only be called from the form's own body (siblings
//! cannot see each other); `labels` bindings can additionally call each
//! other and themselves, since `labels` is the mutually-recursive variant.
//! That visibility difference is reused verbatim from
//! [`paredit_core_semantics::callable_scope::local_callable_names`] and the
//! `CommonLispLocalCallableForm` distinction the codebase already relies on
//! for the `convert-flet-to-labels`/`convert-labels-to-flet` refactors, so
//! this report's notion of "where a binding is visible" never drifts from
//! theirs.
//!
//! Scope, by design: only `flet`/`labels` are checked, not `macrolet`/
//! `compiler-macrolet` — a macro expander's expansion can use its parameters
//! in ways a lexical reference scan cannot always see (see the analogous
//! carve-out in `inspect lets` for earmuffed dynamic variables), so treating
//! an apparently-unused macro expander as dead code risks a false positive
//! with a more surprising consequence than an unused ordinary function.

use std::path::PathBuf;

use crate::error::ProjectAnalysisResult;

use paredit_core_semantics::callable_scope::{
    common_lisp_local_callable_form, local_callable_names,
};
use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::common_lisp::CommonLispLocalCallableForm;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName, SyntaxTree,
};

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

#[derive(Debug, Clone)]
pub struct UnusedLocalCallableItem {
    pub form_span: ByteSpan,
    pub form_head: String,
    pub name: String,
}

#[derive(Debug)]
pub struct UnusedLocalCallableReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub checked_binding_count: usize,
    pub unused: Vec<UnusedLocalCallableItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusedLocalCallablePolicyOptions {
    fail_on_unused: bool,
}

impl UnusedLocalCallablePolicyOptions {
    #[must_use]
    pub const fn new(fail_on_unused: bool) -> Self {
        Self { fail_on_unused }
    }

    #[must_use]
    pub const fn fail_on_unused(self) -> bool {
        self.fail_on_unused
    }
}

#[derive(Debug)]
pub struct UnusedLocalCallablePolicy {
    pub fail_on_unused: bool,
    pub checked_binding_count: usize,
    pub unused_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_unused_local_callable_report(
    path: PathBuf,
    dialect: Dialect,
    input: &str,
    tree: &SyntaxTree,
) -> ProjectAnalysisResult<UnusedLocalCallableReportFile> {
    let mut unused = Vec::new();
    let mut checked_binding_count = 0;

    walk(
        dialect,
        input,
        &tree.root_view(),
        &mut unused,
        &mut checked_binding_count,
    );

    Ok(UnusedLocalCallableReportFile {
        path,
        dialect,
        checked_binding_count,
        unused,
    })
}

fn walk(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    unused: &mut Vec<UnusedLocalCallableItem>,
    checked_binding_count: &mut usize,
) {
    if let Some(head) = list_head(view) {
        if let Some(form) = common_lisp_local_callable_form(dialect, head) {
            if matches!(
                form,
                CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels
            ) {
                analyze_local_callable_form(
                    dialect,
                    input,
                    view,
                    head,
                    form,
                    unused,
                    checked_binding_count,
                );
            }
        }
    }

    for child in &view.children {
        walk(dialect, input, child, unused, checked_binding_count);
    }
}

fn analyze_local_callable_form(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    head: &str,
    form: CommonLispLocalCallableForm,
    unused: &mut Vec<UnusedLocalCallableItem>,
    checked_binding_count: &mut usize,
) {
    let names = local_callable_names(view);
    if names.is_empty() {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    let outer_body = view.children.get(2..).unwrap_or_default();
    let is_labels = form == CommonLispLocalCallableForm::Labels;

    for name in &names {
        *checked_binding_count += 1;
        let Ok(symbol) = SymbolName::new(name.clone()) else {
            continue;
        };
        let mut references = Vec::new();
        for body_form in outer_body {
            collect_unshadowed_symbol_references(
                dialect,
                body_form,
                &symbol,
                input,
                &mut references,
            );
        }
        if is_labels {
            for binding in &binding_list.children {
                for body_form in binding.children.get(2..).unwrap_or_default() {
                    collect_unshadowed_symbol_references(
                        dialect,
                        body_form,
                        &symbol,
                        input,
                        &mut references,
                    );
                }
            }
        }
        if references.is_empty() {
            unused.push(UnusedLocalCallableItem {
                form_span: view.span,
                form_head: head.to_owned(),
                name: name.clone(),
            });
        }
    }
}

#[must_use]
pub fn evaluate_unused_local_callable_policy(
    options: UnusedLocalCallablePolicyOptions,
    reports: &[UnusedLocalCallableReportFile],
) -> UnusedLocalCallablePolicy {
    let checked_binding_count = reports
        .iter()
        .map(|report| report.checked_binding_count)
        .sum::<usize>();
    let unused_count = reports
        .iter()
        .map(|report| report.unused.len())
        .sum::<usize>();

    let mut violations = Vec::new();
    if options.fail_on_unused() && unused_count > 0 {
        violations.push(format!("unused_count {unused_count} exceeds 0"));
    }

    UnusedLocalCallablePolicy {
        fail_on_unused: options.fail_on_unused(),
        checked_binding_count,
        unused_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> UnusedLocalCallableReportFile {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_unused_local_callable_report(
            PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            input,
            &tree,
        )
        .expect("build unused local callable report")
    }

    #[test]
    fn flags_a_flet_binding_never_called_in_the_body() {
        let report = report("(defun f (x) (flet ((helper (y) (+ y 1))) x))");

        assert_eq!(report.checked_binding_count, 1);
        assert_eq!(report.unused.len(), 1);
        assert_eq!(report.unused[0].name, "helper");
        assert_eq!(report.unused[0].form_head, "flet");
    }

    #[test]
    fn does_not_flag_a_flet_binding_called_in_the_body() {
        let report = report("(defun f (x) (flet ((helper (y) (+ y 1))) (helper x)))");
        assert!(report.unused.is_empty());
    }

    #[test]
    fn does_not_flag_flet_bindings_calling_each_other() {
        // flet siblings cannot see each other, so this recursive-looking call
        // is actually a reference to an outer `even?`, and `odd?` itself is
        // genuinely never called from the accessible body.
        let report = report("(defun f (n) (flet ((odd? (x) (even? (- x 1)))) (even? n)))");
        // `odd?` is unused (no call reaches it: its own body cannot see
        // itself under flet's non-recursive semantics, and the outer body
        // only calls `even?`, an unrelated free reference).
        assert_eq!(report.unused.len(), 1);
        assert_eq!(report.unused[0].name, "odd?");
    }

    #[test]
    fn labels_allows_mutual_recursion_to_count_as_usage() {
        let report = report(
            "(defun f (n) (labels ((odd? (x) (if (= x 0) nil (even? (- x 1)))) \
             (even? (x) (if (= x 0) t (odd? (- x 1))))) (even? n)))",
        );
        assert!(report.unused.is_empty());
    }

    #[test]
    fn does_not_flag_macrolet_or_compiler_macrolet() {
        let report = report("(defun f (x) (macrolet ((helper (y) `(+ ,y 1))) x))");
        assert_eq!(report.checked_binding_count, 0);
        assert!(report.unused.is_empty());
    }

    #[test]
    fn finds_nested_flet_forms_anywhere_in_the_tree() {
        let report = report("(defun outer () (let ((z 1)) (flet ((helper () 1)) z)))");
        assert_eq!(report.unused.len(), 1);
        assert_eq!(report.unused[0].name, "helper");
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let report = report("(defun f (x) (flet ((helper (y) (+ y 1))) x))");

        let quiet = evaluate_unused_local_callable_policy(
            UnusedLocalCallablePolicyOptions::new(false),
            std::slice::from_ref(&report),
        );
        assert!(quiet.passed);
        assert_eq!(quiet.unused_count, 1);

        let strict = evaluate_unused_local_callable_policy(
            UnusedLocalCallablePolicyOptions::new(true),
            std::slice::from_ref(&report),
        );
        assert!(!strict.passed);
    }
}
