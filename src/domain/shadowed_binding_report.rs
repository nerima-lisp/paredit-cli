//! Lexical shadowing detection: a `let`-family binding that reuses the name
//! of an enclosing function parameter or an enclosing `let`-family binding.
//! Shadowing is not automatically a bug, but it is a well-known source of
//! confusion and copy-paste mistakes — code later in the same body can read
//! `x` expecting the outer value while actually seeing the inner rebinding.
//!
//! Scope, by design: this report only tracks two binding sources — a
//! definition's own lambda-list parameters, and `let`/`let*`-family bindings
//! at any nesting depth (reusing
//! [`crate::domain::let_report::build_let_report`] verbatim, so its notion of
//! a "binding" never drifts from the existing `inspect lets` report).
//! Bindings introduced by `flet`/`labels`/inline `lambda` parameter lists are
//! not tracked as scopes here, so shadowing relative to *those* binders is
//! not reported. Extending coverage to local-callable scopes would require
//! the same per-dialect local-callable-form recognition already encapsulated
//! in `lexical_scope::traversal`; this report deliberately stays within the
//! two binding sources it can analyze with already-vetted extraction logic
//! rather than reimplement that recognition a second time.

use std::path::PathBuf;

use anyhow::Result;

use crate::domain::common_lisp::CommonLispPackageDeclarationForm;
use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::function_parameter::list_lambda_list_parameter_names;
use crate::domain::let_report::build_let_report;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// A definition's own lambda-list parameters.
    Parameter,
    /// A `let`/`let*`-family binding form.
    LetBinding,
}

impl ScopeKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Parameter => "parameter",
            Self::LetBinding => "let-binding",
        }
    }
}

struct Scope {
    span: ByteSpan,
    names: Vec<String>,
    kind: ScopeKind,
    label: Option<String>,
}

fn spans_contain(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

#[derive(Debug, Clone)]
pub struct ShadowedBindingItem {
    pub name: String,
    pub inner_span: ByteSpan,
    pub outer_span: ByteSpan,
    pub outer_kind: ScopeKind,
    pub outer_label: Option<String>,
}

#[derive(Debug)]
pub struct ShadowedBindingReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub scope_count: usize,
    pub shadowed: Vec<ShadowedBindingItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowedBindingPolicyOptions {
    fail_on_shadowed: bool,
}

impl ShadowedBindingPolicyOptions {
    #[must_use]
    pub fn new(fail_on_shadowed: bool) -> Self {
        Self { fail_on_shadowed }
    }

    #[must_use]
    pub const fn fail_on_shadowed(self) -> bool {
        self.fail_on_shadowed
    }
}

#[derive(Debug)]
pub struct ShadowedBindingPolicy {
    pub fail_on_shadowed: bool,
    pub scope_count: usize,
    pub shadowed_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_shadowed_binding_report(
    path: PathBuf,
    dialect: Dialect,
    input: &str,
    tree: &SyntaxTree,
) -> Result<ShadowedBindingReportFile> {
    let mut scopes = Vec::new();

    for index in 0..tree.root_children().len() {
        let form_path = Path::root_child(index);
        let view = tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            continue;
        }
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        if !shape.category.is_callable() {
            continue;
        }
        let Some(lambda_list) = shape.lambda_list(&view) else {
            continue;
        };
        let Ok(names) = list_lambda_list_parameter_names(dialect, lambda_list) else {
            continue;
        };
        if names.is_empty() {
            continue;
        }
        scopes.push(Scope {
            span: view.span,
            names,
            kind: ScopeKind::Parameter,
            label: shape.name(&view).map(ToOwned::to_owned),
        });
    }

    for form in build_let_report(dialect, input, tree)? {
        if form.bindings.is_empty() {
            continue;
        }
        scopes.push(Scope {
            span: form.span,
            names: form
                .bindings
                .iter()
                .map(|binding| binding.name.clone())
                .collect(),
            kind: ScopeKind::LetBinding,
            label: None,
        });
    }

    let scope_count = scopes.len();
    let mut shadowed = Vec::new();

    for (index, scope) in scopes.iter().enumerate() {
        for name in &scope.names {
            let nearest = scopes
                .iter()
                .enumerate()
                .filter(|(other_index, other)| {
                    *other_index != index
                        && spans_contain(other.span, scope.span)
                        && other.names.iter().any(|other_name| other_name == name)
                })
                .min_by_key(|(_, other)| other.span.len());

            if let Some((_, outer)) = nearest {
                shadowed.push(ShadowedBindingItem {
                    name: name.clone(),
                    inner_span: scope.span,
                    outer_span: outer.span,
                    outer_kind: outer.kind,
                    outer_label: outer.label.clone(),
                });
            }
        }
    }

    shadowed.sort_by_key(|item| (item.inner_span.start().get(), item.name.clone()));

    Ok(ShadowedBindingReportFile {
        path,
        dialect,
        scope_count,
        shadowed,
    })
}

#[must_use]
pub fn evaluate_shadowed_binding_policy(
    options: ShadowedBindingPolicyOptions,
    reports: &[ShadowedBindingReportFile],
) -> ShadowedBindingPolicy {
    let scope_count = reports
        .iter()
        .map(|report| report.scope_count)
        .sum::<usize>();
    let shadowed_count = reports
        .iter()
        .map(|report| report.shadowed.len())
        .sum::<usize>();

    let mut violations = Vec::new();
    if options.fail_on_shadowed() && shadowed_count > 0 {
        violations.push(format!("shadowed_count {shadowed_count} exceeds 0"));
    }

    ShadowedBindingPolicy {
        fail_on_shadowed: options.fail_on_shadowed(),
        scope_count,
        shadowed_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> ShadowedBindingReportFile {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_shadowed_binding_report(
            PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            input,
            &tree,
        )
        .expect("build shadowed binding report")
    }

    #[test]
    fn flags_a_let_binding_that_shadows_a_parameter() {
        let report = report("(defun f (x) (let ((x 1)) (+ x 1)))");

        assert_eq!(report.shadowed.len(), 1);
        assert_eq!(report.shadowed[0].name, "x");
        assert_eq!(report.shadowed[0].outer_kind, ScopeKind::Parameter);
        assert_eq!(report.shadowed[0].outer_label.as_deref(), Some("f"));
    }

    #[test]
    fn flags_a_nested_let_that_shadows_an_outer_let() {
        let report = report("(defun f () (let ((x 1)) (let ((x 2)) (+ x 1))))");

        assert_eq!(report.shadowed.len(), 1);
        assert_eq!(report.shadowed[0].name, "x");
        assert_eq!(report.shadowed[0].outer_kind, ScopeKind::LetBinding);
    }

    #[test]
    fn does_not_flag_distinct_names() {
        let report = report("(defun f (x) (let ((y 1)) (+ x y)))");
        assert!(report.shadowed.is_empty());
    }

    #[test]
    fn does_not_flag_sibling_lets_that_do_not_nest() {
        let report = report("(defun f () (progn (let ((x 1)) x) (let ((x 2)) x)))");
        assert!(report.shadowed.is_empty());
    }

    #[test]
    fn reports_the_nearest_enclosing_scope_when_multiple_ancestors_share_a_name() {
        let report = report("(defun f (x) (let ((x 1)) (let ((x 2)) (+ x 1))))");

        // Both the parameter and the outer `let` bind `x`; the innermost
        // let's `x` should be reported against the *nearer* outer let, not
        // the parameter two levels up.
        let innermost = report
            .shadowed
            .iter()
            .filter(|item| item.name == "x")
            .min_by_key(|item| item.outer_span.len())
            .expect("at least one shadow");
        assert_eq!(innermost.outer_kind, ScopeKind::LetBinding);
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let report = report("(defun f (x) (let ((x 1)) (+ x 1)))");

        let quiet = evaluate_shadowed_binding_policy(
            ShadowedBindingPolicyOptions::new(false),
            std::slice::from_ref(&report),
        );
        assert!(quiet.passed);
        assert_eq!(quiet.shadowed_count, 1);

        let strict = evaluate_shadowed_binding_policy(
            ShadowedBindingPolicyOptions::new(true),
            std::slice::from_ref(&report),
        );
        assert!(!strict.passed);
    }
}
