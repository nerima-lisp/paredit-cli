use paredit_core_cli::CommandResult;

use crate::gethash_default::cli::args::GethashDefaultReportArgs;
use crate::gethash_default::cli::render::print_gethash_default_report;
use crate::gethash_default::usecase::{
    GethashDefaultPolicyOptions, collect_gethash_defaults, evaluate_gethash_default_policy,
    summarize_gethash_defaults,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn gethash_default_report(args: GethashDefaultReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut gethash_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_gethash_defaults(file, dialect, &tree)?;
        gethash_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_gethash_defaults(gethash_form_count, violations);
    let policy = evaluate_gethash_default_policy(
        GethashDefaultPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_gethash_default_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "gethash-default-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
