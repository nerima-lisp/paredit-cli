use paredit_core_syntax::sexpr::SyntaxTree;
use proptest::prelude::*;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};

use super::*;

fn report(input: &str) -> DependencyReport {
    let tree = SyntaxTree::parse(input).expect("parse fixture");
    build_dependency_report(&tree, Dialect::CommonLisp).expect("build dependency report")
}

fn contains_dependency(report: &DependencyReport, kind: DependencyKind, target: &str) -> bool {
    report
        .dependencies
        .iter()
        .any(|dependency| dependency.kind == kind && dependency.target == target)
}

fn system_edges(input: &str) -> Vec<(String, String)> {
    let tree = SyntaxTree::parse(input).expect("parse fixture");
    build_system_dependency_edges(&tree, Dialect::CommonLisp).expect("build system edges")
}

#[test]
fn collects_defsystem_depends_on_edges_with_the_declaring_system_as_source() {
    let edges =
        system_edges(r#"(asdf:defsystem "my-system" :depends-on ("alexandria" "cl-ppcre"))"#);
    assert_eq!(
        edges,
        vec![
            ("my-system".to_owned(), "alexandria".to_owned()),
            ("my-system".to_owned(), "cl-ppcre".to_owned()),
        ]
    );
}

#[test]
fn ignores_forms_that_are_not_defsystem() {
    let edges = system_edges("(defun f (x) x)");
    assert!(edges.is_empty());
}

#[test]
fn returns_empty_when_defsystem_has_no_depends_on() {
    let edges = system_edges(r#"(asdf:defsystem "my-system" :version "1.0")"#);
    assert!(edges.is_empty());
}

#[test]
fn orders_dependency_items_by_span_kind_and_target() {
    let earlier = DependencyReportItem::new(
        DependencyKind::Require,
        "alpha",
        "a.lisp",
        ByteSpan::new(ByteOffset::new(1), ByteOffset::new(2)),
        None,
    );
    let later = DependencyReportItem::new(
        DependencyKind::Provide,
        "beta",
        "a.lisp",
        ByteSpan::new(ByteOffset::new(3), ByteOffset::new(4)),
        None,
    );

    assert!(earlier.cmp_position(&later).is_lt());
    assert!(later.cmp_position(&earlier).is_gt());
}

#[test]
fn collects_runtime_asdf_and_qualified_symbol_dependencies() {
    let report = report(
        r#"(asdf:defsystem #:demo
  :depends-on (#:alexandria "cl-ppcre")
  :components ((:file "package") (:module "src" :components ((:file "core")))))
(require :swank)
(provide 'demo.core)
(load "extra.lisp")
(use-package #:alexandria)
(import 'uiop:ensure-directory-pathname)
(defun render ()
  (alexandria:when-let ((x 1))
    (uiop:ensure-directory-pathname x)))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::AsdfDependsOn,
        "\"cl-ppcre\""
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::AsdfComponent,
        ":file \"package\""
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::Require,
        ":swank"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::UsePackage,
        "#:alexandria"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "alexandria"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "uiop"
    ));
}

#[test]
fn collects_require_provide_and_load_dependencies_in_emacs_lisp() {
    // `require`/`provide`/`load`/`load-file`/`load-library` are the same
    // functions with the same load-order semantics in Emacs Lisp as in
    // Common Lisp, so an Emacs Lisp file's dependency inventory should not
    // come back empty just because the file isn't Common Lisp.
    let tree = SyntaxTree::parse(
        r#"(require 'cl-lib)
(load "extra.el")
(load-library "helper")
(provide 'demo)"#,
    )
    .expect("parse fixture");
    let report =
        build_dependency_report(&tree, Dialect::EmacsLisp).expect("build dependency report");

    assert!(contains_dependency(
        &report,
        DependencyKind::Require,
        "cl-lib"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::Load,
        "\"extra.el\""
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::LoadLibrary,
        "\"helper\""
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::Provide,
        "demo"
    ));
}

#[test]
fn excludes_use_package_and_import_from_emacs_lisp_dependency_report() {
    // Emacs Lisp's `use-package` macro (declarative package *configuration*)
    // and `import` (not a standard Emacs Lisp form) are unrelated to Common
    // Lisp's package-system forms of the same name, so recognizing them
    // here would misclassify an ordinary Emacs Lisp construct as a
    // dependency.
    let tree = SyntaxTree::parse(
        r#"(use-package magit
  :config (setq magit-diff-refine-hunk t))
(import 'not-a-real-emacs-lisp-form)"#,
    )
    .expect("parse fixture");
    let report =
        build_dependency_report(&tree, Dialect::EmacsLisp).expect("build dependency report");

    assert!(report.dependencies.is_empty());
}

#[test]
fn includes_defpackage_dependencies_from_package_report() {
    let report = report(
        r#"(defpackage #:demo.core
  (:use #:cl #:alexandria)
  (:import-from #:uiop #:pathname-directory-pathname))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::DefpackageUse,
        "#:alexandria"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::DefpackageImportFrom,
        "#:uiop"
    ));
}

#[test]
fn excludes_symbol_macrolet_binding_names_and_shadowed_body_references_from_dependencies() {
    let report = report(
        r#"(defun caller ()
  (cl:symbol-macrolet ((cl-user:helper (uiop:ensure-pathname x)))
    cl-user:helper))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "cl"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "uiop"
    ));
    assert_eq!(
        report
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.kind == DependencyKind::QualifiedSymbol && dependency.target == "cl-user"
            })
            .count(),
        0
    );
}

#[test]
fn excludes_macrolet_binding_names_from_dependencies_but_keeps_expander_dependencies() {
    let report = report(
        r#"(defun caller ()
  (cl:macrolet ((cl-user:helper (x) (uiop:ensure-pathname x)))
    (cl-user:helper value)))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "cl"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "uiop"
    ));
    assert_eq!(
        report
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.kind == DependencyKind::QualifiedSymbol && dependency.target == "cl-user"
            })
            .count(),
        0
    );
}

#[test]
fn skips_quoted_dependency_candidates_in_common_lisp_dependency_report() {
    let report = report(
        r#"(defun caller ()
  '(cl-user:quoted uiop:ensure-pathname))"#,
    );

    assert_eq!(
        report
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind == DependencyKind::QualifiedSymbol)
            .count(),
        0
    );
}

#[test]
fn reports_unquoted_dependency_candidates_inside_quasiquote() {
    let report = report(
        r#"(defun caller ()
  `(list ',cl-user:quoted ,uiop:ensure-pathname ,@(cl-user:helper value)))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "uiop"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "cl-user"
    ));
    // Three distinct qualified-symbol occurrences: `,uiop:ensure-pathname`
    // and `,@(cl-user:helper value)` are ordinary unquoted/spliced live
    // code, and `',cl-user:quoted` is a quote wrapping an unquote — the
    // idiom for splicing a computed value as a literal into generated code
    // — which is live too (the quote itself does not block traversal once
    // already inside the quasiquote; only the nested unquote's own
    // reference matters).
    assert_eq!(
        report
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind == DependencyKind::QualifiedSymbol)
            .count(),
        3
    );
}

#[test]
fn excludes_labels_binding_names_from_recursive_definition_and_body_dependencies() {
    let report = report(
        r#"(defun caller ()
  (cl:labels ((cl-user:helper (x)
                (cl-user:helper x)
                (uiop:ensure-pathname x)))
    (cl-user:helper value)))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "cl"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "uiop"
    ));
    assert_eq!(
        report
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.kind == DependencyKind::QualifiedSymbol && dependency.target == "cl-user"
            })
            .count(),
        0
    );
}

#[test]
fn keeps_flet_definition_body_dependencies_outside_local_callable_scope() {
    let report = report(
        r#"(defun caller ()
  (cl:flet ((cl-user:helper (x)
              (cl-user:helper x)
              (uiop:ensure-pathname x)))
    (cl-user:helper value)))"#,
    );

    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "cl"
    ));
    assert!(contains_dependency(
        &report,
        DependencyKind::QualifiedSymbol,
        "uiop"
    ));
    assert_eq!(
        report
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.kind == DependencyKind::QualifiedSymbol && dependency.target == "cl-user"
            })
            .count(),
        1
    );
}

proptest! {
    #[test]
    fn pbt_collects_package_prefix_from_qualified_symbols(
        package in "[a-z][a-z0-9-]{0,12}",
        symbol in "[a-z][a-z0-9-]{0,12}",
        internal in any::<bool>(),
    ) {
        let separator = if internal { "::" } else { ":" };
        let input = format!("({package}{separator}{symbol} 1)");
        let report = report(&input);
        let matches = report
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.kind == DependencyKind::QualifiedSymbol
                    && dependency.target == package
                    && dependency.source.as_deref() == Some(format!("{package}{separator}{symbol}").as_str())
            })
            .count();

        prop_assert_eq!(matches, 1);
    }
}
