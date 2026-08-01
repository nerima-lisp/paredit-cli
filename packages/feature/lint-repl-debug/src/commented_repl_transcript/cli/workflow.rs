use paredit_core_cli::CommandResult;

use crate::commented_repl_transcript::cli::args::CommentedReplTranscriptReportArgs;
use crate::commented_repl_transcript::cli::render::print_commented_repl_transcript_report;
use crate::commented_repl_transcript::usecase::{
    CommentedReplTranscriptPolicyOptions, collect_commented_repl_transcript,
    evaluate_commented_repl_transcript_policy, summarize_commented_repl_transcript,
};
use paredit_core_cli::shared::{expand_input_files, read_input_dialect_and_tree};

pub fn commented_repl_transcript_report(args: CommentedReplTranscriptReportArgs) -> CommandResult {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut scanned_form_count = 0;
    let mut violations = Vec::new();

    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (file_form_count, file_violations) =
            collect_commented_repl_transcript(file, dialect, &tree)?;
        scanned_form_count += file_form_count;
        violations.extend(file_violations);
    }

    let summary = summarize_commented_repl_transcript(scanned_form_count, violations);
    let policy = evaluate_commented_repl_transcript_policy(
        CommentedReplTranscriptPolicyOptions::new(args.fail_on_violation),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_commented_repl_transcript_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "commented-repl-transcript-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}
