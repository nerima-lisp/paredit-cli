use paredit_core_cli::CommandResult;

use crate::typep_predicate::cli::args::TypepPredicateReportArgs;
use crate::typep_predicate::cli::render::print_typep_predicate_report;
use crate::typep_predicate::usecase::{
    TypepPredicatePolicyOptions, collect_typep_predicates, evaluate_typep_predicate_policy,
    summarize_typep_predicates,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn typep_predicate_report(args: TypepPredicateReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut typep_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) = collect_typep_predicates(file, dialect, &tree)?;
        typep_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_typep_predicates(typep_form_count, violations);
    let policy = evaluate_typep_predicate_policy(
        TypepPredicatePolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_typep_predicate_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "typep-predicate-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
