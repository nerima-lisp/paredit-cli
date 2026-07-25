use anyhow::Result;

use crate::application::usecase::getf_default_nil_report::{
    GetfDefaultNilPolicyOptions, collect_getf_default_nils, evaluate_getf_default_nil_policy,
    summarize_getf_default_nils,
};
use crate::presentation::cli::getf_default_nil_report::args::GetfDefaultNilReportArgs;
use crate::presentation::cli::getf_default_nil_report::render::print_getf_default_nil_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn getf_default_nil_report(
    args: GetfDefaultNilReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_getf_default_nils(file, dialect, &tree)?;
        call_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_getf_default_nils(call_form_count, violations);
    let policy = evaluate_getf_default_nil_policy(
        GetfDefaultNilPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_getf_default_nil_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "getf-default-nil-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
