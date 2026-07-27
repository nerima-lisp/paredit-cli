use anyhow::Result;

use crate::application::usecase::defpackage_quoted_report::{
    DefpackageQuotedPolicyOptions, collect_defpackage_quoted, evaluate_defpackage_quoted_policy,
    summarize_defpackage_quoted,
};
use crate::presentation::cli::defpackage_quoted_report::args::DefpackageQuotedReportArgs;
use crate::presentation::cli::defpackage_quoted_report::render::print_defpackage_quoted_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn defpackage_quoted_report(
    args: DefpackageQuotedReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut defpackage_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_defpackage_quoted(file, dialect, &tree)?;
        defpackage_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_defpackage_quoted(defpackage_form_count, violations);
    let policy = evaluate_defpackage_quoted_policy(
        DefpackageQuotedPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_defpackage_quoted_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "defpackage-quoted-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
