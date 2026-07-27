use anyhow::Result;

use crate::application::usecase::make_array_default_keyword_report::{
    MakeArrayDefaultKeywordPolicyOptions, collect_make_array_default_keywords,
    evaluate_make_array_default_keyword_policy, summarize_make_array_default_keywords,
};
use crate::presentation::cli::make_array_default_keyword_report::args::MakeArrayDefaultKeywordReportArgs;
use crate::presentation::cli::make_array_default_keyword_report::render::print_make_array_default_keyword_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn make_array_default_keyword_report(
    args: MakeArrayDefaultKeywordReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_make_array_default_keywords(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_make_array_default_keywords(call_form_count, violations);
    let policy = evaluate_make_array_default_keyword_policy(
        MakeArrayDefaultKeywordPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_make_array_default_keyword_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "make-array-default-keyword-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
