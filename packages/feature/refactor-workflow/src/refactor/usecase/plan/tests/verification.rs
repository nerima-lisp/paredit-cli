use super::fixtures::summary;
use super::*;

#[test]
fn post_rename_verification_requires_old_symbol_removed_and_new_symbol_present() {
    let before = RefactorPlanSummary {
        definition_count: 0,
        reference_count: 0,
        ..summary()
    };
    let after = RefactorPlanSummary {
        definition_count: 1,
        reference_count: 2,
        signature_mismatch_count: 0,
        ..summary()
    };
    let checks = refactor_verification_checks(
        RefactorVerificationRequest {
            operation: RefactorOperation::Rename,
            phase: VerificationPhase::Post,
            symbol: "old-name",
            new_symbol: Some("new-name"),
            target_kind: RefactorPlanTargetKind::Callable,
            before,
            after: Some(after),
        },
        &[],
    );

    assert!(checks.iter().all(|check| check.passed));
    assert!(
        checks
            .iter()
            .any(|check| check.code == "old-symbol-removed")
    );
    assert!(
        checks
            .iter()
            .any(|check| check.code == "new-symbol-present")
    );
    assert!(
        checks
            .iter()
            .any(|check| check.code == "new-symbol-signature-compatible")
    );
}

#[test]
fn post_rename_verification_skips_signature_check_for_macros() {
    let before = RefactorPlanSummary {
        definition_count: 0,
        reference_count: 0,
        ..summary()
    };
    let after = RefactorPlanSummary {
        definition_count: 1,
        reference_count: 2,
        signature_mismatch_count: 4,
        ..summary()
    };
    let checks = refactor_verification_checks(
        RefactorVerificationRequest {
            operation: RefactorOperation::Rename,
            phase: VerificationPhase::Post,
            symbol: "old-name",
            new_symbol: Some("new-name"),
            target_kind: RefactorPlanTargetKind::Macro,
            before,
            after: Some(after),
        },
        &[],
    );

    assert!(checks.iter().all(|check| check.passed));
    assert!(
        !checks
            .iter()
            .any(|check| check.code == "new-symbol-signature-compatible")
    );
}

#[test]
fn post_rename_verification_skips_signature_check_for_symbol_macros() {
    let before = RefactorPlanSummary {
        definition_count: 0,
        reference_count: 0,
        ..summary()
    };
    let after = RefactorPlanSummary {
        definition_count: 1,
        reference_count: 2,
        signature_mismatch_count: 4,
        ..summary()
    };
    let checks = refactor_verification_checks(
        RefactorVerificationRequest {
            operation: RefactorOperation::Rename,
            phase: VerificationPhase::Post,
            symbol: "old-name",
            new_symbol: Some("new-name"),
            target_kind: RefactorPlanTargetKind::SymbolMacro,
            before,
            after: Some(after),
        },
        &[],
    );

    assert!(checks.iter().all(|check| check.passed));
    assert!(
        !checks
            .iter()
            .any(|check| check.code == "new-symbol-signature-compatible")
    );
}

#[test]
fn post_rename_verification_skips_signature_check_for_macro_like_targets() {
    let before = RefactorPlanSummary {
        definition_count: 0,
        reference_count: 0,
        ..summary()
    };
    let after = RefactorPlanSummary {
        definition_count: 1,
        reference_count: 2,
        signature_mismatch_count: 4,
        ..summary()
    };

    for target_kind in [
        RefactorPlanTargetKind::CompilerMacro,
        RefactorPlanTargetKind::SetfExpander,
    ] {
        let checks = refactor_verification_checks(
            RefactorVerificationRequest {
                operation: RefactorOperation::Rename,
                phase: VerificationPhase::Post,
                symbol: "old-name",
                new_symbol: Some("new-name"),
                target_kind,
                before,
                after: Some(after),
            },
            &[],
        );

        assert!(checks.iter().all(|check| check.passed));
        assert!(
            !checks
                .iter()
                .any(|check| check.code == "new-symbol-signature-compatible")
        );
    }
}

#[test]
fn post_move_verification_requires_after_state_to_contain_the_symbol() {
    let before = RefactorPlanSummary {
        definition_count: 5,
        reference_count: 0,
        ..summary()
    };
    let after = RefactorPlanSummary {
        definition_count: 0,
        reference_count: 3,
        ..summary()
    };
    let checks = refactor_verification_checks(
        RefactorVerificationRequest {
            operation: RefactorOperation::Move,
            phase: VerificationPhase::Post,
            symbol: "moved-name",
            new_symbol: None,
            target_kind: RefactorPlanTargetKind::Callable,
            before,
            after: Some(after),
        },
        &[],
    );

    let moved_symbol_present = checks
        .iter()
        .find(|check| check.code == "moved-symbol-present")
        .expect("expected move post-check to be present");

    assert!(!moved_symbol_present.passed);
    assert_eq!(moved_symbol_present.count, 0);
}

#[test]
fn post_move_verification_passes_when_after_state_keeps_the_symbol() {
    let before = RefactorPlanSummary {
        definition_count: 0,
        reference_count: 0,
        ..summary()
    };
    let after = RefactorPlanSummary {
        definition_count: 2,
        reference_count: 1,
        ..summary()
    };
    let checks = refactor_verification_checks(
        RefactorVerificationRequest {
            operation: RefactorOperation::Move,
            phase: VerificationPhase::Post,
            symbol: "moved-name",
            new_symbol: None,
            target_kind: RefactorPlanTargetKind::Callable,
            before,
            after: Some(after),
        },
        &[],
    );

    let moved_symbol_present = checks
        .iter()
        .find(|check| check.code == "moved-symbol-present")
        .expect("expected move post-check to be present");

    assert!(moved_symbol_present.passed);
    assert_eq!(moved_symbol_present.count, 2);
}

#[test]
fn post_signature_verification_skips_signature_check_for_macro_like_targets() {
    let before = RefactorPlanSummary {
        definition_count: 1,
        reference_count: 2,
        signature_mismatch_count: 4,
        ..summary()
    };

    for target_kind in [
        RefactorPlanTargetKind::Macro,
        RefactorPlanTargetKind::CompilerMacro,
        RefactorPlanTargetKind::SetfExpander,
        RefactorPlanTargetKind::SymbolMacro,
    ] {
        let checks = refactor_verification_checks(
            RefactorVerificationRequest {
                operation: RefactorOperation::Signature,
                phase: VerificationPhase::Post,
                symbol: "current-user",
                new_symbol: None,
                target_kind,
                before,
                after: None,
            },
            &[],
        );

        assert!(checks.is_empty());
    }
}
