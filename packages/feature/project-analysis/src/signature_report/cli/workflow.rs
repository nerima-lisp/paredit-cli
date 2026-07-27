use anyhow::Result;

use crate::application::usecase::signature_report::{
    SignatureReportSource, build_signature_reports, evaluate_signature_report_policy,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use crate::presentation::cli::signature_report::args::SignatureReportArgs;
use crate::presentation::cli::signature_report::render::print_signature_report;

pub(in crate::presentation::cli) fn signature_report(args: SignatureReportArgs) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(
            "signature-report policy failed",
        ));
    }
    Ok(())
}
