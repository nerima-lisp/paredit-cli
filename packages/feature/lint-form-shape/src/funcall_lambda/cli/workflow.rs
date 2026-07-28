use anyhow::Result;

use crate::funcall_lambda::cli::args::FuncallLambdaReportArgs;
use crate::funcall_lambda::cli::render::print_funcall_lambda_report;
use crate::funcall_lambda::usecase::{
    FuncallLambdaPolicyOptions, collect_funcall_lambdas, evaluate_funcall_lambda_policy,
    summarize_funcall_lambdas,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn funcall_lambda_report(args: FuncallLambdaReportArgs) -> Result<()> {
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
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "funcall-lambda-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
