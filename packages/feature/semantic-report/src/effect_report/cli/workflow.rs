use anyhow::Result;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::effect_report::cli::args::EffectReportArgs;
use crate::effect_report::cli::render::print_effect_report;
use crate::effect_report::usecase::{
    EffectPolicyOptions, build_effect_report, evaluate_effect_policy,
};
use crate::shared::SemanticFile;

pub fn effect_report(args: EffectReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_effect_report(&SemanticFile::analyze(
            file, dialect, tree,
        )));
    }

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
