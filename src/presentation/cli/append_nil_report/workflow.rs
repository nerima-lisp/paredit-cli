use anyhow::Result;

use crate::application::usecase::append_nil_report::{
    AppendNilPolicyOptions, collect_append_nils, evaluate_append_nil_policy, summarize_append_nils,
};
use crate::presentation::cli::append_nil_report::args::AppendNilReportArgs;
use crate::presentation::cli::append_nil_report::render::print_append_nil_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn append_nil_report(args: AppendNilReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut append_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_append_nils(file, dialect, &tree)?;
        append_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_append_nils(append_form_count, violations);
    let policy = evaluate_append_nil_policy(
        AppendNilPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_append_nil_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "append-nil-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
