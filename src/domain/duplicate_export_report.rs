//! Common Lisp duplicate-export detection: a `defpackage` that names the
//! same symbol more than once across its `:export` clauses. Exporting a
//! symbol twice is pure redundancy — the second entry does nothing — and is
//! almost always a merge-conflict artifact or a copy-paste slip. There is no
//! meaning to a repeated export, so this report has no false positives.
//!
//! Built on the same [`crate::domain::package_report::build_package_report`]
//! extraction [`crate::domain::unused_export_report`] uses, so it inherits
//! the same designator handling — a symbol written `#:foo`, `:foo`, or `foo`
//! across two `:export` entries counts as the same export.
//!
//! Scope: Common Lisp only; non-Common-Lisp files declare no `defpackage`
//! and contribute nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use crate::domain::dialect::Dialect;
use crate::domain::package_report::build_package_report;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

#[derive(Debug, Clone)]
pub struct DuplicateExportItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub symbol: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateExportSummary {
    pub defpackage_count: usize,
    pub duplicates: Vec<DuplicateExportItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateExportPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateExportPolicyOptions {
    #[must_use]
    pub fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateExportPolicy {
    pub fail_on_duplicate: bool,
    pub defpackage_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every duplicated `:export` symbol from every `defpackage` in one
/// file, along with the total number of `defpackage` forms scanned.
pub fn collect_duplicate_exports(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateExportItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let report = build_package_report(tree, dialect)?;
    let mut defpackage_count = 0;
    let mut duplicates = Vec::new();

    for defpackage in report.defpackages {
        defpackage_count += 1;
        let package = normalize_common_lisp_package_designator(&defpackage.name).to_owned();

        // Preserve first-seen spelling and export order while counting.
        let mut order: Vec<String> = Vec::new();
        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for export in &defpackage.exports {
            let symbol = normalize_common_lisp_package_designator(export);
            let needle = common_lisp_symbol_reference_needle(symbol);
            let entry = counts.entry(needle.clone()).or_insert_with(|| {
                order.push(needle);
                (symbol.to_owned(), 0)
            });
            entry.1 += 1;
        }

        for needle in order {
            let (symbol, occurrence_count) = &counts[&needle];
            if *occurrence_count < 2 {
                continue;
            }
            duplicates.push(DuplicateExportItem {
                path: path.to_path_buf(),
                span: defpackage.span,
                package: package.clone(),
                symbol: symbol.clone(),
                occurrence_count: *occurrence_count,
            });
        }
    }

    Ok((defpackage_count, duplicates))
}

#[must_use]
pub fn summarize_duplicate_exports(
    defpackage_count: usize,
    duplicates: Vec<DuplicateExportItem>,
) -> DuplicateExportSummary {
    DuplicateExportSummary {
        defpackage_count,
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_export_policy(
    options: DuplicateExportPolicyOptions,
    summary: &DuplicateExportSummary,
) -> DuplicateExportPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateExportPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        defpackage_count: summary.defpackage_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateExportItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_exports(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate exports")
    }

    #[test]
    fn flags_a_symbol_exported_twice() {
        let (defpackage_count, duplicates) =
            duplicates("(defpackage :app (:export #:run #:stop #:run))");
        assert_eq!(defpackage_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].symbol, "run");
        assert_eq!(duplicates[0].occurrence_count, 2);
        assert_eq!(duplicates[0].package, "app");
    }

    #[test]
    fn folds_symbol_case_and_designator_style() {
        // `#:run` and `:RUN` name the same export.
        let (_, duplicates) = duplicates("(defpackage :app (:export #:run :RUN))");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn flags_duplicates_across_two_export_clauses() {
        let (_, duplicates) = duplicates("(defpackage :app (:export #:a) (:export #:a))");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_exports() {
        let (defpackage_count, duplicates) = duplicates("(defpackage :app (:export #:a #:b #:c))");
        assert_eq!(defpackage_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_confuse_exports_of_two_packages() {
        let (defpackage_count, duplicates) =
            duplicates("(defpackage :a (:export #:run))\n(defpackage :b (:export #:run))");
        assert_eq!(defpackage_count, 2);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(defpackage :app (:export #:a #:a))").expect("parse input");
        let (defpackage_count, duplicates) =
            collect_duplicate_exports(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate exports");
        assert_eq!(defpackage_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (defpackage_count, items) = duplicates("(defpackage :app (:export #:a #:a))");
        let summary = summarize_duplicate_exports(defpackage_count, items);

        let quiet =
            evaluate_duplicate_export_policy(DuplicateExportPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict =
            evaluate_duplicate_export_policy(DuplicateExportPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
