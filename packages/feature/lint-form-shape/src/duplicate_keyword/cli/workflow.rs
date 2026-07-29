use paredit_core_cli::CommandResult;

use crate::duplicate_keyword::cli::args::DuplicateKeywordReportArgs;
use crate::duplicate_keyword::cli::render::print_duplicate_keyword_report;
use crate::duplicate_keyword::usecase::{
    DuplicateKeywordPolicyOptions, collect_duplicate_keywords, evaluate_duplicate_keyword_policy,
    summarize_duplicate_keywords,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn duplicate_keyword_report(args: DuplicateKeywordReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_form_count, file_violations) =
            collect_duplicate_keywords(file, dialect, &tree)?;
        call_form_count += file_call_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_duplicate_keywords(call_form_count, violations);
    let policy = evaluate_duplicate_keyword_policy(
        DuplicateKeywordPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_keyword_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-keyword-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
