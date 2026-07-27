use anyhow::Result;

use crate::application::usecase::nested_unless_report::{
    NestedUnlessPolicyOptions, collect_nested_unlesses, evaluate_nested_unless_policy,
    summarize_nested_unlesses,
};
use crate::presentation::cli::nested_unless_report::args::NestedUnlessReportArgs;
use crate::presentation::cli::nested_unless_report::render::print_nested_unless_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn nested_unless_report(
    args: NestedUnlessReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut unless_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_nested_unlesses(file, dialect, &tree)?;
        unless_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nested_unlesses(unless_form_count, violations);
    let policy = evaluate_nested_unless_policy(
        NestedUnlessPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nested_unless_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "nested-unless-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
