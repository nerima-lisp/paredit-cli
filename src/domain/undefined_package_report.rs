//! Common Lisp "undefined package" detection: an `(in-package pkg)` form
//! naming a package that no analyzed `defpackage` form declares — most
//! often a typo (`:aap` for `:app`) that would otherwise only surface as a
//! runtime "package does not exist" error the first time the file loads,
//! rather than at review time.
//!
//! Scope: the small set of packages every Common Lisp image provides
//! without a `defpackage` form — `CL`, `COMMON-LISP`, `CL-USER`,
//! `COMMON-LISP-USER`, `KEYWORD` — are never flagged.
//!
//! [`crate::domain::unused_package_report`]'s own caveat applies here in
//! reverse: a package whose `defpackage` lives in a file outside the
//! analyzed fileset is indistinguishable, from a purely syntactic view,
//! from a genuine typo — pass the whole project, not a subset, for a
//! trustworthy result. `--fail-on-undefined` stays an opt-in gate for
//! exactly that reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use crate::domain::dialect::Dialect;
use crate::domain::package_report::build_package_report;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

/// Packages every Common Lisp image provides without a `defpackage` form.
/// [`common_lisp_symbol_reference_needle`] upcases, so these must already
/// be upper-case to compare equal.
const STANDARD_PACKAGE_NEEDLES: [&str; 5] = [
    "CL",
    "COMMON-LISP",
    "CL-USER",
    "COMMON-LISP-USER",
    "KEYWORD",
];

#[derive(Debug, Clone)]
pub struct InPackageReference {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
}

impl InPackageReference {
    pub fn new(path: PathBuf, span: ByteSpan, name: impl Into<String>) -> Self {
        Self {
            path,
            span,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UndefinedPackageItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub name: String,
}

#[derive(Debug)]
pub struct UndefinedPackageSummary {
    pub in_package_count: usize,
    pub undefined: Vec<UndefinedPackageItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UndefinedPackagePolicyOptions {
    fail_on_undefined: bool,
}

impl UndefinedPackagePolicyOptions {
    pub fn new(fail_on_undefined: bool) -> Self {
        Self { fail_on_undefined }
    }

    pub const fn fail_on_undefined(self) -> bool {
        self.fail_on_undefined
    }
}

#[derive(Debug)]
pub struct UndefinedPackagePolicy {
    pub fail_on_undefined: bool,
    pub in_package_count: usize,
    pub undefined_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every `defpackage`-declared name in one file.
pub fn collect_declared_package_names(dialect: Dialect, tree: &SyntaxTree) -> Result<Vec<String>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    Ok(report
        .defpackages
        .into_iter()
        .map(|defpackage| normalize_common_lisp_package_designator(&defpackage.name).to_owned())
        .collect())
}

/// Collects every `(in-package pkg)` form's target in one file.
pub fn collect_in_package_references(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<InPackageReference>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    Ok(report
        .in_packages
        .into_iter()
        .map(|in_package| {
            InPackageReference::new(
                path.to_path_buf(),
                in_package.span,
                normalize_common_lisp_package_designator(&in_package.name),
            )
        })
        .collect())
}

fn is_standard_package(needle: &str) -> bool {
    STANDARD_PACKAGE_NEEDLES.contains(&needle)
}

pub fn analyze_undefined_packages(
    declared: &[String],
    referenced: &[InPackageReference],
) -> UndefinedPackageSummary {
    let declared_needles: BTreeSet<String> = declared
        .iter()
        .map(|name| common_lisp_symbol_reference_needle(name))
        .collect();

    let undefined = referenced
        .iter()
        .filter(|reference| {
            let needle = common_lisp_symbol_reference_needle(&reference.name);
            !declared_needles.contains(&needle) && !is_standard_package(&needle)
        })
        .map(|reference| UndefinedPackageItem {
            path: reference.path.clone(),
            span: reference.span,
            name: reference.name.clone(),
        })
        .collect();

    UndefinedPackageSummary {
        in_package_count: referenced.len(),
        undefined,
    }
}

pub fn evaluate_undefined_package_policy(
    options: UndefinedPackagePolicyOptions,
    summary: &UndefinedPackageSummary,
) -> UndefinedPackagePolicy {
    let undefined_count = summary.undefined.len();
    let mut violations = Vec::new();
    if options.fail_on_undefined() && undefined_count > 0 {
        violations.push(format!("undefined_count {undefined_count} exceeds 0"));
    }

    UndefinedPackagePolicy {
        fail_on_undefined: options.fail_on_undefined(),
        in_package_count: summary.in_package_count,
        undefined_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_package_names(Dialect::CommonLisp, &tree)
            .expect("collect declared package names")
    }

    fn referenced(path: &str, input: &str) -> Vec<InPackageReference> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_in_package_references(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect in-package references")
    }

    #[test]
    fn flags_an_in_package_form_naming_an_undeclared_package() {
        let referenced_names = referenced("app.lisp", "(in-package :aap)");
        let summary = analyze_undefined_packages(&[], &referenced_names);

        assert_eq!(summary.in_package_count, 1);
        assert_eq!(summary.undefined.len(), 1);
        assert_eq!(summary.undefined[0].name, "aap");
    }

    #[test]
    fn does_not_flag_an_in_package_form_naming_a_declared_package() {
        let declared_names = declared("(defpackage :app (:use :cl))");
        let referenced_names = referenced("app.lisp", "(in-package :app)");

        let summary = analyze_undefined_packages(&declared_names, &referenced_names);
        assert!(summary.undefined.is_empty());
    }

    #[test]
    fn does_not_flag_the_standard_packages() {
        let referenced_names = referenced(
            "app.lisp",
            "(in-package :cl-user)\n(in-package :common-lisp-user)\n(in-package :cl)",
        );

        let summary = analyze_undefined_packages(&[], &referenced_names);
        assert!(summary.undefined.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app)").expect("parse input");
        let declared_names = collect_declared_package_names(Dialect::Clojure, &tree)
            .expect("collect declared package names");
        let referenced_names =
            collect_in_package_references(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect in-package references");

        assert!(declared_names.is_empty());
        assert!(referenced_names.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let referenced_names = referenced("app.lisp", "(in-package :aap)");
        let summary = analyze_undefined_packages(&[], &referenced_names);

        let quiet =
            evaluate_undefined_package_policy(UndefinedPackagePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.undefined_count, 1);

        let strict =
            evaluate_undefined_package_policy(UndefinedPackagePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
