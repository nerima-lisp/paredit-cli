//! Common Lisp package-identity conflict detection: two or more *distinct*
//! `defpackage` forms — in one file or across several — that claim the same
//! package identity, whether through a duplicated primary name, a nickname
//! that collides with another package's primary name, or two packages both
//! claiming the same nickname.
//!
//! A package's `:nicknames` share the same designator namespace as its
//! primary name: within one running image, no two packages may resolve to
//! the same name. A genuine collision here is a load-order-dependent bug —
//! whichever `defpackage` form runs last silently redefines (or steals the
//! identity of) the other — not a mere style nit like an unused nickname
//! ([`crate::unused_nickname_report::domain`]).
//!
//! Scope: a package declaring its own primary name as one of its own
//! nicknames (`(defpackage :app (:nicknames :app))`, redundant but
//! harmless) is not a conflict — only identifiers contributed by *distinct*
//! `defpackage` occurrences collide.
//!
//! Built on the same [`crate::package_report::domain::build_package_report`]
//! extraction [`crate::unused_package_report::domain`] and
//! [`crate::unused_nickname_report::domain`] already reuse.

use std::path::{Path, PathBuf};

use anyhow::Result;
use std::collections::BTreeMap;

use crate::package_report::domain::build_package_report;
use paredit_core_syntax::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};

#[derive(Debug, Clone)]
pub struct DeclaredPackageIdentifier {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub identifier: String,
    pub is_primary_name: bool,
}

impl DeclaredPackageIdentifier {
    pub fn new(
        path: PathBuf,
        span: ByteSpan,
        package: impl Into<String>,
        identifier: impl Into<String>,
        is_primary_name: bool,
    ) -> Self {
        Self {
            path,
            span,
            package: package.into(),
            identifier: identifier.into(),
            is_primary_name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageConflictOccurrence {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub is_primary_name: bool,
}

#[derive(Debug, Clone)]
pub struct PackageConflictItem {
    pub identifier: String,
    pub occurrences: Vec<PackageConflictOccurrence>,
}

#[derive(Debug)]
pub struct PackageConflictSummary {
    pub declared_identifier_count: usize,
    pub conflicts: Vec<PackageConflictItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageConflictPolicyOptions {
    fail_on_conflict: bool,
}

impl PackageConflictPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_conflict: bool) -> Self {
        Self { fail_on_conflict }
    }

    #[must_use]
    pub const fn fail_on_conflict(self) -> bool {
        self.fail_on_conflict
    }
}

#[derive(Debug)]
pub struct PackageConflictPolicy {
    pub fail_on_conflict: bool,
    pub declared_identifier_count: usize,
    pub conflict_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every identifier (primary name, then each nickname) a
/// `defpackage` form declares in one file, each tagged with the span of the
/// declaring form so distinct occurrences can be told apart from repeated
/// identifiers within the same occurrence.
pub fn collect_declared_package_identifiers(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<DeclaredPackageIdentifier>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    let mut identifiers = Vec::new();
    for defpackage in report.defpackages {
        let package = normalize_common_lisp_package_designator(&defpackage.name).to_owned();
        let span = defpackage.span;
        identifiers.push(DeclaredPackageIdentifier::new(
            path.to_path_buf(),
            span,
            package.clone(),
            package.clone(),
            true,
        ));
        for nickname in &defpackage.nicknames {
            identifiers.push(DeclaredPackageIdentifier::new(
                path.to_path_buf(),
                span,
                package.clone(),
                normalize_common_lisp_package_designator(nickname),
                false,
            ));
        }
    }
    Ok(identifiers)
}

#[must_use]
pub fn analyze_package_conflicts(declared: &[DeclaredPackageIdentifier]) -> PackageConflictSummary {
    let mut groups: BTreeMap<String, Vec<&DeclaredPackageIdentifier>> = BTreeMap::new();
    for identifier in declared {
        groups
            .entry(common_lisp_symbol_reference_needle(&identifier.identifier))
            .or_default()
            .push(identifier);
    }

    let mut conflicts = Vec::new();
    for group in groups.into_values() {
        let mut occurrences: Vec<PackageConflictOccurrence> = Vec::new();
        for identifier in &group {
            if occurrences.iter().any(|occurrence| {
                occurrence.path == identifier.path && occurrence.span == identifier.span
            }) {
                continue;
            }
            occurrences.push(PackageConflictOccurrence {
                path: identifier.path.clone(),
                span: identifier.span,
                package: identifier.package.clone(),
                is_primary_name: identifier.is_primary_name,
            });
        }

        if occurrences.len() < 2 {
            continue;
        }

        conflicts.push(PackageConflictItem {
            identifier: group[0].identifier.clone(),
            occurrences,
        });
    }

    PackageConflictSummary {
        declared_identifier_count: declared.len(),
        conflicts,
    }
}

#[must_use]
pub fn evaluate_package_conflict_policy(
    options: PackageConflictPolicyOptions,
    summary: &PackageConflictSummary,
) -> PackageConflictPolicy {
    let conflict_count = summary.conflicts.len();
    let mut violations = Vec::new();
    if options.fail_on_conflict() && conflict_count > 0 {
        violations.push(format!("conflict_count {conflict_count} exceeds 0"));
    }

    PackageConflictPolicy {
        fail_on_conflict: options.fail_on_conflict(),
        declared_identifier_count: summary.declared_identifier_count,
        conflict_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(path: &str, input: &str) -> Vec<DeclaredPackageIdentifier> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_package_identifiers(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared package identifiers")
    }

    #[test]
    fn flags_two_distinct_packages_declaring_the_same_primary_name() {
        let mut declared_identifiers = declared("a.lisp", "(defpackage :app (:use :cl))");
        declared_identifiers.extend(declared("b.lisp", "(defpackage :app (:use :cl))"));

        let summary = analyze_package_conflicts(&declared_identifiers);
        assert_eq!(summary.conflicts.len(), 1);
        assert_eq!(summary.conflicts[0].identifier, "app");
        assert_eq!(summary.conflicts[0].occurrences.len(), 2);
    }

    #[test]
    fn flags_a_nickname_that_collides_with_another_packages_primary_name() {
        let mut declared_identifiers = declared("a.lisp", "(defpackage :util (:nicknames :app))");
        declared_identifiers.extend(declared("b.lisp", "(defpackage :app (:use :cl))"));

        let summary = analyze_package_conflicts(&declared_identifiers);
        assert_eq!(summary.conflicts.len(), 1);
        assert_eq!(summary.conflicts[0].identifier, "app");
    }

    #[test]
    fn flags_two_packages_declaring_the_same_nickname() {
        let mut declared_identifiers = declared("a.lisp", "(defpackage :a (:nicknames :x))");
        declared_identifiers.extend(declared("b.lisp", "(defpackage :b (:nicknames :x))"));

        let summary = analyze_package_conflicts(&declared_identifiers);
        assert_eq!(summary.conflicts.len(), 1);
        assert_eq!(summary.conflicts[0].identifier, "x");
        assert_eq!(summary.conflicts[0].occurrences.len(), 2);
    }

    #[test]
    fn does_not_flag_a_package_declaring_its_own_name_as_its_own_nickname() {
        let declared_identifiers = declared("a.lisp", "(defpackage :app (:nicknames :app))");

        let summary = analyze_package_conflicts(&declared_identifiers);
        assert!(summary.conflicts.is_empty());
    }

    #[test]
    fn does_not_flag_distinct_packages_with_distinct_identities() {
        let mut declared_identifiers = declared("a.lisp", "(defpackage :app (:nicknames :a))");
        declared_identifiers.extend(declared("b.lisp", "(defpackage :lib (:nicknames :l))"));

        let summary = analyze_package_conflicts(&declared_identifiers);
        assert!(summary.conflicts.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app)").expect("parse input");
        let declared_identifiers = collect_declared_package_identifiers(
            &PathBuf::from("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("collect declared package identifiers");
        assert!(declared_identifiers.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let mut declared_identifiers = declared("a.lisp", "(defpackage :app (:use :cl))");
        declared_identifiers.extend(declared("b.lisp", "(defpackage :app (:use :cl))"));
        let summary = analyze_package_conflicts(&declared_identifiers);

        let quiet =
            evaluate_package_conflict_policy(PackageConflictPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.conflict_count, 1);

        let strict =
            evaluate_package_conflict_policy(PackageConflictPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
