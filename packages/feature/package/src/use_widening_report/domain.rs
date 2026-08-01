//! Common Lisp `defpackage` `:use` widening-risk detection: every package a
//! `defpackage` `:use`s imports that package's *entire* exported symbol
//! table into the importing package's namespace, implicitly widening it and
//! risking a future name collision. `:import-from` names an explicit symbol
//! list instead and carries no such risk. This report flags every `:use`d
//! package per `defpackage` as a widening-risk finding, so a user auditing a
//! codebase's package hygiene can see where explicit `:import-from` could
//! replace a blanket `:use`.
//!
//! Built on the same [`crate::package_report::domain::build_package_report`]
//! extraction other reports in this package use; `:use` is already collected
//! there as [`crate::package_report::domain::PackageDefinitionReport::uses`].
//!
//! Scope: Common Lisp only, since `defpackage`/`:use`/`:import-from` are
//! specific to the CL package system.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::normalize_common_lisp_package_designator;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use serde_json::{Value, json};

use crate::package_report::domain::build_package_report;

/// A `:use`d package flagged for the namespace-widening risk it carries.
#[derive(Debug, Clone)]
pub struct UseWideningItem {
    pub span: ByteSpan,
    /// The `defpackage` doing the `:use`ing.
    pub declaring_package: String,
    /// The package it `:use`s, whose whole exported symbol table is pulled in.
    pub used_package: String,
}

impl Finding for UseWideningItem {
    fn kind(&self) -> &'static str {
        "use"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.declaring_package.clone(), self.used_package.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("declaring_package", json!(self.declaring_package)),
            ("used_package", json!(self.used_package)),
        ]
    }
}

/// Collects one `:use`-widening finding per `:use`d package, for every
/// `defpackage` form in one file.
///
/// A `build_package_report` failure (a `defpackage` written in a shape this
/// tool does not read) degrades to no findings for the file, the same
/// conservative default the report envelope already uses for an unmodelled
/// dialect: the file is not a defect, the tool is just declining to read
/// part of it.
#[must_use]
pub fn build_use_widening_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<UseWideningItem> {
    let dialect_modelled = dialect == Dialect::CommonLisp;
    let mut findings = Vec::new();
    let mut defpackage_count = 0usize;
    let mut use_clause_count = 0usize;

    if dialect_modelled {
        if let Ok(report) = build_package_report(tree, dialect) {
            defpackage_count = report.defpackages.len();
            for defpackage in &report.defpackages {
                let declaring_package =
                    normalize_common_lisp_package_designator(&defpackage.name).to_owned();
                for used in &defpackage.uses {
                    use_clause_count += 1;
                    findings.push(UseWideningItem {
                        span: defpackage.span,
                        declaring_package: declaring_package.clone(),
                        used_package: normalize_common_lisp_package_designator(used).to_owned(),
                    });
                }
            }
        }
    }

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect_modelled,
        tree.source(),
        findings,
        vec![
            ("defpackage_count", json!(defpackage_count)),
            ("use_clause_count", json!(use_clause_count)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<UseWideningItem> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_use_widening_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn flags_every_used_package() {
        let report = report("(defpackage :app (:use :cl :lib))");
        assert_eq!(report.findings.len(), 2);
        let used = report
            .findings
            .iter()
            .map(|finding| finding.used_package.as_str())
            .collect::<Vec<_>>();
        assert!(used.contains(&"cl"));
        assert!(used.contains(&"lib"));
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.declaring_package == "app")
        );
    }

    #[test]
    fn does_not_flag_a_bare_defpackage_with_no_use_clause() {
        let report = report("(defpackage :app)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn does_not_flag_import_from_as_widening() {
        let report = report("(defpackage :app (:import-from :lib :foo))");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn counts_defpackages_and_use_clauses_in_the_summary() {
        let report = report("(defpackage :app (:use :cl :lib))\n(defpackage :other (:use :cl))");
        assert_eq!(report.summary[0], ("defpackage_count", json!(2)));
        assert_eq!(report.summary[1], ("use_clause_count", json!(3)));
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(ns app)", Dialect::Clojure).expect("parse");
        let report = build_use_widening_report(Path::new("app.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defpackage :a (:use :cl))\n(defpackage :b (:use :cl :lib))");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }
}
