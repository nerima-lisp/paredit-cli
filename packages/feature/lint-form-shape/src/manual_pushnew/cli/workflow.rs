use anyhow::Result;

use crate::manual_pushnew::cli::args::ManualPushnewReportArgs;
use crate::manual_pushnew::cli::render::print_manual_pushnew_report;
use crate::manual_pushnew::usecase::{
    ManualPushnewPolicyOptions, collect_manual_pushnews, evaluate_manual_pushnew_policy,
    summarize_manual_pushnews,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn manual_pushnew_report(args: ManualPushnewReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_manual_pushnews(file, dialect, &tree)?;
        assignment_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_manual_pushnews(assignment_form_count, violations);
    let policy = evaluate_manual_pushnew_policy(
        ManualPushnewPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_manual_pushnew_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "manual-pushnew-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
