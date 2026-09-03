use paredit_core_cli::CommandResult;

use crate::duplicate_slot_report::cli::args::DuplicateSlotReportArgs;
use crate::duplicate_slot_report::cli::render::print_duplicate_slot_report;
use crate::duplicate_slot_report::usecase::{
    DuplicateSlotPolicyOptions, collect_duplicate_slots, evaluate_duplicate_slot_policy,
    summarize_duplicate_slots,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_slot_report(args: DuplicateSlotReportArgs) -> CommandResult {
    let mut definition_count = 0;
    let mut duplicates = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_definition_count, file_duplicates) =
            collect_duplicate_slots(file, dialect, &tree)?;
        definition_count += file_definition_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_slots(definition_count, duplicates);
    let policy = evaluate_duplicate_slot_policy(
        DuplicateSlotPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_slot_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-slot-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
