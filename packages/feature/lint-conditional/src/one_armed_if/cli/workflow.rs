use anyhow::Result;

use crate::application::usecase::one_armed_if_report::{
    OneArmedIfPolicyOptions, collect_one_armed_ifs, evaluate_one_armed_if_policy,
    summarize_one_armed_ifs,
};
use crate::presentation::cli::one_armed_if_report::args::OneArmedIfReportArgs;
use crate::presentation::cli::one_armed_if_report::render::print_one_armed_if_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn one_armed_if_report(args: OneArmedIfReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_one_armed_ifs(file, dialect, &tree)?;
        if_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_one_armed_ifs(if_form_count, violations);
    let policy = evaluate_one_armed_if_policy(
        OneArmedIfPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_one_armed_if_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "one-armed-if-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
