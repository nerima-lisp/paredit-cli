use super::*;

#[test]
fn ignores_shadowed_local_references_when_detecting_unused_definitions() {
    let text = "(in-package #:app)\n(defun shadowed-helper ()\n  (let ((shadowed-helper 1))\n    shadowed-helper))\n";
    let form = "(defun shadowed-helper ()\n  (let ((shadowed-helper 1))\n    shadowed-helper))";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "shadowed-helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(form));
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn ignores_macrolet_body_references_shadowed_by_local_macro_bindings() {
    let text = "(in-package #:app)\n(defun shadowed-helper () 1)\n(defun caller ()\n  (cl:macrolet ((cl-user:shadowed-helper (value) value))\n    (shadowed-helper 1)))\n";
    let form = "(defun shadowed-helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "shadowed-helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(form));
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn keeps_definitions_referenced_from_compiler_macrolet_expander_bodies() {
    let text = "(in-package #:app)\n(defun helper () 1)\n(defun caller ()\n  (cl-user:compiler-macrolet ((cl:expand (value) (list helper value)))\n    (expand 1)))\n";
    let form = "(defun helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 0);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 0);
    assert_eq!(plan.files[0].rewritten, text);
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn keeps_definitions_referenced_from_package_qualified_macrolet_expander_bodies() {
    let text = "(in-package #:app)\n(defun helper () 1)\n(defun caller ()\n  (cl:macrolet ((cl-user:expand (value) (list helper value)))\n    (expand 1)))\n";
    let form = "(defun helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 0);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 0);
    assert_eq!(plan.files[0].rewritten, text);
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn ignores_symbol_macrolet_body_references_shadowed_by_local_symbol_bindings() {
    let text = "(in-package #:app)\n(defun helper () 1)\n(defun caller ()\n  (cl:symbol-macrolet ((cl-user:helper 42))\n    helper))\n";
    let form = "(defun helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(form));
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn keeps_definitions_referenced_from_symbol_macrolet_expansions() {
    let text = "(in-package #:app)\n(defun helper () 1)\n(defun caller ()\n  (symbol-macrolet ((alias helper))\n    alias))\n";
    let form = "(defun helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 0);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 0);
    assert_eq!(plan.files[0].rewritten, text);
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn keeps_definitions_referenced_from_package_qualified_symbol_macrolet_expansions() {
    let text = "(in-package #:app)\n(defun helper () 1)\n(defun caller ()\n  (cl-user:symbol-macrolet ((alias helper))\n    alias))\n";
    let form = "(defun helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 0);
    assert_eq!(plan.removal_count, 0);
    assert_eq!(plan.skipped_count, 0);
    assert_eq!(plan.files[0].rewritten, text);
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn ignores_callable_designators_shadowed_by_local_flet_bindings() {
    let text = "(in-package #:app)\n(defun shadowed-helper () 1)\n(defun caller ()\n  (flet ((shadowed-helper (value) value))\n    #'shadowed-helper))\n";
    let form = "(defun shadowed-helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "shadowed-helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(form));
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn ignores_callable_designators_shadowed_by_local_labels_bindings() {
    let text = "(in-package #:app)\n(defun shadowed-helper () 1)\n(defun caller ()\n  (labels ((shadowed-helper () #'shadowed-helper))\n    (shadowed-helper)))\n";
    let form = "(defun shadowed-helper () 1)";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "shadowed-helper",
            DefinitionCategory::Function,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(form));
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}

#[test]
fn ignores_macro_callable_accessors_shadowed_by_local_macrolet_bindings() {
    let text = "(in-package #:app)\n(defmacro shadowed-helper (&rest body) `(progn ,@body))\n(defun caller ()\n  (macrolet ((shadowed-helper (&rest body) `(progn ,@body)))\n    (macro-function 'shadowed-helper)))\n";
    let form = "(defmacro shadowed-helper (&rest body) `(progn ,@body))";
    let plan = plan_remove_unused_definitions(request_for(
        text,
        vec![definition(
            text,
            form,
            "shadowed-helper",
            DefinitionCategory::Macro,
        )],
    ))
    .expect("plan should build");

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(plan.removal_count, 1);
    assert_eq!(plan.skipped_count, 0);
    assert!(!plan.files[0].rewritten.contains(form));
    SyntaxTree::parse(&plan.files[0].rewritten).expect("rewrite must stay parseable");
}
