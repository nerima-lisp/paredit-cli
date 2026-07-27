//! Common Lisp "unused export" detection: a symbol named in a `defpackage`
//! `:export` clause that is never reached by a qualified symbol reference
//! (`pkg:sym`) anywhere in the analyzed fileset — the per-symbol analog of
//! [`crate::unused_package_report::domain`]'s whole-package check, one
//! level more precise: a package can be genuinely used (via some *other*
//! export) while still carrying an export nobody actually reads.
//!
//! Same caveat as `unused-packages` and `unused-definitions`: an export
//! consumed only by files outside the analyzed fileset (a library's public
//! API used solely by downstream consumers) is indistinguishable, from a
//! purely syntactic view, from a genuinely dead export — `--fail-on-unused`
//! stays an opt-in gate for exactly that reason.
//!
//! Built on [`crate::package_report::domain::build_package_report`] for the
//! declared `:export` lists and
//! [`crate::dependency_report::domain::build_dependency_report`]'s
//! `QualifiedSymbol` items for references — the same two primitives
//! [`crate::unused_package_report::domain`] already reuses, read one level
//! deeper: each qualified reference's raw text (`pkg:sym`/`pkg::sym`) is
//! split into its package and symbol parts so exports can be matched by
//! *symbol name*, not just by declaring package.
//!
//! Scope: Common Lisp only. Reported spans point at the *declaring
//! `defpackage` form*, not the individual `:export` atom — this codebase
//! does not currently track a per-export-symbol span, only a per-option
//! list of names, so a file with several unused exports in one `defpackage`
//! reports the same span for each.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::dependency_report::domain::{DependencyKind, build_dependency_report};
use crate::package_report::domain::build_package_report;
use paredit_core_syntax::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};

#[derive(Debug, Clone)]
pub struct DeclaredExport {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub symbol: String,
}

impl DeclaredExport {
    pub fn new(
        path: PathBuf,
        span: ByteSpan,
        package: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        Self {
            path,
            span,
            package: package.into(),
            symbol: symbol.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReferencedSymbol {
    pub package: String,
    pub symbol: String,
}

#[derive(Debug, Clone)]
pub struct UnusedExportItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub symbol: String,
}

#[derive(Debug)]
pub struct UnusedExportSummary {
    pub declared_count: usize,
    pub unused: Vec<UnusedExportItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusedExportPolicyOptions {
    fail_on_unused: bool,
}

impl UnusedExportPolicyOptions {
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
pub struct UnusedExportPolicy {
    pub fail_on_unused: bool,
    pub declared_count: usize,
    pub unused_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every `(:export ...)` symbol declared by a `defpackage` form in
/// one file, paired with that package's own (normalized) name.
pub fn collect_declared_exports(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<DeclaredExport>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    Ok(report
        .defpackages
        .into_iter()
        .flat_map(|defpackage| {
            let path = path.to_path_buf();
            let package = normalize_common_lisp_package_designator(&defpackage.name).to_owned();
            let span = defpackage.span;
            defpackage.exports.into_iter().map(move |export| {
                DeclaredExport::new(
                    path.clone(),
                    span,
                    package.clone(),
                    normalize_common_lisp_package_designator(&export),
                )
            })
        })
        .collect())
}

/// Splits a qualified reference's raw text (as captured by
/// [`DependencyKind::QualifiedSymbol`]'s `source`, e.g. `pkg:sym` or
/// `pkg::sym`) into the symbol part, given the already-extracted package
/// part (`target`).
fn qualified_symbol_name<'a>(source: &'a str, target: &str) -> Option<&'a str> {
    let rest = source.strip_prefix(target)?;
    rest.strip_prefix("::").or_else(|| rest.strip_prefix(':'))
}

/// Collects every qualified-symbol reference (`pkg:sym`/`pkg::sym`) in one
/// file, split into its package and symbol parts.
pub fn collect_referenced_symbols(
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<ReferencedSymbol>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let dependencies = build_dependency_report(tree, dialect)?;
    Ok(dependencies
        .dependencies
        .into_iter()
        .filter(|item| item.kind == DependencyKind::QualifiedSymbol)
        .filter_map(|item| {
            let source = item.source?;
            let symbol = qualified_symbol_name(&source, &item.target)?;
            Some(ReferencedSymbol {
                package: normalize_common_lisp_package_designator(&item.target).to_owned(),
                symbol: normalize_common_lisp_package_designator(symbol).to_owned(),
            })
        })
        .collect())
}

#[must_use]
pub fn analyze_unused_exports(
    declared: &[DeclaredExport],
    referenced: &[ReferencedSymbol],
) -> UnusedExportSummary {
    let referenced_needles: BTreeSet<(String, String)> = referenced
        .iter()
        .map(|reference| {
            (
                common_lisp_symbol_reference_needle(&reference.package),
                common_lisp_symbol_reference_needle(&reference.symbol),
            )
        })
        .collect();

    let unused = declared
        .iter()
        .filter(|export| {
            let needle = (
                common_lisp_symbol_reference_needle(&export.package),
                common_lisp_symbol_reference_needle(&export.symbol),
            );
            !referenced_needles.contains(&needle)
        })
        .map(|export| UnusedExportItem {
            path: export.path.clone(),
            span: export.span,
            package: export.package.clone(),
            symbol: export.symbol.clone(),
        })
        .collect();

    UnusedExportSummary {
        declared_count: declared.len(),
        unused,
    }
}

#[must_use]
pub fn evaluate_unused_export_policy(
    options: UnusedExportPolicyOptions,
    summary: &UnusedExportSummary,
) -> UnusedExportPolicy {
    let unused_count = summary.unused.len();
    let mut violations = Vec::new();
    if options.fail_on_unused() && unused_count > 0 {
        violations.push(format!("unused_count {unused_count} exceeds 0"));
    }

    UnusedExportPolicy {
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

    fn declared(path: &str, input: &str) -> Vec<DeclaredExport> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_exports(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared exports")
    }

    fn referenced(input: &str) -> Vec<ReferencedSymbol> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_referenced_symbols(Dialect::CommonLisp, &tree).expect("collect referenced symbols")
    }

    #[test]
    fn flags_an_export_never_referenced_by_a_qualified_symbol() {
        let declared = declared("lib.lisp", "(defpackage :lib (:export #:dead))");
        let summary = analyze_unused_exports(&declared, &[]);

        assert_eq!(summary.declared_count, 1);
        assert_eq!(summary.unused.len(), 1);
        assert_eq!(summary.unused[0].symbol, "dead");
        assert_eq!(summary.unused[0].package, "lib");
    }

    #[test]
    fn does_not_flag_an_export_reached_by_a_single_colon_reference() {
        let declared = declared("lib.lisp", "(defpackage :lib (:export #:live))");
        let referenced = referenced("(defun f () (lib:live))");

        let summary = analyze_unused_exports(&declared, &referenced);
        assert!(summary.unused.is_empty());
    }

    #[test]
    fn does_not_flag_an_export_reached_by_a_double_colon_reference() {
        let declared = declared("lib.lisp", "(defpackage :lib (:export #:live))");
        let referenced = referenced("(defun f () (lib::live))");

        let summary = analyze_unused_exports(&declared, &referenced);
        assert!(summary.unused.is_empty());
    }

    #[test]
    fn does_not_confuse_two_packages_exporting_the_same_symbol_name() {
        let mut declared_exports = declared("a.lisp", "(defpackage :a (:export #:run))");
        declared_exports.extend(declared("b.lisp", "(defpackage :b (:export #:run))"));
        let referenced_symbols = referenced("(defun f () (a:run))");

        let summary = analyze_unused_exports(&declared_exports, &referenced_symbols);
        assert_eq!(summary.unused.len(), 1);
        assert_eq!(summary.unused[0].package, "b");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app)").expect("parse input");
        let declared = collect_declared_exports(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
            .expect("collect declared exports");
        let referenced = collect_referenced_symbols(Dialect::Clojure, &tree)
            .expect("collect referenced symbols");

        assert!(declared.is_empty());
        assert!(referenced.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let declared = declared("lib.lisp", "(defpackage :lib (:export #:dead))");
        let summary = analyze_unused_exports(&declared, &[]);

        let quiet = evaluate_unused_export_policy(UnusedExportPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.unused_count, 1);

        let strict = evaluate_unused_export_policy(UnusedExportPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
