use anyhow::Result;

use crate::application::usecase::eql_search_literal_report::{
    EqlSearchLiteralPolicyOptions, collect_eql_search_literals, evaluate_eql_search_literal_policy,
    summarize_eql_search_literals,
};
use crate::presentation::cli::eql_search_literal_report::args::EqlSearchLiteralReportArgs;
use crate::presentation::cli::eql_search_literal_report::render::print_eql_search_literal_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn eql_search_literal_report(
    args: EqlSearchLiteralReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut search_call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) = collect_eql_search_literals(file, dialect, &tree)?;
        search_call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_eql_search_literals(search_call_count, violations);
    let policy = evaluate_eql_search_literal_policy(
        EqlSearchLiteralPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_eql_search_literal_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "eql-search-literal-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
