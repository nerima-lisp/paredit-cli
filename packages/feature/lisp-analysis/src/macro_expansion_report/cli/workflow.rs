use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::macro_expansion_report::cli::args::MacroExpansionReportArgs;
use crate::macro_expansion_report::cli::render::print_declined_report;
use crate::macro_expansion_report::usecase::{
    build_macro_expansion_report, evaluate_fail_on_declined_policy,
};

pub fn macro_expansion_report(args: MacroExpansionReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_macro_expansion_report(file, dialect, &tree));
    }

    let policy = evaluate_fail_on_declined_policy(args.fail_on_declined, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_declined_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect macro-expansion policy failed: {message}"
        )));
    }

    Ok(())
}
