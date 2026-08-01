use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

use crate::blame_report::cli::args::BlameReportArgs;
use crate::blame_report::cli::render::print_blame_report;
use crate::blame_report::usecase::{build_blame_report, evaluate_blame_policy, measure_blame};

pub fn blame_report(args: BlameReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        // One `git blame` per file, run in the file's own directory so a report
        // spanning several repositories answers for each of them.
        let blame = measure_blame(&file.canonicalize().unwrap_or_else(|_| file.clone()));
        reports.push(build_blame_report(file, dialect, &tree, &blame));
    }

    let policy = evaluate_blame_policy(args.fail_on_unattributed, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_blame_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "inspect blame policy failed: {message}"
        )));
    }

    Ok(())
}
