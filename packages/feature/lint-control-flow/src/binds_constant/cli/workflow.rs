use anyhow::Result;

use crate::binds_constant::cli::args::BindsConstantReportArgs;
use crate::binds_constant::cli::render::print_binds_constant_report;
use crate::binds_constant::usecase::{
    BindsConstantPolicyOptions, collect_binds_constant, evaluate_binds_constant_policy,
    summarize_binds_constant,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn binds_constant_report(args: BindsConstantReportArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut binding_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_binding_form_count, file_violations) =
            collect_binds_constant(file, dialect, &tree)?;
        binding_form_count += file_binding_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_binds_constant(binding_form_count, violations);
    let policy = evaluate_binds_constant_policy(
        BindsConstantPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_binds_constant_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "binds-constant-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
