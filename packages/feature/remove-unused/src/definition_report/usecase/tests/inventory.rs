use super::*;

#[test]
fn builds_definition_inventory_with_package_and_counts() {
    let input = "(in-package #:demo)\n\
             (defun render-pane (session pane) (list session pane))\n\
             (defmacro with-pane ((pane) &body body) `(progn ,pane ,@body))\n\
             (defclass renderer () ())\n\
             (defstruct point x y)\n\
             (define-symbol-macro current-user (slot-value *session* 'user))\n";
    let tree = SyntaxTree::parse(input).expect("parse input");

    let report = build_definition_report(PathBuf::from("core.lisp"), Dialect::CommonLisp, &tree)
        .expect("build report");

    assert_eq!(report.package.as_deref(), Some("#:demo"));
    assert_eq!(report.definitions.len(), 5);
    assert_eq!(report.definitions[0].name.as_deref(), Some("render-pane"));
    assert_eq!(report.definitions[0].parameter_count, Some(2));
    assert_eq!(report.definitions[0].body_form_count, Some(1));
    assert_eq!(report.definitions[0].package.as_deref(), Some("#:demo"));
    assert_eq!(report.definitions[1].name.as_deref(), Some("with-pane"));
    assert_eq!(report.definitions[1].parameter_count, Some(2));
    assert_eq!(report.definitions[2].name.as_deref(), Some("renderer"));
    assert_eq!(report.definitions[2].category, DefinitionCategory::Class);
    assert_eq!(report.definitions[2].parameter_count, None);
    assert_eq!(report.definitions[2].body_form_count, Some(2));
    assert_eq!(report.definitions[2].package.as_deref(), Some("#:demo"));
    assert_eq!(report.definitions[3].name.as_deref(), Some("point"));
    assert_eq!(report.definitions[3].category, DefinitionCategory::Struct);
    assert_eq!(report.definitions[3].parameter_count, None);
    assert_eq!(report.definitions[3].body_form_count, Some(2));
    assert_eq!(report.definitions[3].package.as_deref(), Some("#:demo"));
    assert_eq!(report.definitions[4].name.as_deref(), Some("current-user"));
    assert_eq!(report.definitions[4].category, DefinitionCategory::Variable);
    assert_eq!(report.definitions[4].parameter_count, None);
    assert_eq!(report.definitions[4].body_form_count, Some(1));
    assert_eq!(report.definitions[4].package.as_deref(), Some("#:demo"));
}

#[test]
fn builds_definition_inventory_for_defstruct_with_options_list_name() {
    let input = "(defstruct (line (:constructor make-line)) start end)\n";
    let tree = SyntaxTree::parse(input).expect("parse input");

    let report = build_definition_report(PathBuf::from("core.lisp"), Dialect::CommonLisp, &tree)
        .expect("build report");

    assert_eq!(report.definitions.len(), 1);
    assert_eq!(report.definitions[0].name.as_deref(), Some("line"));
    assert_eq!(report.definitions[0].category, DefinitionCategory::Struct);
}
