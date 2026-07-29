use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::shared::SemanticFile;
use crate::value_propagation_report::cli::args::ValuePropagationReportArgs;
use crate::value_propagation_report::cli::render::print_value_propagation_report;
use crate::value_propagation_report::usecase::{
    ValuePropagationPolicyOptions, build_value_propagation_report,
    evaluate_value_propagation_policy,
};

pub fn value_propagation_report(args: ValuePropagationReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_value_propagation_report(&SemanticFile::analyze(
            file, dialect, tree,
        )));
    }

    let policy = evaluate_value_propagation_policy(
        ValuePropagationPolicyOptions::new(args.min_coverage),
        &reports,
    );
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_value_propagation_report(&reports, &policy, args.blocked_only, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "value-propagation-report policy failed: {message}"
        )));
    }

    Ok(())
}
