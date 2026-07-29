//! `query find` — which forms in this repository have this shape?
//!
//! `inspect resolve` answers the same question for one named file, as a way
//! of debugging a selector before editing with it. This answers it for a file
//! set, which is a different job: the pattern stops being scaffolding for an
//! edit and becomes the query itself.
//!
//! The pattern is parsed **once**, before any file is read, against the
//! dialect `--dialect` names or Common Lisp when it names none. Parsing it per
//! file would let `--query '(defun'` be reported as a failure of the four
//! hundredth file rather than of the command line.

use paredit_core_cli::CommandResult;

use paredit_core_cli::args::DialectArg;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::Pattern;

use crate::find_report::cli::args::QueryFindArgs;
use crate::find_report::cli::render::print_find_report;
use crate::find_report::usecase::{build_find_report, evaluate_find_policy};
use crate::scan::selected_files;

pub fn query_find(args: QueryFindArgs) -> CommandResult {
    let pattern = Pattern::parse(&args.query, pattern_dialect(args.dialect))?;
    let files = selected_files(&args.input, &args.roots)?;

    let mut reports = Vec::with_capacity(files.len());
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        reports.push(build_find_report(
            file,
            dialect,
            &tree,
            &pattern,
            args.preview_bytes,
        ));
    }

    let policy = evaluate_find_policy(args.fail_on_match, args.fail_on_no_match, &reports);
    let passed = policy.passed;
    let message = policy.violations.join("; ");

    print_find_report(&reports, &policy, args.output)?;

    if !passed {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "query find policy failed: {message}"
        )));
    }
    Ok(())
}

/// Which reader parses the pattern text.
///
/// A pattern is read by the source language's own reader, so a mixed-dialect
/// scan has no single right answer. Common Lisp is the default because it is
/// the dialect this tool models deepest and the one whose reader accepts the
/// widest set of the tokens the other nine use; a caller scanning Clojure or
/// Fennel says `--dialect` and gets that reader for both the pattern and the
/// files.
fn pattern_dialect(explicit: Option<DialectArg>) -> Dialect {
    explicit.map_or(Dialect::CommonLisp, Dialect::from)
}
