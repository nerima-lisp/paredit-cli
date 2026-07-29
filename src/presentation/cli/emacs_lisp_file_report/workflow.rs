use paredit_core_cli::CommandResult;

use crate::presentation::cli::emacs_lisp_file_report::args::EmacsLispFileReportArgs;
use crate::presentation::cli::emacs_lisp_file_report::render::print_emacs_lisp_file_report;
use crate::presentation::cli::shared::read_input_dialect_and_tree;
use paredit_feature_emacs_lisp::file_report::usecase::{
    EmacsLispFilePolicyOptions, collect_emacs_lisp_file_facts, evaluate_emacs_lisp_file_policy,
};

pub(in crate::presentation::cli) fn emacs_lisp_file_report(
    args: EmacsLispFileReportArgs,
) -> CommandResult {
    let mut files = Vec::new();

    for file in &args.files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        files.push(collect_emacs_lisp_file_facts(
            file,
            dialect,
            &tree,
            &input.text,
        ));
    }

    let policy = evaluate_emacs_lisp_file_policy(
        EmacsLispFilePolicyOptions::new(args.fail_on_missing_lexical_binding),
        &files,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_emacs_lisp_file_report(&files, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "elisp-file policy failed: {policy_message}"
        )));
    }

    Ok(())
}
