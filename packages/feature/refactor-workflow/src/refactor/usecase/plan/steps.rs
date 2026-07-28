use std::path::PathBuf;

use super::types::RefactorPlanStep;
use paredit_core_edit::refactor_plan::{
    RefactorOperation, RefactorPlanGate, RefactorPlanTargetKind,
};

#[must_use]
pub fn refactor_plan_steps(
    operation: RefactorOperation,
    symbol: &str,
    files: &[PathBuf],
    target_kind: RefactorPlanTargetKind,
    gates: &[RefactorPlanGate],
) -> Vec<RefactorPlanStep> {
    let file_args = shell_file_args(files);
    let symbol_arg = shell_quote(symbol);
    let impact_command =
        gated_impact_report_command(operation, target_kind, &symbol_arg, &file_args);
    let verification_command =
        verification_command(operation, &symbol_arg, &file_args, &impact_command);
    let mut steps = vec![
        RefactorPlanStep {
            order: 1,
            action: "run-impact-report",
            rationale: "Validate definitions, references, call graph edges, and signature compatibility before editing."
                .to_owned(),
            command: Some(impact_command.clone()),
        },
        RefactorPlanStep {
            order: 2,
            action: "run-dependency-report",
            rationale: "Check package, ASDF, load, and qualified-symbol dependencies that can invalidate a cross-file refactor."
                .to_owned(),
            command: Some(format!("paredit inspect dependencies --output json {file_args}")),
        },
    ];

    let blocked = gates.iter().any(|gate| gate.blocks_automation);
    let has_non_call_references = gates.iter().any(|gate| gate.code == "non-call-references");
    let (action, rationale, command) = match operation {
        RefactorOperation::Rename if blocked => (
            "review-rename-scope",
            "Blocking gates were found; inspect exact references before choosing function-only or atom-wide rename.",
            None,
        ),
        RefactorOperation::Rename if target_kind == RefactorPlanTargetKind::SymbolMacro => (
            "apply-symbol-macro-rename",
            "Target definition is a symbol macro; use the dedicated symbol-macro rename workflow to preserve macro expansion and value references.",
            Some(format!(
                "paredit refactor rename-symbol-macro --from {symbol_arg} --to <new-symbol> --output json {file_args}"
            )),
        ),
        RefactorOperation::Rename if target_kind.is_macro_like() => (
            "apply-macro-rename",
            "Target definition is macro-like; use the callable rename workflow because it already rewrites the definition and invocation sites.",
            Some(format!(
                "paredit refactor rename-function --from {symbol_arg} --to <new-symbol> --output json {file_args}"
            )),
        ),
        RefactorOperation::Rename if has_non_call_references => (
            "apply-symbol-rename",
            "Non-call references exist; use atom-wide rename after reviewing the exact impact scope.",
            Some(format!(
                "paredit refactor rename-symbols --from {symbol_arg} --to <new-symbol> --output json {file_args}"
            )),
        ),
        RefactorOperation::Rename => (
            "apply-rename",
            "No blocking rename gates were found; use callable rename when every reference is a call/definition.",
            Some(format!(
                "paredit refactor rename-function --from {symbol_arg} --to <new-symbol> --output json {file_args}"
            )),
        ),
        RefactorOperation::Remove if blocked => (
            "review-remove-scope",
            "Removal has live references or ambiguous definitions; update callers before deleting the definition.",
            None,
        ),
        RefactorOperation::Remove => (
            "apply-unused-definition-removal",
            "No blocking removal gates were found; remove unused definition candidates across the reviewed file set with a dry-run plan first.",
            Some(format!("paredit refactor remove-unused-definitions --output json {file_args}")),
        ),
        RefactorOperation::Move if blocked => (
            "review-move-scope",
            "Move has callers or ambiguity; inspect exports, packages, and load order before moving forms.",
            None,
        ),
        RefactorOperation::Move => (
            "apply-move",
            "No blocking move gates were found; move the selected top-level definition with a dry-run plan first.",
            Some("paredit refactor move-definition --from-file <file> --to-file <file> --path <definition-path> --plan --output json".to_owned()),
        ),
        RefactorOperation::Signature if target_kind.skips_signature_compatibility() => (
            "review-signature-scope",
            "The target kind does not expose callable signatures; review the expansion or binding semantics before editing parameters.",
            None,
        ),
        RefactorOperation::Signature if blocked => (
            "review-signature-scope",
            "Signature changes require manual review when call arity, non-call references, or callers are unsafe.",
            None,
        ),
        RefactorOperation::Signature => (
            "apply-signature-change",
            "No blocking signature gates were found; use the dedicated parameter-edit commands with plans first.",
            Some("paredit refactor add-function-parameter --file <file> --path <definition-path> --name <parameter> --plan --output json".to_owned()),
        ),
    };

    steps.push(RefactorPlanStep {
        order: 3,
        action,
        rationale: rationale.to_owned(),
        command,
    });
    steps.push(RefactorPlanStep {
        order: 4,
        action: "verify-after-edit",
        rationale: "Re-run impact, dependency, signature, and workspace checks after the edit to detect regressions."
            .to_owned(),
        command: Some(verification_command),
    });

    steps
}

fn verification_command(
    operation: RefactorOperation,
    symbol_arg: &str,
    file_args: &str,
    impact_command: &str,
) -> String {
    match operation {
        RefactorOperation::Remove => format!(
            "paredit refactor verify --symbol {symbol_arg} --operation remove --phase post --output json {file_args} && paredit inspect dependencies --output json {file_args}"
        ),
        RefactorOperation::Rename | RefactorOperation::Move | RefactorOperation::Signature => {
            format!("{impact_command} && paredit inspect dependencies --output json {file_args}")
        }
    }
}

fn gated_impact_report_command(
    operation: RefactorOperation,
    target_kind: RefactorPlanTargetKind,
    symbol_arg: &str,
    file_args: &str,
) -> String {
    let require_calls = if target_kind.requires_call_coverage(operation) {
        " --require-calls 1"
    } else {
        ""
    };

    format!(
        "paredit inspect impact --symbol {symbol_arg} --fail-on-risk-level warning --require-definitions 1 --require-references 1{require_calls} --output json {file_args}"
    )
}

fn shell_file_args(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|file| shell_quote(&file.display().to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
