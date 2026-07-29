use paredit_core_cli::CommandResult;

use crate::parse_integer_default_radix::cli::args::ParseIntegerDefaultRadixReportArgs;
use crate::parse_integer_default_radix::cli::render::print_parse_integer_default_radix_report;
use crate::parse_integer_default_radix::usecase::{
    ParseIntegerDefaultRadixPolicyOptions, collect_parse_integer_default_radixes,
    evaluate_parse_integer_default_radix_policy, summarize_parse_integer_default_radixes,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn parse_integer_default_radix_report(
    args: ParseIntegerDefaultRadixReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_parse_integer_default_radixes(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_parse_integer_default_radixes(call_form_count, violations);
    let policy = evaluate_parse_integer_default_radix_policy(
        ParseIntegerDefaultRadixPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_parse_integer_default_radix_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "parse-integer-default-radix-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
