use paredit_core_cli::CommandResult;

use crate::call_report::cli::{
    args::CallReportArgs, render::print_call_report, types::CallReportFile,
};
use crate::call_report::usecase::build_call_report;
use crate::error::ProjectAnalysisResult;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn call_report(args: CallReportArgs) -> CommandResult {
    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        let calls = build_call_report(
            tree,
            dialect,
            args.symbol.as_ref(),
            args.include_definitions,
        )?;
        ProjectAnalysisResult::Ok(CallReportFile {
            path: file.to_path_buf(),
            dialect,
            calls,
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let call_count = reports
        .iter()
        .map(|report| report.calls.len())
        .sum::<usize>();
    print_call_report(
        &reports,
        args.symbol.as_ref(),
        args.include_definitions,
        args.output,
    )?;

    match args.require_calls {
        Some(minimum) if call_count < minimum => {
            Err(paredit_core_cli::gate::gate_failure(format!(
                "require-calls policy failed: found {call_count} call sites, required at least {minimum}"
            )))
        }
        _ => Ok(()),
    }
}
