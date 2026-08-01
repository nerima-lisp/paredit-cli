use paredit_core_cli::CommandResult;

use crate::giant_conditional_form::cli::args::GiantConditionalFormReportArgs;
use crate::giant_conditional_form::cli::render::print_giant_conditional_form_report;
use crate::giant_conditional_form::usecase::{
    GiantConditionalFormPolicyOptions, collect_giant_conditional_form,
    evaluate_giant_conditional_form_policy, summarize_giant_conditional_form,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn giant_conditional_form_report(args: GiantConditionalFormReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut scanned_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_giant_conditional_form(file, dialect, &tree)?;
        scanned_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_giant_conditional_form(scanned_form_count, violations);
    let policy = evaluate_giant_conditional_form_policy(
        GiantConditionalFormPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_giant_conditional_form_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "giant-conditional-form-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
