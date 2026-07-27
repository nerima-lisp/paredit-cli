use proptest::prelude::*;

use super::*;
use crate::domain::{dialect::Dialect, sexpr::SyntaxTree};

fn report_for(input: &str) -> PackageReport {
    let tree = SyntaxTree::parse(input).expect("input should parse");
    build_package_report(&tree, Dialect::CommonLisp).expect("package report should build")
}

#[test]
fn reports_defpackage_options_and_in_package_forms() {
    let report = report_for(
        "(cl:defpackage #:app.main\n  (:nicknames #:main)\n  (:use #:cl #:alexandria)\n  (:export #:run #:stop)\n  (:import-from #:lib #:thing #:other))\n(in-package #:app.main)\n",
    );

    assert_eq!(report.defpackages.len(), 1);
    assert_eq!(report.in_packages.len(), 1);

    let package = &report.defpackages[0];
    assert_eq!(package.path, "0");
    assert_eq!(package.name, "#:app.main");
    assert_eq!(package.nicknames, vec!["#:main"]);
    assert_eq!(package.uses, vec!["#:cl", "#:alexandria"]);
    assert_eq!(package.exports, vec!["#:run", "#:stop"]);
    assert_eq!(package.imports.len(), 1);
    assert_eq!(package.imports[0].package, "#:lib");
    assert_eq!(package.imports[0].symbols, vec!["#:thing", "#:other"]);
    assert_eq!(package.option_count, 4);

    let in_package = &report.in_packages[0];
    assert_eq!(in_package.path, "1");
    assert_eq!(in_package.name, "#:app.main");
}

#[test]
fn scans_nested_package_forms_for_agent_reports() {
    let report = report_for("(progn (defpackage #:nested (:export #:x)) (in-package #:nested))");

    assert_eq!(report.defpackages.len(), 1);
    assert_eq!(report.defpackages[0].path, "0.1");
    assert_eq!(report.defpackages[0].exports, vec!["#:x"]);
    assert_eq!(report.in_packages.len(), 1);
    assert_eq!(report.in_packages[0].path, "0.2");
}

#[test]
fn skips_quasiquoted_in_package_template_without_error() {
    // A backquoted `(in-package ,pkg)` code template (emitted into a stream)
    // has a non-atom package designator; it must be skipped, not treated as a
    // declaration that aborts the whole report.
    let report = report_for(
        "(in-package #:real)\n(defun emit (package stream)\n  (write `(in-package ,(string-upcase (string package))) :stream stream))\n",
    );

    assert_eq!(report.in_packages.len(), 1);
    assert_eq!(report.in_packages[0].name, "#:real");
}

#[test]
fn skips_quasiquoted_defpackage_template_without_error() {
    // A backquoted `(defpackage ,(...) ...)` whose name is a computed list form
    // has no statically resolvable name and must be skipped, not crash-reported.
    let report = report_for(
        "(defmacro define-it (name) `(defpackage ,(intern (string-upcase name)) (:use #:cl)))\n(defpackage #:real (:use #:cl))\n",
    );

    assert_eq!(report.defpackages.len(), 1);
    assert_eq!(report.defpackages[0].name, "#:real");
}

proptest! {
    #[test]
    fn reports_generated_exports(
        package in "[a-z][a-z0-9-]{0,12}",
        exports in prop::collection::vec("[a-z][a-z0-9-]{0,12}", 0..8),
    ) {
        let export_form = exports
            .iter()
            .map(|symbol| format!("#:{symbol}"))
            .collect::<Vec<_>>()
            .join(" ");
        let input = format!("(defpackage #:{package} (:export {export_form}))");
        let report = report_for(&input);

        prop_assert_eq!(report.defpackages.len(), 1);
        prop_assert_eq!(&report.defpackages[0].name, &format!("#:{package}"));
        prop_assert_eq!(
            &report.defpackages[0].exports,
            &exports
                .iter()
                .map(|symbol| format!("#:{symbol}"))
                .collect::<Vec<_>>()
        );
    }
}
