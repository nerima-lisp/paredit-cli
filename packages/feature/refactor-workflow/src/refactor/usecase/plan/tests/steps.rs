use super::*;
use std::path::PathBuf;

#[test]
fn remove_plan_uses_unused_definition_cleanup_usecase() {
    let files = vec![PathBuf::from("src/core.lisp"), PathBuf::from("src/ui.el")];
    let steps = refactor_plan_steps(
        RefactorOperation::Remove,
        "stale-helper",
        &files,
        RefactorPlanTargetKind::Unknown,
        &[],
    );

    let apply = steps
        .iter()
        .find(|step| step.order == 3)
        .expect("apply step");
    assert_eq!(apply.action, "apply-unused-definition-removal");
    let apply_command = apply.command.as_deref().expect("apply command");
    assert!(apply_command.contains("paredit refactor remove-unused-definitions --output json"));
    assert!(apply_command.contains("'src/core.lisp'"));
    assert!(apply_command.contains("'src/ui.el'"));

    let verify = steps
        .iter()
        .find(|step| step.order == 4)
        .expect("verify step");
    let verify_command = verify.command.as_deref().expect("verify command");
    assert!(verify_command.contains(
        "paredit refactor verify --symbol 'stale-helper' --operation remove --phase post --output json"
    ));
    assert!(!verify_command.contains("--require-definitions 1"));
    assert!(!verify_command.contains("--require-references 1"));
}

#[test]
fn rename_plan_uses_symbol_macro_rename_usecase_for_symbol_macros() {
    let files = vec![PathBuf::from("src/core.lisp")];
    let steps = refactor_plan_steps(
        RefactorOperation::Rename,
        "current-user",
        &files,
        RefactorPlanTargetKind::SymbolMacro,
        &[],
    );

    let apply = steps
        .iter()
        .find(|step| step.order == 3)
        .expect("apply step");
    assert_eq!(apply.action, "apply-symbol-macro-rename");
    let apply_command = apply.command.as_deref().expect("apply command");
    assert!(apply_command.contains(
        "paredit refactor rename-symbol-macro --from 'current-user' --to <new-symbol> --output json"
    ));
    assert!(apply_command.contains("'src/core.lisp'"));

    let verify = steps
        .iter()
        .find(|step| step.order == 4)
        .expect("verify step");
    let verify_command = verify.command.as_deref().expect("verify command");
    assert!(verify_command.contains(
        "paredit inspect impact --symbol 'current-user' --fail-on-risk-level warning --require-definitions 1 --require-references 1 --output json"
    ));
    assert!(!verify_command.contains("--require-calls 1"));
    assert!(verify_command.contains("paredit inspect dependencies --output json"));
}

#[test]
fn rename_plan_uses_callable_rename_workflow_for_macros() {
    let files = vec![PathBuf::from("src/core.lisp")];
    let steps = refactor_plan_steps(
        RefactorOperation::Rename,
        "render-pane",
        &files,
        RefactorPlanTargetKind::Macro,
        &[],
    );

    let apply = steps
        .iter()
        .find(|step| step.order == 3)
        .expect("apply step");
    assert_eq!(apply.action, "apply-macro-rename");
    let apply_command = apply.command.as_deref().expect("apply command");
    assert!(apply_command.contains(
        "paredit refactor rename-function --from 'render-pane' --to <new-symbol> --output json"
    ));
    assert!(apply_command.contains("'src/core.lisp'"));

    let verify = steps
        .iter()
        .find(|step| step.order == 4)
        .expect("verify step");
    let verify_command = verify.command.as_deref().expect("verify command");
    assert!(verify_command.contains(
        "paredit inspect impact --symbol 'render-pane' --fail-on-risk-level warning --require-definitions 1 --require-references 1 --require-calls 1 --output json"
    ));
    assert!(verify_command.contains("paredit inspect dependencies --output json"));
}

#[test]
fn rename_plan_uses_callable_rename_workflow_for_macro_like_targets() {
    let files = vec![PathBuf::from("src/core.lisp")];

    for target_kind in [
        RefactorPlanTargetKind::CompilerMacro,
        RefactorPlanTargetKind::SetfExpander,
    ] {
        let steps = refactor_plan_steps(
            RefactorOperation::Rename,
            "render-pane",
            &files,
            target_kind,
            &[],
        );

        let apply = steps
            .iter()
            .find(|step| step.order == 3)
            .expect("apply step");
        assert_eq!(apply.action, "apply-macro-rename");
        let apply_command = apply.command.as_deref().expect("apply command");
        assert!(apply_command.contains(
            "paredit refactor rename-function --from 'render-pane' --to <new-symbol> --output json"
        ));
        assert!(apply_command.contains("'src/core.lisp'"));
    }
}

#[test]
fn move_plan_uses_definition_move_workflow_for_symbol_macros_without_call_coverage() {
    let files = vec![PathBuf::from("src/core.lisp")];
    let steps = refactor_plan_steps(
        RefactorOperation::Move,
        "current-user",
        &files,
        RefactorPlanTargetKind::SymbolMacro,
        &[],
    );

    let apply = steps
        .iter()
        .find(|step| step.order == 3)
        .expect("apply step");
    assert_eq!(apply.action, "apply-move");
    let apply_command = apply.command.as_deref().expect("apply command");
    assert!(apply_command.contains("paredit refactor move-definition --from-file <file> --to-file <file> --path <definition-path> --plan --output json"));

    let verify = steps
        .iter()
        .find(|step| step.order == 4)
        .expect("verify step");
    let verify_command = verify.command.as_deref().expect("verify command");
    assert!(verify_command.contains(
        "paredit inspect impact --symbol 'current-user' --fail-on-risk-level warning --require-definitions 1 --require-references 1 --output json"
    ));
    assert!(!verify_command.contains("--require-calls 1"));
    assert!(verify_command.contains("paredit inspect dependencies --output json"));
}

#[test]
fn signature_plan_uses_manual_review_for_macro_like_targets() {
    let files = vec![PathBuf::from("src/core.lisp")];
    for target_kind in [
        RefactorPlanTargetKind::Macro,
        RefactorPlanTargetKind::CompilerMacro,
        RefactorPlanTargetKind::SetfExpander,
        RefactorPlanTargetKind::SymbolMacro,
    ] {
        let steps = refactor_plan_steps(
            RefactorOperation::Signature,
            "current-user",
            &files,
            target_kind,
            &[],
        );

        let apply = steps
            .iter()
            .find(|step| step.order == 3)
            .expect("apply step");
        assert_eq!(apply.action, "review-signature-scope");
        assert!(apply.command.is_none());

        let verify = steps
            .iter()
            .find(|step| step.order == 4)
            .expect("verify step");
        let verify_command = verify.command.as_deref().expect("verify command");
        assert!(verify_command.contains(
            "paredit inspect impact --symbol 'current-user' --fail-on-risk-level warning --require-definitions 1 --require-references 1 --output json"
        ));
        assert!(!verify_command.contains("--require-calls 1"));
    }
}
