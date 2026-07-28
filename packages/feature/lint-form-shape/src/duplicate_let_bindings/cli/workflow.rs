use anyhow::Result;

use crate::duplicate_let_bindings::cli::args::DuplicateLetBindingReportArgs;
use crate::duplicate_let_bindings::cli::render::print_duplicate_let_binding_report;
use crate::duplicate_let_bindings::usecase::{
    DuplicateLetBindingPolicyOptions, collect_duplicate_let_bindings,
    evaluate_duplicate_let_binding_policy, summarize_duplicate_let_bindings,
};
use paredit_core_cli::shared::read_input_dialect_and_tree;

pub fn duplicate_let_binding_report(args: DuplicateLetBindingReportArgs) -> Result<()> {
    let mut let_form_count = 0;
    let mut duplicates = Vec::new();

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_let_form_count, file_duplicates) =
            collect_duplicate_let_bindings(file, dialect, &tree)?;
        let_form_count += file_let_form_count;
        duplicates.extend(file_duplicates);
    }

    let summary = summarize_duplicate_let_bindings(let_form_count, duplicates);
    let policy = evaluate_duplicate_let_binding_policy(
        DuplicateLetBindingPolicyOptions::new(args.fail_on_duplicate),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_duplicate_let_binding_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "duplicate-let-binding-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
