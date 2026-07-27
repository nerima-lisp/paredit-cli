use anyhow::Result;

use crate::application::usecase::malformed_let_binding_report::{
    MalformedLetBindingPolicyOptions, collect_malformed_let_bindings,
    evaluate_malformed_let_binding_policy, summarize_malformed_let_bindings,
};
use crate::presentation::cli::malformed_let_binding_report::args::MalformedLetBindingReportArgs;
use crate::presentation::cli::malformed_let_binding_report::render::print_malformed_let_binding_report;
use crate::presentation::cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub(in crate::presentation::cli) fn malformed_let_binding_report(
    args: MalformedLetBindingReportArgs,
) -> Result<()> {
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
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "malformed-let-binding-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
