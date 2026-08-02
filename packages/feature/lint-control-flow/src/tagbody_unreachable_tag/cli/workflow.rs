use paredit_core_cli::CommandResult;

use crate::tagbody_unreachable_tag::cli::args::TagbodyUnreachableTagReportArgs;
use crate::tagbody_unreachable_tag::cli::render::print_tagbody_unreachable_tag_report;
use crate::tagbody_unreachable_tag::usecase::{
    build_tagbody_unreachable_tag_report, evaluate_fail_on_violation_policy,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn tagbody_unreachable_tag_report(args: TagbodyUnreachableTagReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_tagbody_unreachable_tag_report(file, dialect, &tree)?);
    }

    let policy = evaluate_fail_on_violation_policy(args.fail_on_violation, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_tagbody_unreachable_tag_report(&reports, &policy, args.output, args.verbosity)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "tagbody-unreachable-tag-report policy failed: {message}"
        )));
    }

    Ok(())
}
