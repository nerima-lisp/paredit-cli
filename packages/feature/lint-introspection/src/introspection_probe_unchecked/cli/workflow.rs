use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files_raw, expand_input_files, note_partial_file_failures, read_input_dialect_and_tree,
    total_file_failure,
};

use crate::introspection_probe_unchecked::cli::args::IntrospectionProbeUncheckedReportArgs;
use crate::introspection_probe_unchecked::cli::render::print_introspection_probe_unchecked_report;
use crate::introspection_probe_unchecked::usecase::{
    build_introspection_probe_unchecked_report, evaluate_fail_on_violation_policy,
};

pub fn introspection_probe_unchecked_report(
    args: IntrospectionProbeUncheckedReportArgs,
) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files_raw(&files, |file| {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        CliResult::Ok(build_introspection_probe_unchecked_report(
            file, dialect, &tree,
        )?)
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_introspection_probe_unchecked_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "introspection-probe-unchecked-report policy failed: {message}"
        )));
    }

    Ok(())
}
