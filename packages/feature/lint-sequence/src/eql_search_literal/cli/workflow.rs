use paredit_core_cli::CommandResult;

use crate::eql_search_literal::cli::args::EqlSearchLiteralReportArgs;
use crate::eql_search_literal::cli::render::print_eql_search_literal_report;
use crate::eql_search_literal::usecase::{
    build_eql_search_literal_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn eql_search_literal_report(args: EqlSearchLiteralReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_eql_search_literal_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_eql_search_literal_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "eql-search-literal-report policy failed: {message}"
        )));
    }

    Ok(())
}
