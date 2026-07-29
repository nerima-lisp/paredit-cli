use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::shared::SemanticFile;
use crate::type_report::cli::args::TypeReportArgs;
use crate::type_report::cli::render::print_type_report;
use crate::type_report::usecase::{
    TypeReportPolicyOptions, build_type_report, evaluate_type_report_policy,
};

pub fn type_report(args: TypeReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_type_report(&SemanticFile::analyze(
            file, dialect, tree,
        )));
    }

    let policy = evaluate_type_report_policy(
        TypeReportPolicyOptions::new(args.fail_on_contradiction),
        &reports,
    );
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_type_report(&reports, &policy, args.contradictions_only, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "type-report policy failed: {message}"
        )));
    }

    Ok(())
}
