use paredit_core_cli::CommandResult;

use crate::unused_nickname_report::cli::args::UnusedNicknameReportArgs;
use crate::unused_nickname_report::cli::render::print_unused_nickname_report;
use crate::unused_nickname_report::usecase::{
    UnusedNicknamePolicyOptions, analyze_unused_nicknames, collect_declared_nicknames,
    collect_referenced_package_names, evaluate_unused_nickname_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn unused_nickname_report(args: UnusedNicknameReportArgs) -> CommandResult {
    let mut declared = Vec::new();
    let mut referenced = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        declared.extend(collect_declared_nicknames(file, dialect, &tree)?);
        referenced.extend(collect_referenced_package_names(dialect, &tree)?);
    }

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
