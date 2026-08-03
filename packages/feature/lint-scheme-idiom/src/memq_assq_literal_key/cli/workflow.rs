use paredit_core_cli::CommandResult;

use crate::memq_assq_literal_key::cli::args::MemqAssqLiteralKeyReportArgs;
use crate::memq_assq_literal_key::cli::render::print_memq_assq_literal_key_report;
use crate::memq_assq_literal_key::usecase::{
    build_memq_assq_literal_key_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn memq_assq_literal_key_report(args: MemqAssqLiteralKeyReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_memq_assq_literal_key_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_memq_assq_literal_key_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "scheme-memq-assq-literal-key-report policy failed: {message}"
        )));
    }

    Ok(())
}
