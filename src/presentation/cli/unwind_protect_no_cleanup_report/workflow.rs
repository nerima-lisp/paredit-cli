use anyhow::Result;

use crate::application::usecase::unwind_protect_no_cleanup_report::{
    UnwindProtectNoCleanupPolicyOptions, collect_unwind_protect_no_cleanup,
    evaluate_unwind_protect_no_cleanup_policy, summarize_unwind_protect_no_cleanup,
};
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};
use crate::presentation::cli::unwind_protect_no_cleanup_report::args::UnwindProtectNoCleanupReportArgs;
use crate::presentation::cli::unwind_protect_no_cleanup_report::render::print_unwind_protect_no_cleanup_report;

pub(in crate::presentation::cli) fn unwind_protect_no_cleanup_report(
    args: UnwindProtectNoCleanupReportArgs,
) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut unwind_protect_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_unwind_protect_no_cleanup(file, dialect, &tree)?;
        unwind_protect_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_unwind_protect_no_cleanup(unwind_protect_form_count, violations);
    let policy = evaluate_unwind_protect_no_cleanup_policy(
        UnwindProtectNoCleanupPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_unwind_protect_no_cleanup_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "unwind-protect-no-cleanup-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
