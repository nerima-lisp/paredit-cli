use anyhow::Result;

use crate::prog2_to_progn::cli::args::Prog2ToPrognReportArgs;
use crate::prog2_to_progn::cli::render::print_prog2_to_progn_report;
use crate::prog2_to_progn::usecase::{
    Prog2ToPrognPolicyOptions, collect_prog2_to_progn, evaluate_prog2_to_progn_policy,
    summarize_prog2_to_progn,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn prog2_to_progn_report(args: Prog2ToPrognReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut prog2_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_prog2_to_progn(file, dialect, &tree)?;
        prog2_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_prog2_to_progn(prog2_form_count, violations);
    let policy = evaluate_prog2_to_progn_policy(
        Prog2ToPrognPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_prog2_to_progn_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "prog2-to-progn-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
