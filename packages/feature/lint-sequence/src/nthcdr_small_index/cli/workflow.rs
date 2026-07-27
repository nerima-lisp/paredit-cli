use anyhow::Result;

use crate::nthcdr_small_index::cli::args::NthcdrSmallIndexReportArgs;
use crate::nthcdr_small_index::cli::render::print_nthcdr_small_index_report;
use crate::nthcdr_small_index::usecase::{
    NthcdrSmallIndexPolicyOptions, collect_nthcdr_small_indexes,
    evaluate_nthcdr_small_index_policy, summarize_nthcdr_small_indexes,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn nthcdr_small_index_report(args: NthcdrSmallIndexReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut nthcdr_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_nthcdr_small_indexes(file, dialect, &tree)?;
        nthcdr_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_nthcdr_small_indexes(nthcdr_form_count, violations);
    let policy = evaluate_nthcdr_small_index_policy(
        NthcdrSmallIndexPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_nthcdr_small_index_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "nthcdr-small-index-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
