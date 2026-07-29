use paredit_core_cli::CommandResult;

use crate::coerce_to_t::cli::args::CoerceToTReportArgs;
use crate::coerce_to_t::cli::render::print_coerce_to_t_report;
use crate::coerce_to_t::usecase::{
    CoerceToTPolicyOptions, collect_coerce_to_ts, evaluate_coerce_to_t_policy,
    summarize_coerce_to_ts,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn coerce_to_t_report(args: CoerceToTReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut coerce_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_coerce_to_ts(file, dialect, &tree)?;
        coerce_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_coerce_to_ts(coerce_form_count, violations);
    let policy = evaluate_coerce_to_t_policy(
        CoerceToTPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_coerce_to_t_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "coerce-to-t-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
