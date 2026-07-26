//! Unused function-parameter detection: a declared parameter with no
//! unshadowed reference anywhere in its definition's body.
//!
//! Parameter names are extracted with the same validated lambda-list parser
//! the parameter add/remove/reorder refactors use
//! ([`crate::domain::function_parameter::list_lambda_list_parameter_names`]),
//! and usage is checked with the same shadow-aware traversal `inline-let`
//! relies on to avoid false captures
//! ([`crate::domain::lexical_scope::collect_unshadowed_symbol_references`]).
//! Reusing both keeps this report's notion of "parameter" and "reference"
//! identical to the refactors that already depend on them, rather than
//! drifting from a second, independently-written lambda-list parser.

use std::path::PathBuf;

use anyhow::Result;

use crate::domain::common_lisp::CommonLispPackageDeclarationForm;
use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::function_parameter::list_lambda_list_parameter_names;
use crate::domain::lexical_scope::collect_unshadowed_symbol_references;
use crate::domain::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
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

/// Whether `name` follows the cross-dialect convention for a deliberately
/// unused parameter (a bare `_`, or a leading underscore), which should never
/// be flagged.
fn is_conventionally_ignored(name: &str) -> bool {
    name == "_" || name.starts_with('_')
}

#[derive(Debug, Clone)]
pub struct UnusedParameterReportItem {
    pub definition_path: String,
    pub definition_span: ByteSpan,
    pub head: String,
    pub definition_name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_name: String,
}

#[derive(Debug)]
pub struct UnusedParameterReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Callable definitions with a parseable lambda list that were checked.
    pub checked_definition_count: usize,
    pub unused_parameters: Vec<UnusedParameterReportItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusedParameterReportPolicyOptions {
    fail_on_unused: bool,
}

impl UnusedParameterReportPolicyOptions {
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
pub struct UnusedParameterReportPolicy {
    pub fail_on_unused: bool,
    pub checked_definition_count: usize,
    pub unused_parameter_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_unused_parameter_report(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
    input: &str,
) -> Result<UnusedParameterReportFile> {
    let mut unused_parameters = Vec::new();
    let mut checked_definition_count = 0;

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
        // A definition whose lambda list this parser cannot classify (e.g. an
        // unsupported marker) is skipped rather than failing the whole scan:
        // one unusual macro's parameter DSL should not block reporting on
        // every other definition in the file.
        let Ok(parameter_names) = list_lambda_list_parameter_names(dialect, lambda_list) else {
            continue;
        };
        checked_definition_count += 1;

        let body_forms = shape.body_forms(&view);
        for name in parameter_names {
            if is_conventionally_ignored(&name) {
                continue;
            }
            let Ok(symbol) = SymbolName::new(name.clone()) else {
                continue;
            };
            let mut references = Vec::new();
            for body_form in body_forms {
                collect_unshadowed_symbol_references(
                    dialect,
                    body_form,
                    &symbol,
                    input,
                    &mut references,
                );
            }
            if references.is_empty() {
                unused_parameters.push(UnusedParameterReportItem {
                    definition_path: form_path.to_string(),
                    definition_span: view.span,
                    head: head.to_owned(),
                    definition_name: shape.name(&view).map(ToOwned::to_owned),
                    category: shape.category,
                    parameter_name: name,
                });
            }
        }
    }

    Ok(UnusedParameterReportFile {
        path,
        dialect,
        checked_definition_count,
        unused_parameters,
    })
}

#[must_use]
pub fn evaluate_unused_parameter_policy(
    options: UnusedParameterReportPolicyOptions,
    reports: &[UnusedParameterReportFile],
) -> UnusedParameterReportPolicy {
    let checked_definition_count = reports
        .iter()
        .map(|report| report.checked_definition_count)
        .sum::<usize>();
    let unused_parameter_count = reports
        .iter()
        .map(|report| report.unused_parameters.len())
        .sum::<usize>();

    let mut violations = Vec::new();
    if options.fail_on_unused() && unused_parameter_count > 0 {
        violations.push(format!(
            "unused_parameter_count {unused_parameter_count} exceeds 0"
        ));
    }

    UnusedParameterReportPolicy {
        fail_on_unused: options.fail_on_unused(),
        checked_definition_count,
        unused_parameter_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> UnusedParameterReportFile {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_unused_parameter_report(
            PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
        )
        .expect("build unused parameter report")
    }

    #[test]
    fn flags_a_parameter_never_referenced_in_the_body() {
        let report = report("(defun f (x y) (+ x 1))");

        assert_eq!(report.checked_definition_count, 1);
        assert_eq!(report.unused_parameters.len(), 1);
        assert_eq!(report.unused_parameters[0].parameter_name, "y");
        assert_eq!(
            report.unused_parameters[0].definition_name.as_deref(),
            Some("f")
        );
    }

    #[test]
    fn does_not_flag_parameters_used_in_the_body() {
        let report = report("(defun f (x y) (+ x y))");
        assert!(report.unused_parameters.is_empty());
    }

    #[test]
    fn does_not_flag_conventionally_ignored_names() {
        let report = report("(defun f (x _ _ignored) (+ x 1))");
        assert!(report.unused_parameters.is_empty());
    }

    #[test]
    fn does_not_flag_parameters_shadowed_and_then_used_under_a_new_binding() {
        // `x` the parameter is never referenced; only the inner `let`'s `x`
        // is used, so the outer parameter must still be flagged.
        let report = report("(defun f (x) (let ((x 1)) (+ x 1)))");
        assert_eq!(report.unused_parameters.len(), 1);
        assert_eq!(report.unused_parameters[0].parameter_name, "x");
    }

    #[test]
    fn handles_optional_and_key_parameters() {
        let report = report("(defun f (x &optional y &key z) (+ x z))");
        assert_eq!(report.unused_parameters.len(), 1);
        assert_eq!(report.unused_parameters[0].parameter_name, "y");
    }

    #[test]
    fn ignores_in_package_and_non_callable_forms() {
        let report = report("(in-package :foo)\n(defvar *x* 1)\n(defun f (x) (+ x 1))");
        assert_eq!(report.checked_definition_count, 1);
        assert!(report.unused_parameters.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let report = report("(defun f (x y) (+ x 1))");

        let quiet = evaluate_unused_parameter_policy(
            UnusedParameterReportPolicyOptions::new(false),
            std::slice::from_ref(&report),
        );
        assert!(quiet.passed);
        assert_eq!(quiet.unused_parameter_count, 1);

        let strict = evaluate_unused_parameter_policy(
            UnusedParameterReportPolicyOptions::new(true),
            std::slice::from_ref(&report),
        );
        assert!(!strict.passed);
    }
}
