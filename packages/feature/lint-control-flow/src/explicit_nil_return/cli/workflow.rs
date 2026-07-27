use anyhow::Result;

use crate::application::usecase::explicit_nil_return_report::{
    ExplicitNilReturnPolicyOptions, collect_explicit_nil_returns,
    evaluate_explicit_nil_return_policy, summarize_explicit_nil_returns,
};
use crate::presentation::cli::explicit_nil_return_report::args::ExplicitNilReturnReportArgs;
use crate::presentation::cli::explicit_nil_return_report::render::print_explicit_nil_return_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn explicit_nil_return_report(
    args: ExplicitNilReturnReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut return_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_explicit_nil_returns(file, dialect, &tree)?;
        return_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_explicit_nil_returns(return_form_count, violations);
    let policy = evaluate_explicit_nil_return_policy(
        ExplicitNilReturnPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_explicit_nil_return_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "explicit-nil-return-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
