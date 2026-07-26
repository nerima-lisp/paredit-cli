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
//! Both sides of the comparison are canonicalized by the project layer's
//! [`PackageId`] rather than by a bare upcasing, which is what makes a
//! *nickname* a legitimate designator: `(defpackage :my-app (:nicknames :app))`
//! declares two names for one package, and `(in-package :app)` is not a typo.
//! Reading only the primary name reported it as undefined.
//!
//! The canonicalization can only ever *add* declared names, never remove a
//! reference's grounds for matching: a designator the layer cannot
//! canonicalize keeps the bare-name comparison it always had.
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
use crate::domain::semantics::project::service::canonical_package_id;
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

/// Collects every name a `defpackage` in one file declares.
///
/// A package's `:nicknames` are declared names too — CLHS makes a nickname a
/// designator for the same package — so a file that declares them and then
/// says `(in-package <nickname>)` is correct code. Collecting only the primary
/// name reported that as an undefined package.
pub fn collect_declared_package_names(dialect: Dialect, tree: &SyntaxTree) -> Result<Vec<String>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    Ok(report
        .defpackages
        .into_iter()
        .flat_map(|defpackage| {
            std::iter::once(defpackage.name)
                .chain(defpackage.nicknames)
                .map(|name| normalize_common_lisp_package_designator(&name).to_owned())
        })
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

/// One designator's canonical identity, as the project layer sees it.
///
/// [`PackageId`] folds the two standard nicknames (`CL`/`COMMON-LISP`,
/// `CL-USER`/`COMMON-LISP-USER`) on top of the upcasing the bare needle does,
/// so the same package written two ways compares equal. Both sides go through
/// it, which is the whole of what makes the comparison an identity test rather
/// than a spelling test.
fn package_identity(designator: &str) -> String {
    canonical_package_id(designator).as_str().to_owned()
}

pub fn analyze_undefined_packages(
    declared: &[String],
    referenced: &[InPackageReference],
) -> UndefinedPackageSummary {
    let declared_identities: BTreeSet<String> =
        declared.iter().map(|name| package_identity(name)).collect();

    let undefined = referenced
        .iter()
        .filter(|reference| {
            let needle = common_lisp_symbol_reference_needle(&reference.name);
            !declared_identities.contains(&package_identity(&reference.name))
                && !is_standard_package(&needle)
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
    fn does_not_flag_an_in_package_form_naming_a_declared_nickname() {
        // CLHS makes a nickname a designator for the same package, so this is
        // correct code. Reading only the primary name called it a typo.
        let declared_names = declared("(defpackage :my-application (:nicknames :app) (:use :cl))");
        let referenced_names = referenced("app.lisp", "(in-package :app)");

        let summary = analyze_undefined_packages(&declared_names, &referenced_names);
        assert!(summary.undefined.is_empty());
    }

    #[test]
    fn still_flags_a_typo_that_is_neither_the_name_nor_a_nickname() {
        // The nickname must widen what counts as declared, not disable the
        // check: `aap` is still nobody's designator.
        let declared_names = declared("(defpackage :my-application (:nicknames :app) (:use :cl))");
        let referenced_names = referenced("app.lisp", "(in-package :aap)");

        let summary = analyze_undefined_packages(&declared_names, &referenced_names);
        assert_eq!(summary.undefined.len(), 1);
        assert_eq!(summary.undefined[0].name, "aap");
    }

    #[test]
    fn two_packages_of_the_same_bare_name_stay_two_packages() {
        // `app` and `test` both declare a package; an `in-package` naming a
        // third is undefined regardless of how many others exist. The
        // identity comparison must not collapse distinct designators.
        let declared_names =
            declared("(defpackage :app (:use :cl))\n(defpackage :test (:use :cl))");
        let referenced_names = referenced("app.lisp", "(in-package :app)\n(in-package :prod)");

        let summary = analyze_undefined_packages(&declared_names, &referenced_names);
        assert_eq!(summary.undefined.len(), 1);
        assert_eq!(summary.undefined[0].name, "prod");
    }

    #[test]
    fn a_designator_spelled_four_ways_names_one_package() {
        // Symbol, keyword, uninterned symbol, and string all designate the
        // same package. Routing both sides through the project layer's
        // identity is what makes them compare equal.
        let declared_names = declared(r#"(defpackage "APP" (:use :cl))"#);
        for reference in [
            "(in-package app)",
            "(in-package :app)",
            "(in-package #:app)",
            r#"(in-package "APP")"#,
        ] {
            let referenced_names = referenced("app.lisp", reference);
            let summary = analyze_undefined_packages(&declared_names, &referenced_names);
            assert!(summary.undefined.is_empty(), "{reference}");
        }
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
