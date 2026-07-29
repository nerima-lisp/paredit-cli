use paredit_core_cli::CommandResult;

use crate::quoted_case_key::cli::args::QuotedCaseKeyReportArgs;
use crate::quoted_case_key::cli::render::print_quoted_case_key_report;
use crate::quoted_case_key::usecase::{
    QuotedCaseKeyPolicyOptions, collect_quoted_case_keys, evaluate_quoted_case_key_policy,
    summarize_quoted_case_keys,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn quoted_case_key_report(args: QuotedCaseKeyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut case_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_case_form_count, file_violations) =
            collect_quoted_case_keys(file, dialect, &tree)?;
        case_form_count += file_case_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_quoted_case_keys(case_form_count, violations);
    let policy = evaluate_quoted_case_key_policy(
        QuotedCaseKeyPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_quoted_case_key_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "quoted-case-key-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
