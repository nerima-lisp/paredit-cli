use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::narrowing_report::cli::args::NarrowingReportArgs;
use crate::narrowing_report::cli::render::print_narrowing_report;
use crate::narrowing_report::usecase::{
    NarrowingPolicyOptions, build_narrowing_report, evaluate_narrowing_policy,
};
use crate::shared::SemanticFile;

pub fn narrowing_report(args: NarrowingReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_narrowing_report(&SemanticFile::analyze(
            file, dialect, tree,
        )));
    }

    let policy =
        evaluate_narrowing_policy(NarrowingPolicyOptions::new(args.fail_on_none), &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_narrowing_report(&reports, &policy, args.binding.as_deref(), args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "narrowing-report policy failed: {message}"
        )));
    }

    Ok(())
}
