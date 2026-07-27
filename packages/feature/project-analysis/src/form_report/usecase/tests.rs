use crate::application::usecase::form_report::types::{FormKind, FormReport, FormReportRequest};
use crate::application::usecase::form_report::workflow::build_form_report;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SyntaxTree};

fn report_for(input: &str, path: &str, dialect: Dialect) -> FormReport {
    let tree = SyntaxTree::parse(input).expect("valid input");
    let path = path.parse::<Path>().expect("valid path");
    let selection = tree.select_path(&path).expect("selection");
    build_form_report(FormReportRequest {
        input,
        dialect,
        path: Some(path),
        target: selection.view(),
        include_source: true,
    })
    .expect("report")
}

#[test]
fn reports_definition_like_common_lisp_form() {
    let report = report_for("(defun add (x y) (+ x y))", "0", Dialect::CommonLisp);

    assert_eq!(report.kind, FormKind::List);
    assert_eq!(report.head.as_deref(), Some("defun"));
    assert!(report.definition_like);
    assert_eq!(report.child_count, 4);
    assert_eq!(report.list_count, 3);
    assert_eq!(report.source.as_deref(), Some("(defun add (x y) (+ x y))"));
    assert!(report.symbols.iter().any(|symbol| symbol.symbol == "x"));
}

#[test]
fn reports_atom_target_without_head() {
    let report = report_for("(message \"foo\" bar)", "0.2", Dialect::EmacsLisp);

    assert_eq!(report.kind, FormKind::Atom);
    assert_eq!(report.head, None);
    assert!(!report.definition_like);
    assert_eq!(report.atom_count, 1);
    assert_eq!(report.list_count, 0);
    assert_eq!(report.symbols[0].symbol, "bar");
}
