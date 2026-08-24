use paredit_core_cli::{CliResult, CommandResult};

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, note_partial_file_failures, total_file_failure,
};

use crate::effect_report::cli::args::EffectReportArgs;
use crate::effect_report::cli::render::print_effect_report;
use crate::effect_report::usecase::{
    EffectPolicyOptions, build_effect_report, evaluate_effect_policy,
};
use crate::shared::SemanticFile;

pub fn effect_report(args: EffectReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _| {
        CliResult::Ok(build_effect_report(&SemanticFile::analyze(
            file,
            dialect,
            tree.clone(),
        )))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let reports = analysis.succeeded;

    let policy = evaluate_effect_policy(EffectPolicyOptions::new(args.fail_on_unknown), &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_effect_report(&reports, &policy, args.purity, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "effect-report policy failed: {message}"
        )));
    }

    Ok(())
}
