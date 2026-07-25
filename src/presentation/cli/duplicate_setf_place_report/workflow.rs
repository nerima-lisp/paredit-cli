use anyhow::Result;

use crate::application::usecase::duplicate_setf_place_report::{
    DuplicateSetfPlacePolicyOptions, collect_duplicate_setf_places,
    evaluate_duplicate_setf_place_policy, summarize_duplicate_setf_places,
};
use crate::presentation::cli::duplicate_setf_place_report::args::DuplicateSetfPlaceReportArgs;
use crate::presentation::cli::duplicate_setf_place_report::render::print_duplicate_setf_place_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn duplicate_setf_place_report(
    args: DuplicateSetfPlaceReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "duplicate-setf-place-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
