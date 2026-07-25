use anyhow::Result;

use crate::application::usecase::unused_nickname_report::{
    UnusedNicknamePolicyOptions, analyze_unused_nicknames, collect_declared_nicknames,
    collect_referenced_package_names, evaluate_unused_nickname_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::unused_nickname_report::args::UnusedNicknameReportArgs;
use crate::presentation::cli::unused_nickname_report::render::print_unused_nickname_report;

pub(in crate::presentation::cli) fn unused_nickname_report(
    args: UnusedNicknameReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "unused-nickname-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
