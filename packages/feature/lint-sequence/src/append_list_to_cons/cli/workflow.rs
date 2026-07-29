use paredit_core_cli::CommandResult;

use crate::append_list_to_cons::cli::args::AppendListToConsReportArgs;
use crate::append_list_to_cons::cli::render::print_append_list_to_cons_report;
use crate::append_list_to_cons::usecase::{
    AppendListToConsPolicyOptions, collect_append_list_to_cons,
    evaluate_append_list_to_cons_policy, summarize_append_list_to_cons,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn append_list_to_cons_report(args: AppendListToConsReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut append_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_append_list_to_cons(file, dialect, &tree)?;
        append_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_append_list_to_cons(append_form_count, violations);
    let policy = evaluate_append_list_to_cons_policy(
        AppendListToConsPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_append_list_to_cons_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "append-list-to-cons-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
