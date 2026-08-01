use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::macro_hygiene_report::cli::args::MacroHygieneReportArgs;
use crate::macro_hygiene_report::cli::render::print_risk_report;
use crate::macro_hygiene_report::usecase::{
    build_macro_hygiene_report, evaluate_fail_on_risk_policy,
};

pub fn macro_hygiene_report(args: MacroHygieneReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_macro_hygiene_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_risk_policy(args.fail_on_risk, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_risk_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect macro-hygiene policy failed: {message}"
        )));
    }

    Ok(())
}
