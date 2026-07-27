use anyhow::Result;

use crate::signature_report::cli::args::SignatureReportArgs;
use crate::signature_report::cli::render::print_signature_report;
use crate::signature_report::usecase::{
    SignatureReportSource, build_signature_reports, evaluate_signature_report_policy,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn signature_report(args: SignatureReportArgs) -> Result<()> {
    let symbol = args.symbol.as_ref();
    let mut sources = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        sources.push(SignatureReportSource {
            path: file.clone(),
            dialect,
            tree,
        });
    }

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
