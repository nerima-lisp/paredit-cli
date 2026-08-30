use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::color::{Painter, colorize_diff};
use paredit_core_cli::shared::{
    analyze_files, apply_byte_span_edits, expand_input_files, note_partial_file_failures,
    total_file_failure, unified_diff, write_files_with_rollback,
};
use paredit_core_syntax::sexpr::ByteSpan;

use crate::constant_report::domain::build_constant_report;
use crate::fold_constants::cli::args::FoldConstantsArgs;
use crate::fold_constants::domain::should_fold;
use crate::shared::SemanticFile;

/// One file's planned folds and the text they produce.
struct FoldPlan {
    path: std::path::PathBuf,
    dialect: paredit_core_syntax::dialect::Dialect,
    before: String,
    rewritten: String,
    /// `(as written, folded to, bytes removed)`, in source order.
    folds: Vec<(String, String, i64)>,
}

/// Replaces every provably-constant expression with the literal it evaluates
/// to.
///
/// What is folded: compound forms outside every quote context whose value is
/// an integer, character, keyword, boolean, `nil`, or string. Nested folds are
/// excluded by the report, which yields only the outermost form of each folded
/// region, so the spans are non-overlapping and apply in one pass.
///
/// What is not folded, and why the exclusions are not "by construction":
///
/// * Anything under `'`, `` ` ``, `#.`, a reader conditional, or a reader
///   label, at any depth. The value layer declining to evaluate a quoted form
///   is not enough on its own — that check sees one form's own prefixes, so a
///   walk into the *children* of `'(a (+ 1 2))` would lose the quote. The
///   report prunes those subtrees instead; see
///   `constant_report::domain::opens_unevaluated_context`, which also explains
///   why `,` does not re-open evaluated context here.
/// * Floats, refused by [`should_fold`]: the value layer keeps an `f64` and
///   drops the exponent marker, so no spelling this could emit preserves a
///   `double-float`'s type.
pub fn fold_constants(args: FoldConstantsArgs) -> CliResult<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, input| {
        let report = build_constant_report(&SemanticFile::analyze(file, dialect, tree.clone()));

        let mut edits: Vec<(ByteSpan, String)> = Vec::new();
        let mut folds = Vec::new();
        for foldable in &report.foldable {
            if !should_fold(foldable, args.min_saved_bytes) {
                continue;
            }
            edits.push((foldable.span, foldable.value.clone()));
            folds.push((
                foldable.text.clone(),
                foldable.value.clone(),
                foldable.saved_bytes,
            ));
        }

        let rewritten = if edits.is_empty() {
            input.text.clone()
        } else {
            apply_byte_span_edits(&input.text, edits)?
        };
        CliResult::Ok(FoldPlan {
            path: file.to_path_buf(),
            dialect,
            before: input.text.clone(),
            rewritten,
            folds,
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);
    let plans = analysis.succeeded;

    if args.diff {
        let painter = Painter::stdout();
        for plan in &plans {
            if plan.rewritten != plan.before {
                print!(
                    "{}",
                    colorize_diff(
                        painter,
                        &unified_diff(&plan.path, &plan.before, &plan.rewritten)
                    )
                );
            }
        }
    } else {
        print_plans(&plans, args.output)?;
    }

    if args.write {
        let written: Vec<(std::path::PathBuf, String)> = plans
            .iter()
            .filter(|plan| plan.rewritten != plan.before)
            .map(|plan| (plan.path.clone(), plan.rewritten.clone()))
            .collect();
        if !written.is_empty() {
            write_files_with_rollback(written)?;
        }
    }

    Ok(())
}

fn print_plans(plans: &[FoldPlan], output: OutputFormat) -> CliResult<()> {
    let fold_count: usize = plans.iter().map(|plan| plan.folds.len()).sum();
    let saved_bytes: i64 = plans
        .iter()
        .flat_map(|plan| &plan.folds)
        .map(|(_, _, saved)| *saved)
        .sum();

    match output {
        OutputFormat::Text => {
            for plan in plans {
                for (text, value, saved) in &plan.folds {
                    println!("{}: {text} -> {value} ({saved} bytes)", plan.path.display());
                }
            }
            println!("{fold_count} folds, {saved_bytes} bytes");
        }
        OutputFormat::Json => {
            let files = plans
                .iter()
                .map(|plan| {
                    json!({
                        "path": plan.path.display().to_string(),
                        "dialect": plan.dialect.label(),
                        "changed": plan.rewritten != plan.before,
                        "folds": plan.folds.iter().map(|(text, value, saved)| json!({
                            "text": text,
                            "value": value,
                            "saved_bytes": saved,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "fold_count": fold_count,
                    "saved_bytes": saved_bytes,
                    "files": files,
                }))?
            );
        }
    }
    Ok(())
}
