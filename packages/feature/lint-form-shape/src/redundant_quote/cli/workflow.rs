use paredit_core_cli::CommandResult;

use crate::redundant_quote::cli::args::RedundantQuoteReportArgs;
use crate::redundant_quote::cli::render::print_redundant_quote_report;
use crate::redundant_quote::usecase::{
    RedundantQuotePolicyOptions, collect_redundant_quotes, evaluate_redundant_quote_policy,
    summarize_redundant_quotes,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn redundant_quote_report(args: RedundantQuoteReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut quoted_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_quoted_form_count, file_violations) =
            collect_redundant_quotes(file, dialect, &tree)?;
        quoted_form_count += file_quoted_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_redundant_quotes(quoted_form_count, violations);
    let policy = evaluate_redundant_quote_policy(
        RedundantQuotePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_redundant_quote_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "redundant-quote-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
