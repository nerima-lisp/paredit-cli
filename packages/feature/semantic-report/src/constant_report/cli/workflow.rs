use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::constant_report::cli::args::ConstantReportArgs;
use crate::constant_report::cli::render::print_constant_report;
use crate::constant_report::usecase::{
    ConstantReportPolicyOptions, build_constant_report, evaluate_constant_report_policy,
};
use crate::shared::SemanticFile;

pub fn constant_report(args: ConstantReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_constant_report(&SemanticFile::analyze(
            file, dialect, tree,
        )));
    }

    let policy = evaluate_constant_report_policy(
        ConstantReportPolicyOptions::new(args.fail_on_foldable),
        &reports,
    );
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_constant_report(&reports, &policy, args.min_saved_bytes, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "constant-report policy failed: {message}"
        )));
    }

    Ok(())
}
