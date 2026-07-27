use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SyntaxTree;

use super::super::shared::{read_input_dialect_and_tree, write_file_with_rollback};
use super::super::{MoveInsert, OutputFormat};
use super::args::InsertTopLevelArgs;
use super::shared::insert_top_level_form;

pub(in crate::presentation::cli) fn insert_top_level(args: InsertTopLevelArgs) -> Result<()> {
    if args.insert == MoveInsert::Append && args.anchor_path.is_some() {
        bail!("--anchor-path is only valid with --insert before or --insert after");
    }
    if matches!(args.insert, MoveInsert::Before | MoveInsert::After) && args.anchor_path.is_none() {
        bail!("--insert before/after requires --anchor-path");
    }

    let dialect = Dialect::detect(Some(&args.file), args.dialect.map(Into::into));
    let replacement_tree = SyntaxTree::parse_with_dialect(&args.with, dialect)
        .context("--with must contain a valid, complete top-level S-expression")?;
    if replacement_tree.root_children().len() != 1 {
        bail!("--with must contain exactly one top-level S-expression");
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
        .context("insertion produced invalid Lisp syntax")?;

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
