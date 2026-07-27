use anyhow::Result;

use crate::class_cycle_report::cli::args::ClassCycleReportArgs;
use crate::class_cycle_report::cli::render::print_class_cycle_report;
use crate::class_cycle_report::usecase::{
    ClassCyclePolicyOptions, analyze_class_cycles, collect_class_inheritance_edges,
    evaluate_class_cycle_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn class_cycle_report(args: ClassCycleReportArgs) -> Result<()> {
    let mut edges = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        edges.extend(collect_class_inheritance_edges(dialect, &tree)?);
    }

    let summary = analyze_class_cycles(&edges);
    let policy =
        evaluate_class_cycle_policy(ClassCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_class_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "class-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
