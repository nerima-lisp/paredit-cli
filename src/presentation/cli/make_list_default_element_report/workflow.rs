use anyhow::Result;

use crate::application::usecase::make_list_default_element_report::{
    MakeListDefaultElementPolicyOptions, collect_make_list_default_elements,
    evaluate_make_list_default_element_policy, summarize_make_list_default_elements,
};
use crate::presentation::cli::make_list_default_element_report::args::MakeListDefaultElementReportArgs;
use crate::presentation::cli::make_list_default_element_report::render::print_make_list_default_element_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn make_list_default_element_report(
    args: MakeListDefaultElementReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_make_list_default_elements(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_make_list_default_elements(call_form_count, violations);
    let policy = evaluate_make_list_default_element_policy(
        MakeListDefaultElementPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_make_list_default_element_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "make-list-default-element-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
