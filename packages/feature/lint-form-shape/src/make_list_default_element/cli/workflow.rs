use paredit_core_cli::CommandResult;

use crate::make_list_default_element::cli::args::MakeListDefaultElementReportArgs;
use crate::make_list_default_element::cli::render::print_make_list_default_element_report;
use crate::make_list_default_element::usecase::{
    build_make_list_default_element_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn make_list_default_element_report(args: MakeListDefaultElementReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_make_list_default_element_report(
            file, dialect, &tree,
        )?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_make_list_default_element_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "make-list-default-element-report policy failed: {message}"
        )));
    }

    Ok(())
}
