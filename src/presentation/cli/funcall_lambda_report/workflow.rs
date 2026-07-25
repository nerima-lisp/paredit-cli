use anyhow::Result;

use crate::application::usecase::funcall_lambda_report::{
    FuncallLambdaPolicyOptions, collect_funcall_lambdas, evaluate_funcall_lambda_policy,
    summarize_funcall_lambdas,
};
use crate::presentation::cli::funcall_lambda_report::args::FuncallLambdaReportArgs;
use crate::presentation::cli::funcall_lambda_report::render::print_funcall_lambda_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn funcall_lambda_report(
    args: FuncallLambdaReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut funcall_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_funcall_lambdas(file, dialect, &tree)?;
        funcall_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_funcall_lambdas(funcall_form_count, violations);
    let policy = evaluate_funcall_lambda_policy(
        FuncallLambdaPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_funcall_lambda_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "funcall-lambda-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
