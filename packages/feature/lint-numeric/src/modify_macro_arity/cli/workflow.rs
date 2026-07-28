use anyhow::Result;

use crate::modify_macro_arity::cli::args::ModifyMacroArityReportArgs;
use crate::modify_macro_arity::cli::render::print_modify_macro_arity_report;
use crate::modify_macro_arity::usecase::{
    ModifyMacroArityPolicyOptions, collect_modify_macro_arity_violations,
    evaluate_modify_macro_arity_policy, summarize_modify_macro_arity,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn modify_macro_arity_report(args: ModifyMacroArityReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut call_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_call_count, file_violations) =
            collect_modify_macro_arity_violations(file, dialect, &tree)?;
        call_count += file_call_count;
        violations.extend(file_violations);
    }

    let summary = summarize_modify_macro_arity(call_count, violations);
    let policy = evaluate_modify_macro_arity_policy(
        ModifyMacroArityPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_modify_macro_arity_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "modify-macro-arity-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
