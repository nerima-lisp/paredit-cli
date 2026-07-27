use anyhow::Result;

use crate::application::usecase::struct_cycle_report::{
    StructCyclePolicyOptions, analyze_struct_cycles, collect_struct_inheritance_edges,
    evaluate_struct_cycle_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::struct_cycle_report::args::StructCycleReportArgs;
use crate::presentation::cli::struct_cycle_report::render::print_struct_cycle_report;

pub(in crate::presentation::cli) fn struct_cycle_report(args: StructCycleReportArgs) -> Result<()> {
    let mut edges = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        edges.extend(collect_struct_inheritance_edges(dialect, &tree)?);
    }

    let summary = analyze_struct_cycles(&edges);
    let policy =
        evaluate_struct_cycle_policy(StructCyclePolicyOptions::new(args.fail_on_cycle), &summary);
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_struct_cycle_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "struct-cycle-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
