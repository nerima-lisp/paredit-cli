use anyhow::Result;

use crate::application::usecase::sharp_quoted_lambda_report::{
    SharpQuotedLambdaPolicyOptions, collect_sharp_quoted_lambdas,
    evaluate_sharp_quoted_lambda_policy, summarize_sharp_quoted_lambdas,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::sharp_quoted_lambda_report::args::SharpQuotedLambdaReportArgs;
use crate::presentation::cli::sharp_quoted_lambda_report::render::print_sharp_quoted_lambda_report;

pub(in crate::presentation::cli) fn sharp_quoted_lambda_report(
    args: SharpQuotedLambdaReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut lambda_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_sharp_quoted_lambdas(file, dialect, &tree)?;
        lambda_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_sharp_quoted_lambdas(lambda_form_count, violations);
    let policy = evaluate_sharp_quoted_lambda_policy(
        SharpQuotedLambdaPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_sharp_quoted_lambda_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "sharp-quoted-lambda-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
