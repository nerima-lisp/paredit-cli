use paredit_core_cli::CommandResult;

use crate::signature_report::cli::args::SignatureReportArgs;
use crate::signature_report::cli::render::print_signature_report;
use crate::signature_report::usecase::{
    SignatureReportSource, build_signature_reports, evaluate_signature_report_policy,
};
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

pub fn signature_report(args: SignatureReportArgs) -> CommandResult {
    let symbol = args.symbol.as_ref();

    // A file that will not parse is reported, not fatal — see `query find`.
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(SignatureReportSource {
            path: file.to_path_buf(),
            dialect,
            tree: tree.clone(),
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let sources = analysis.succeeded;

    let reports = build_signature_reports(sources, symbol)?;
    let policy = evaluate_signature_report_policy(
        &reports,
        args.fail_on_mismatch,
        args.require_definitions,
        args.require_calls,
    );
    print_signature_report(&reports, symbol, &policy, args.output)?;
    if !policy.passed {
        return Err(paredit_core_cli::gate::gate_failure(
            "signature-report policy failed",
        ));
    }
    Ok(())
}
