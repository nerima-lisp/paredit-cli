use paredit_core_cli::CommandResult;

use crate::funcall_lambda::cli::args::FuncallLambdaReportArgs;
use crate::funcall_lambda::cli::render::print_funcall_lambda_report;
use crate::funcall_lambda::usecase::{
    build_funcall_lambda_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn funcall_lambda_report(args: FuncallLambdaReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_funcall_lambda_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_funcall_lambda_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "funcall-lambda-report policy failed: {message}"
        )));
    }

    Ok(())
}
