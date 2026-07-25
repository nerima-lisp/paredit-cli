//! Common Lisp "unused package" detection: a `defpackage` whose name is
//! never referenced anywhere in the analyzed fileset — not `:use`d, not
//! `:import-from`ed, and never reached through a qualified symbol
//! (`pkg:sym` or `pkg::sym`). This is the package-level analog of
//! [`crate::domain::definition_report`]'s unused-definition detection: a
//! candidate for deletion, or evidence that a package's public API is
//! consumed only outside the analyzed fileset (e.g. by downstream
//! consumers of a library) — `--fail-on-unused` stays an opt-in gate for
//! exactly that reason, the same caveat `unused-definitions` already
//! carries.
//!
//! Built on the same [`crate::domain::dependency_report::build_dependency_report`]
//! extraction used by [`crate::domain::package_cycle_report`] (for
//! `DefpackageUse`/`DefpackageImportFrom` edges) and
//! [`crate::domain::package_boundary_report`] (for `QualifiedSymbol`
//! references) — every kind of package reference this report needs is
//! already collected there; this module only decides which declared
//! packages never appear as *any* of those references' target.
//!
//! Scope: Common Lisp only, since `defpackage` is specific to the CL
//! package system; non-Common-Lisp files contribute no declarations and no
//! references.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use crate::domain::dependency_report::{DependencyKind, build_dependency_report};
use crate::domain::dialect::Dialect;
use crate::domain::package_report::build_package_report;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

#[derive(Debug, Clone)]
pub struct DeclaredPackage {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
}

impl DeclaredPackage {
    pub fn new(path: PathBuf, span: ByteSpan, name: impl Into<String>) -> Self {
        Self {
            path,
            span,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnusedPackageItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
}

#[derive(Debug)]
pub struct UnusedPackageSummary {
    pub declared_count: usize,
    pub unused: Vec<UnusedPackageItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusedPackagePolicyOptions {
    fail_on_unused: bool,
}

impl UnusedPackagePolicyOptions {
    pub fn new(fail_on_unused: bool) -> Self {
        Self { fail_on_unused }
    }

    pub const fn fail_on_unused(self) -> bool {
        self.fail_on_unused
    }
}

#[derive(Debug)]
pub struct UnusedPackagePolicy {
    pub fail_on_unused: bool,
    pub declared_count: usize,
    pub unused_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every `defpackage` declaration in one file, regardless of
/// whether it has any options at all (a bare `(defpackage :app)` still
/// declares a package and is still a candidate for "unused").
pub fn collect_declared_packages(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<DeclaredPackage>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    Ok(report
        .defpackages
        .into_iter()
        .map(|defpackage| {
            DeclaredPackage::new(
                path.to_path_buf(),
                defpackage.span,
                normalize_common_lisp_package_designator(&defpackage.name),
            )
        })
        .collect())
}

/// Collects every package name referenced from one file, through a
/// `:use`/`:import-from` clause or a qualified symbol (`pkg:sym` or
/// `pkg::sym`).
pub fn collect_referenced_package_names(
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<String>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let dependencies = build_dependency_report(tree, dialect)?;
    Ok(dependencies
        .dependencies
        .into_iter()
        .filter(|item| {
            matches!(
                item.kind,
                DependencyKind::DefpackageUse
                    | DependencyKind::DefpackageImportFrom
                    | DependencyKind::QualifiedSymbol
            )
        })
        .map(|item| normalize_common_lisp_package_designator(&item.target).to_owned())
        .collect())
}

pub fn analyze_unused_packages(
    declared: &[DeclaredPackage],
    referenced: &[String],
) -> UnusedPackageSummary {
    let referenced_needles: BTreeSet<String> = referenced
        .iter()
        .map(|name| common_lisp_symbol_reference_needle(name))
        .collect();

    let unused = declared
        .iter()
        .filter(|package| {
            !referenced_needles.contains(&common_lisp_symbol_reference_needle(&package.name))
        })
        .map(|package| UnusedPackageItem {
            path: package.path.clone(),
            span: package.span,
            name: package.name.clone(),
        })
        .collect();

    UnusedPackageSummary {
        declared_count: declared.len(),
        unused,
    }
}

pub fn evaluate_unused_package_policy(
    options: UnusedPackagePolicyOptions,
    summary: &UnusedPackageSummary,
) -> UnusedPackagePolicy {
    let unused_count = summary.unused.len();
    let mut violations = Vec::new();
    if options.fail_on_unused() && unused_count > 0 {
        violations.push(format!("unused_count {unused_count} exceeds 0"));
    }

    UnusedPackagePolicy {
        fail_on_unused: options.fail_on_unused(),
        declared_count: summary.declared_count,
        unused_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(path: &str, input: &str) -> Vec<DeclaredPackage> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_packages(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared packages")
    }

    fn referenced(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_referenced_package_names(Dialect::CommonLisp, &tree)
            .expect("collect referenced package names")
    }

    #[test]
    fn flags_a_package_declared_but_never_referenced() {
        let declared = declared("orphan.lisp", "(defpackage :orphan (:use :cl))");
        let summary = analyze_unused_packages(&declared, &[]);

        assert_eq!(summary.declared_count, 1);
        assert_eq!(summary.unused.len(), 1);
        assert_eq!(summary.unused[0].name, "orphan");
    }

    #[test]
    fn does_not_flag_a_package_used_by_another_defpackage() {
        let mut declared_packages = declared("app.lisp", "(defpackage :app (:use :cl :lib))");
        declared_packages.extend(declared("lib.lisp", "(defpackage :lib (:use :cl))"));
        let referenced_names = referenced("(defpackage :app (:use :cl :lib))");

        let summary = analyze_unused_packages(&declared_packages, &referenced_names);
        assert!(summary.unused.iter().all(|item| item.name != "lib"));
    }

    #[test]
    fn does_not_flag_a_package_reached_through_a_qualified_symbol() {
        let declared_packages = declared("lib.lisp", "(defpackage :lib (:use :cl))");
        let referenced_names = referenced("(defun f () (lib:public-api))");

        let summary = analyze_unused_packages(&declared_packages, &referenced_names);
        assert!(summary.unused.is_empty());
    }

    #[test]
    fn does_not_flag_a_package_reached_through_a_double_colon_reference() {
        let declared_packages = declared("lib.lisp", "(defpackage :lib (:use :cl))");
        let referenced_names = referenced("(defun f () (lib::internal-helper))");

        let summary = analyze_unused_packages(&declared_packages, &referenced_names);
        assert!(summary.unused.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app)").expect("parse input");
        let declared_packages =
            collect_declared_packages(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect declared packages");
        let referenced_names = collect_referenced_package_names(Dialect::Clojure, &tree)
            .expect("collect referenced package names");

        assert!(declared_packages.is_empty());
        assert!(referenced_names.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let declared_packages = declared("orphan.lisp", "(defpackage :orphan (:use :cl))");
        let summary = analyze_unused_packages(&declared_packages, &[]);

        let quiet =
            evaluate_unused_package_policy(UnusedPackagePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.unused_count, 1);

        let strict =
            evaluate_unused_package_policy(UnusedPackagePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
