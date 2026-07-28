use anyhow::Result;

use crate::duplicate_setf_places::cli::args::DuplicateSetfPlaceReportArgs;
use crate::duplicate_setf_places::cli::render::print_duplicate_setf_place_report;
use crate::duplicate_setf_places::usecase::{
    DuplicateSetfPlacePolicyOptions, collect_duplicate_setf_places,
    evaluate_duplicate_setf_place_policy, summarize_duplicate_setf_places,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn duplicate_setf_place_report(args: DuplicateSetfPlaceReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_duplicate_setf_places(file, dialect, &tree)?;
        assignment_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_duplicate_setf_places(assignment_form_count, violations);
    let policy = evaluate_duplicate_setf_place_policy(
        DuplicateSetfPlacePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_setf_place_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-setf-place-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
