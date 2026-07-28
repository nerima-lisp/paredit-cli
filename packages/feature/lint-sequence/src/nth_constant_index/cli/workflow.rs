use anyhow::Result;

use crate::nth_constant_index::cli::args::NthConstantIndexReportArgs;
use crate::nth_constant_index::cli::render::print_nth_constant_index_report;
use crate::nth_constant_index::usecase::{
    NthConstantIndexPolicyOptions, collect_nth_constant_indexes,
    evaluate_nth_constant_index_policy, summarize_nth_constant_indexes,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nth_constant_index_report(args: NthConstantIndexReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut nth_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_nth_constant_indexes(file, dialect, &tree)?;
        nth_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nth_constant_indexes(nth_form_count, violations);
    let policy = evaluate_nth_constant_index_policy(
        NthConstantIndexPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nth_constant_index_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nth-constant-index-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
