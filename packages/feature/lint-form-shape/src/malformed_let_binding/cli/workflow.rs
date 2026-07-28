use anyhow::Result;

use crate::malformed_let_binding::cli::args::MalformedLetBindingReportArgs;
use crate::malformed_let_binding::cli::render::print_malformed_let_binding_report;
use crate::malformed_let_binding::usecase::{
    MalformedLetBindingPolicyOptions, collect_malformed_let_bindings,
    evaluate_malformed_let_binding_policy, summarize_malformed_let_bindings,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn malformed_let_binding_report(args: MalformedLetBindingReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut let_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_let_form_count, file_violations) =
            collect_malformed_let_bindings(file, dialect, &tree)?;
        let_form_count += file_let_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_malformed_let_bindings(let_form_count, violations);
    let policy = evaluate_malformed_let_binding_policy(
        MalformedLetBindingPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_malformed_let_binding_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "malformed-let-binding-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
