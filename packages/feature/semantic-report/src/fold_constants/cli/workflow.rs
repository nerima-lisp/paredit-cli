use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::shared::{
    apply_byte_span_edits, expand_input_files, read_input_dialect_and_tree, unified_diff,
    write_files_with_rollback,
};
use paredit_core_syntax::sexpr::ByteSpan;

use crate::constant_report::domain::build_constant_report;
use crate::fold_constants::cli::args::FoldConstantsArgs;
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
/// Quoted forms are safe by construction rather than by a guard here: the
/// value layer refuses to evaluate through `'` and `` ` ``, so `'(+ 1 2)`
/// never reaches this as a foldable span. Nested folds are likewise already
/// excluded by the report, which yields only the outermost form of each
/// folded region — so the spans are non-overlapping and can be applied in one
/// pass.
pub fn fold_constants(args: FoldConstantsArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;
    let mut plans = Vec::with_capacity(files.len());

    for file in &files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let report = build_constant_report(&SemanticFile::analyze(file, dialect, tree));

        let mut edits: Vec<(ByteSpan, String)> = Vec::new();
        let mut folds = Vec::new();
        for foldable in &report.foldable {
            if foldable.saved_bytes < args.min_saved_bytes {
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
        plans.push(FoldPlan {
            path: file.clone(),
            dialect,
            before: input.text.clone(),
            rewritten,
            folds,
        });
    }

    if args.diff {
        for plan in &plans {
            if plan.rewritten != plan.before {
                print!(
                    "{}",
                    unified_diff(&plan.path, &plan.before, &plan.rewritten)
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

fn print_plans(plans: &[FoldPlan], output: OutputFormat) -> Result<()> {
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
