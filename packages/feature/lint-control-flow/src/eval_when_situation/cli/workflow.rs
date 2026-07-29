use paredit_core_cli::CommandResult;

use crate::eval_when_situation::cli::args::EvalWhenSituationReportArgs;
use crate::eval_when_situation::cli::render::print_eval_when_situation_report;
use crate::eval_when_situation::usecase::{
    EvalWhenSituationPolicyOptions, collect_eval_when_situations,
    evaluate_eval_when_situation_policy, summarize_eval_when_situations,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn eval_when_situation_report(args: EvalWhenSituationReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut eval_when_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_eval_when_situations(file, dialect, &tree)?;
        eval_when_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_eval_when_situations(eval_when_form_count, violations);
    let policy = evaluate_eval_when_situation_policy(
        EvalWhenSituationPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_eval_when_situation_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eval-when-situation-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
