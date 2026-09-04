use paredit_core_cli::CommandResult;

use crate::malformed_let_binding::cli::args::MalformedLetBindingReportArgs;
use crate::malformed_let_binding::cli::render::print_malformed_let_binding_report;
use crate::malformed_let_binding::usecase::{
    build_malformed_let_binding_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn malformed_let_binding_report(args: MalformedLetBindingReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_malformed_let_binding_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_malformed_let_binding_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "malformed-let-binding-report policy failed: {message}"
        )));
    }

    Ok(())
}
