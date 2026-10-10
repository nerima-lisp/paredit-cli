use paredit_core_cli::CommandResult;
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use super::args::TestRiskReportArgs;
use super::render::print_test_risk_report;
use crate::test_risk_report::usecase::{build_test_risk_report, evaluate_test_risk_policy};

pub fn test_risk_report(args: TestRiskReportArgs) -> CommandResult {
    let mut files = expand_input_files(&args.files, args.dialect)?;
    files.sort();
    let mut inputs = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        inputs.push((file.as_path(), dialect, tree));
    }
    let documents = inputs
        .iter()
        .map(|(path, dialect, tree)| (*path, *dialect, tree))
        .collect::<Vec<_>>();
    let report = build_test_risk_report(&documents)?;
    let policy = evaluate_test_risk_policy(args.fail_on_untested, &report);
    print_test_risk_report(&report, &policy, args.top, args.output)?;
    if !policy.passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect test-risk policy failed: {}",
            policy.violations.join("; ")
        )));
    }
    Ok(())
}
