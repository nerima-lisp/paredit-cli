use anyhow::Result;

use crate::application::usecase::if_arity_report::{
    IfArityPolicyOptions, collect_if_arity_violations, evaluate_if_arity_policy, summarize_if_arity,
};
use crate::presentation::cli::if_arity_report::args::IfArityReportArgs;
use crate::presentation::cli::if_arity_report::render::print_if_arity_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn if_arity_report(args: IfArityReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut if_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_if_form_count, file_violations) =
            collect_if_arity_violations(file, dialect, &tree)?;
        if_form_count += file_if_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_if_arity(if_form_count, violations);
    let policy =
        evaluate_if_arity_policy(IfArityPolicyOptions::new(args.fail_on_violation), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_if_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "if-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
