use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use super::args::InsertTopLevelArgs;
use super::shared::insert_top_level_form;
use paredit_core_cli::args::{MoveInsert, OutputFormat};
use paredit_core_cli::shared::{read_input_dialect_and_tree, write_file_with_rollback};

pub fn insert_top_level(args: InsertTopLevelArgs) -> CliResult<()> {
    if args.insert == MoveInsert::Append && args.anchor_path.is_some() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "--anchor-path is only valid with --insert before or --insert after",
        )
        .into());
    }
    if matches!(args.insert, MoveInsert::Before | MoveInsert::After) && args.anchor_path.is_none() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "--insert before/after requires --anchor-path",
        )
        .into());
    }

    let dialect = Dialect::detect(Some(&args.file), args.dialect.map(Into::into));
    let replacement_tree =
        SyntaxTree::parse_with_dialect(&args.with, dialect).map_err(|source| {
            crate::error::DefinitionMovementError::WithArgument {
                summary: "--with must contain a valid, complete top-level S-expression",
                source,
            }
        })?;
    if replacement_tree.root_children().len() != 1 {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "--with must contain exactly one top-level S-expression",
        )
        .into());
    }

    let (input, dialect, tree) =
        read_input_dialect_and_tree(Some(args.file.clone()), args.dialect)?;
    let (rewritten, anchor_span) = insert_top_level_form(
        &input.text,
        &tree,
        &args.with,
        args.insert,
        args.anchor_path.as_ref(),
        "insert-top-level",
    )?;

    SyntaxTree::parse_with_dialect(&rewritten, dialect)
        .map_err(|source| crate::error::DefinitionMovementError::InsertionInvalid { source })?;

    let changed = input.text != rewritten;
    let written = args.write && changed;
    if written {
        write_file_with_rollback(args.file.clone(), rewritten.clone())?;
    }

    match args.output {
        OutputFormat::Text => println!(
            "file={} dialect={} insert={:?} anchor_path={:?} changed={} written={}",
            safe_text!(args.file.display()),
            dialect.label(),
            args.insert,
            args.anchor_path,
            changed,
            written,
        ),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string(&json!({
                "schema_version": 1,
                "file": args.file.display().to_string(),
                "dialect": dialect.label(),
                "insert": args.insert.label(),
                "anchor_path": args.anchor_path.as_ref().map(ToString::to_string),
                "anchor_span": anchor_span.map(|span| json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                })),
                "text": args.with,
                "rewritten": rewritten,
                "changed": changed,
                "written": written,
            }))?
        ),
    }

    Ok(())
}
