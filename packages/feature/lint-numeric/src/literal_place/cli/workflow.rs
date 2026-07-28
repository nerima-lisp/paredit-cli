use anyhow::Result;

use crate::literal_place::cli::args::LiteralPlaceReportArgs;
use crate::literal_place::cli::render::print_literal_place_report;
use crate::literal_place::usecase::{
    LiteralPlacePolicyOptions, collect_literal_places, evaluate_literal_place_policy,
    summarize_literal_places,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn literal_place_report(args: LiteralPlaceReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut modify_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_literal_places(file, dialect, &tree)?;
        modify_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_literal_places(modify_form_count, violations);
    let policy = evaluate_literal_place_policy(
        LiteralPlacePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_literal_place_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "literal-place-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
