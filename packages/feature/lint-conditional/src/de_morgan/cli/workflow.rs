use anyhow::Result;

use crate::application::usecase::de_morgan_report::{
    DeMorganPolicyOptions, collect_de_morgans, evaluate_de_morgan_policy, summarize_de_morgans,
};
use crate::presentation::cli::de_morgan_report::args::DeMorganReportArgs;
use crate::presentation::cli::de_morgan_report::render::print_de_morgan_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn de_morgan_report(args: DeMorganReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_de_morgans(file, dialect, &tree)?;
        boolean_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_de_morgans(boolean_form_count, violations);
    let policy =
        evaluate_de_morgan_policy(DeMorganPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_de_morgan_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "de-morgan-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
