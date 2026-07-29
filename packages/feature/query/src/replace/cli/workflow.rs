//! `query replace` — structural search-and-replace.
//!
//! Every other write command in this tool knows its edit at compile time:
//! `convert-if-to-when` converts `if` to `when` and nothing else, and the
//! shape it is allowed to produce was reviewed when it was written. This one
//! takes both halves from the caller, which is what makes a codemod something
//! a user can write — and also why the safety has to be structural rather
//! than per-transform.
//!
//! Three things stand between a template and a corrupted file:
//!
//! 1. **Nothing is written without `--write`.** The default prints the plan.
//! 2. **The rewrite must reparse.** [`emit_document`] refuses to persist text
//!    that no longer reads as the input dialect. This catches an unbalanced
//!    template and nothing subtler, which is exactly why it is not the only
//!    guard.
//! 3. **Two silent-loss cases are refused up front**, in
//!    `paredit_core_syntax::selector::rewrite`: rewriting inside a match
//!    already rewritten, and rewriting away a comment no capture carries.
//!    Both produce source that parses, so the reparse guard would pass them.
//!
//! What is deliberately *not* guaranteed: that the rewrite preserves meaning.
//! `--query '(car ?x)' --rewrite '(cdr ?x)'` is a valid instruction and this
//! command carries it out. The reviewable artifact is the plan, which is why
//! it prints by default.

use anyhow::Result;

use paredit_core_cli::args::DialectArg;
use paredit_core_cli::shared::{emit_document, read_input_dialect_and_tree};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{Pattern, Template};

use crate::replace::cli::args::QueryReplaceArgs;
use crate::replace::cli::render::print_replace_report;
use crate::replace::usecase::{FileRewrite, RewriteTotals, build_file_rewrite};
use crate::scan::selected_files;

pub fn query_replace(args: QueryReplaceArgs) -> Result<()> {
    let dialect = pattern_dialect(args.dialect);
    let pattern = Pattern::parse(&args.query, dialect)?;
    let template = Template::parse(&args.rewrite, dialect)?;
    // Before a single file is read: a template naming a capture the pattern
    // does not bind is a mistake in the command line, and discovering it on
    // file four hundred sends the caller looking at the wrong thing.
    template.check_against(&pattern)?;

    let files = selected_files(&args.input, &args.roots)?;
    let mut rewrites: Vec<(FileRewrite, _)> = Vec::with_capacity(files.len());
    for file in &files {
        let (input, file_dialect, tree) =
            read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let rewrite = build_file_rewrite(
            file,
            file_dialect,
            &tree,
            &pattern,
            &template,
            args.allow_comment_loss,
        );
        rewrites.push((rewrite, (input, file_dialect)));
    }

    let plans: Vec<FileRewrite> = rewrites.iter().map(|(plan, _)| plan.clone()).collect();
    let totals = RewriteTotals::of(&plans);

    if args.write || args.diff {
        for (plan, (input, file_dialect)) in &rewrites {
            let Some(rewritten) = plan.rewritten.clone() else {
                continue;
            };
            emit_document(input, *file_dialect, args.write, args.diff, rewritten)?;
        }
        // With --diff the diff *is* the report; printing the plan beside it
        // would say the same thing twice in two formats.
        if !args.diff {
            print_replace_report(&plans, &totals, &args, args.output)?;
        }
    } else {
        print_replace_report(&plans, &totals, &args, args.output)?;
    }

    if args.check && totals.replacements > 0 {
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "query replace policy failed: --check, {} replacement(s) pending in {} file(s)",
            totals.replacements, totals.files_touched
        )));
    }
    Ok(())
}

/// Which reader parses the pattern and the template.
///
/// Both are read with one reader, never with each file's own: a pattern that
/// meant one thing in `foo.lisp` and another in `foo.clj` would make a single
/// `--rewrite` two different instructions in one run.
fn pattern_dialect(explicit: Option<DialectArg>) -> Dialect {
    explicit.map_or(Dialect::CommonLisp, Dialect::from)
}
