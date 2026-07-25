use anyhow::Result;

use crate::application::usecase::duplicate_export_report::{
    DuplicateExportPolicyOptions, collect_duplicate_exports, evaluate_duplicate_export_policy,
    summarize_duplicate_exports,
};
use crate::presentation::cli::duplicate_export_report::args::DuplicateExportReportArgs;
use crate::presentation::cli::duplicate_export_report::render::print_duplicate_export_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn duplicate_export_report(
    args: DuplicateExportReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut defpackage_count = 0;
    let mut duplicates = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_defpackage_count, file_duplicates) =
            collect_duplicate_exports(file, dialect, &tree)?;
        defpackage_count += file_defpackage_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_exports(defpackage_count, duplicates);
    let policy = evaluate_duplicate_export_policy(
        DuplicateExportPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_export_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "duplicate-export-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
