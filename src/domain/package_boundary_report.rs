//! Common Lisp package-boundary violation detection: a `package::symbol`
//! (double-colon) reference to another package's *internal*, non-exported
//! symbol table, used from code that is not itself part of that package.
//!
//! `pkg:sym` (single colon) only resolves if `sym` is exported from `pkg`;
//! `pkg::sym` (double colon) always resolves, bypassing export entirely.
//! Using the double-colon form from outside `pkg` reaches into an
//! implementation detail the package author never declared as public API —
//! the same "don't import a module's private members" smell most languages
//! warn about, expressed through Common Lisp's own package system.
//!
//! Scope: Common Lisp only, since the exported/internal symbol distinction
//! this report is built on is specific to the CL package system. The active
//! package for each qualified reference is tracked purely from preceding
//! top-level `in-package` forms in the same file (mirroring how
//! `crate::domain::definition_report::collect_definition_forms` already
//! tracks "current package" for its own inventory), so a reference is only
//! flagged when the file's *own* declared package differs from the
//! reference's target — a same-package self-reference through the
//! double-colon form is redundant, not a boundary violation.

use std::path::PathBuf;

use anyhow::Result;

use crate::domain::common_lisp::{
    CommonLispPackageDeclarationForm, common_lisp_symbol_name_eq,
    normalize_common_lisp_package_designator,
};
use crate::domain::dependency_report::{DependencyKind, build_dependency_report};
use crate::domain::dialect::Dialect;
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

fn root_index_of(path: &str) -> Option<usize> {
    path.split('.').next()?.parse().ok()
}

#[derive(Debug, Clone)]
pub struct PackageBoundaryItem {
    pub path: String,
    pub span: ByteSpan,
    pub reference: String,
    pub target_package: String,
    pub current_package: Option<String>,
}

#[derive(Debug)]
pub struct PackageBoundaryReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub qualified_symbol_count: usize,
    pub violations: Vec<PackageBoundaryItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageBoundaryPolicyOptions {
    fail_on_violation: bool,
}

impl PackageBoundaryPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct PackageBoundaryPolicy {
    pub fail_on_violation: bool,
    pub qualified_symbol_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Maps each top-level form's root index to the package that was active
/// (via the nearest preceding `in-package`) *when that form itself runs*,
/// i.e. an `in-package` form updates the active package for every form
/// *after* it, not for itself.
fn package_by_root_index(dialect: Dialect, tree: &SyntaxTree) -> Result<Vec<Option<String>>> {
    let mut current_package = None;
    let mut by_index = Vec::with_capacity(tree.root_children().len());

    for index in 0..tree.root_children().len() {
        by_index.push(current_package.clone());
        let view = tree.select_path(&Path::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            if let Some(name) = atom_child(&view, 1) {
                current_package = Some(normalize_common_lisp_package_designator(name).to_owned());
            }
        }
    }

    Ok(by_index)
}

pub fn build_package_boundary_report(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<PackageBoundaryReportFile> {
    if dialect != Dialect::CommonLisp {
        return Ok(PackageBoundaryReportFile {
            path,
            dialect,
            qualified_symbol_count: 0,
            violations: Vec::new(),
        });
    }

    let by_index = package_by_root_index(dialect, tree)?;
    let dependencies = build_dependency_report(tree, dialect)?;

    let mut qualified_symbol_count = 0;
    let mut violations = Vec::new();

    for item in &dependencies.dependencies {
        if item.kind != DependencyKind::QualifiedSymbol {
            continue;
        }
        qualified_symbol_count += 1;

        let Some(reference) = &item.source else {
            continue;
        };
        if !reference.contains("::") {
            continue;
        }

        let current_package = root_index_of(&item.path)
            .and_then(|index| by_index.get(index))
            .cloned()
            .flatten();
        let is_self_reference = current_package
            .as_deref()
            .is_some_and(|package| common_lisp_symbol_name_eq(package, &item.target));
        if is_self_reference {
            continue;
        }

        violations.push(PackageBoundaryItem {
            path: item.path.clone(),
            span: item.span,
            reference: reference.clone(),
            target_package: item.target.clone(),
            current_package,
        });
    }

    Ok(PackageBoundaryReportFile {
        path,
        dialect,
        qualified_symbol_count,
        violations,
    })
}

pub fn evaluate_package_boundary_policy(
    options: PackageBoundaryPolicyOptions,
    reports: &[PackageBoundaryReportFile],
) -> PackageBoundaryPolicy {
    let qualified_symbol_count = reports
        .iter()
        .map(|report| report.qualified_symbol_count)
        .sum::<usize>();
    let violation_count = reports
        .iter()
        .map(|report| report.violations.len())
        .sum::<usize>();

    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    PackageBoundaryPolicy {
        fail_on_violation: options.fail_on_violation(),
        qualified_symbol_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> PackageBoundaryReportFile {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_package_boundary_report(PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build package boundary report")
    }

    #[test]
    fn flags_a_double_colon_reference_to_another_package() {
        let report = report("(in-package :app)\n(defun f () (other-pkg::internal-helper))");

        assert_eq!(report.qualified_symbol_count, 1);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].target_package, "other-pkg");
        assert_eq!(report.violations[0].current_package.as_deref(), Some("app"));
    }

    #[test]
    fn does_not_flag_single_colon_exported_access() {
        let report = report("(in-package :app)\n(defun f () (other-pkg:public-api))");
        assert_eq!(report.qualified_symbol_count, 1);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_double_colon_self_reference() {
        let report = report("(in-package :app)\n(defun f () (app::internal-helper))");
        assert_eq!(report.qualified_symbol_count, 1);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn does_not_flag_references_before_any_in_package_as_self_reference() {
        // No `in-package` precedes this form, so there is no declared
        // "current package" to compare against; conservatively treat any
        // double-colon reference here as crossing a boundary.
        let report = report("(defun f () (other-pkg::internal-helper))");
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].current_package, None);
    }

    #[test]
    fn tracks_package_changes_across_multiple_in_package_forms() {
        let report = report(
            "(in-package :app)\n\
             (defun a () (lib::detail))\n\
             (in-package :lib)\n\
             (defun b () (lib::detail))",
        );

        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].current_package.as_deref(), Some("app"));
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defn f [] (other.ns/thing))").expect("parse input");
        let report =
            build_package_boundary_report(PathBuf::from("test.clj"), Dialect::Clojure, &tree)
                .expect("build package boundary report");
        assert_eq!(report.qualified_symbol_count, 0);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let report = report("(in-package :app)\n(defun f () (other-pkg::internal-helper))");

        let quiet = evaluate_package_boundary_policy(
            PackageBoundaryPolicyOptions::new(false),
            std::slice::from_ref(&report),
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_package_boundary_policy(
            PackageBoundaryPolicyOptions::new(true),
            std::slice::from_ref(&report),
        );
        assert!(!strict.passed);
    }
}
