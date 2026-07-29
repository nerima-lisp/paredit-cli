use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_syntax::selector::SkipReason;

use crate::replace::cli::args::QueryReplaceArgs;
use crate::replace::usecase::{FileRewrite, RewriteTotals};

/// The reasons a match can be skipped, listed once so the summary reports the
/// zeroes too.
///
/// A skip count that only appears when it is non-zero reads as "this cannot
/// happen" until the day it does. Printing `comment_loss 0` is what makes
/// `comment_loss 4` a number the caller was already looking at.
const SKIP_REASONS: [SkipReason; 3] = [
    SkipReason::Overlapping,
    SkipReason::CommentLoss,
    SkipReason::Quoted,
];

pub fn print_replace_report(
    files: &[FileRewrite],
    totals: &RewriteTotals,
    args: &QueryReplaceArgs,
    output: OutputFormat,
) -> Result<()> {
    let touched: Vec<&FileRewrite> = files
        .iter()
        .filter(|file| file.is_touched() || !file.skipped.is_empty())
        .collect();

    match output {
        OutputFormat::Text => {
            println!(
                "files_scanned\t{}\tfiles_touched\t{}\treplacements\t{}\tskipped\t{}\tunchanged\t{}",
                totals.files_scanned,
                totals.files_touched,
                totals.replacements,
                totals.skipped,
                totals.unchanged
            );
            println!("written\t{}", args.write);
            for reason in SKIP_REASONS {
                println!(
                    "skipped\t{}\t{}",
                    reason.as_str(),
                    RewriteTotals::skipped_for(files, reason)
                );
            }
            for file in touched {
                for edit in &file.edits {
                    println!(
                        "replace\t{}\t{}:{}\t{}\t{}",
                        file.path.display(),
                        edit.line,
                        edit.column,
                        safe_text!(edit.before),
                        safe_text!(edit.after)
                    );
                }
                for skipped in &file.skipped {
                    println!(
                        "skip\t{}\t{}\t{}",
                        file.path.display(),
                        skipped.line,
                        skipped.reason.as_str()
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "query": args.query,
                "rewrite": args.rewrite,
                "written": args.write,
                "summary": {
                    "filesScanned": totals.files_scanned,
                    "filesTouched": totals.files_touched,
                    "replacements": totals.replacements,
                    "skipped": totals.skipped,
                    "unchanged": totals.unchanged,
                },
                "skippedByReason": SKIP_REASONS
                    .iter()
                    .map(|reason| json!({
                        "reason": reason.as_str(),
                        "count": RewriteTotals::skipped_for(files, *reason),
                        "detail": reason.detail(),
                    }))
                    .collect::<Vec<_>>(),
                "files": touched
                    .iter()
                    .map(|file| json!({
                        "path": file.path.display().to_string(),
                        "dialect": file.dialect.label(),
                        "unchanged": file.unchanged,
                        "replacements": file
                            .edits
                            .iter()
                            .map(|edit| json!({
                                "path": edit.path,
                                "line": edit.line,
                                "column": edit.column,
                                "before": edit.before,
                                "after": edit.after,
                            }))
                            .collect::<Vec<_>>(),
                        "skipped": file
                            .skipped
                            .iter()
                            .map(|skipped| json!({
                                "path": skipped.path,
                                "line": skipped.line,
                                "reason": skipped.reason.as_str(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}
