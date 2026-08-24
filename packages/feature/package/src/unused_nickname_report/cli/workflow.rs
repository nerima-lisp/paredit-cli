use paredit_core_cli::{CliResult, CommandResult};

use crate::unused_nickname_report::cli::args::UnusedNicknameReportArgs;
use crate::unused_nickname_report::cli::render::print_unused_nickname_report;
use crate::unused_nickname_report::usecase::{
    UnusedNicknamePolicyOptions, analyze_unused_nicknames, collect_declared_nicknames,
    collect_referenced_package_names, evaluate_unused_nickname_policy,
};
use paredit_core_cli::shared::{
    analyze_files_raw, note_partial_file_failures, read_input_dialect_and_tree, total_file_failure,
};

pub fn unused_nickname_report(args: UnusedNicknameReportArgs) -> CommandResult {
    let analysis = analyze_files_raw(&args.files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let declared = collect_declared_nicknames(file, dialect, &tree)?;
        let referenced = collect_referenced_package_names(dialect, &tree)?;
        CliResult::Ok((declared, referenced))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let (declared_lists, referenced_lists): (Vec<_>, Vec<_>) =
        analysis.succeeded.into_iter().unzip();
    let declared: Vec<_> = declared_lists.into_iter().flatten().collect();
    let referenced: Vec<_> = referenced_lists.into_iter().flatten().collect();

    let summary = analyze_unused_nicknames(&declared, &referenced);
    let policy = evaluate_unused_nickname_policy(
        UnusedNicknamePolicyOptions::new(args.fail_on_unused),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_unused_nickname_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "unused-nickname-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
