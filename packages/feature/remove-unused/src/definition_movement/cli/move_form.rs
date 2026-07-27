use anyhow::{Context, Result};

use crate::domain::sexpr::{Edit, SyntaxTree};

use super::super::MoveInsert;
use super::super::shared::{
    detect_dialect, list_head, read_file_or_empty, read_input_dialect_and_tree,
    write_files_with_rollback,
};
use super::args::MoveFormArgs;
use super::render::move_form::print_move_form_plan;
use super::shared::{insert_top_level_form, same_file_path, top_level_path_index};
use super::types::MoveFormPlan;

pub(in crate::presentation::cli) fn move_form(args: MoveFormArgs) -> Result<()> {
    let same_file = same_file_path(&args.from_file, &args.to_file);
    if same_file {
        anyhow::bail!("--from-file and --to-file must refer to different files");
    }
    if args.insert == MoveInsert::Append && args.anchor_path.is_some() {
        anyhow::bail!("--anchor-path is only valid with --insert before or --insert after");
    }
    if matches!(args.insert, MoveInsert::Before | MoveInsert::After) && args.anchor_path.is_none() {
        anyhow::bail!("--insert before/after requires --anchor-path");
    }

    let (from_input, from_dialect, from_tree) =
        read_input_dialect_and_tree(Some(args.from_file.clone()), args.dialect)?;
    let (to_input, to_file_existed) = read_file_or_empty(&args.to_file)?;
    let to_dialect = detect_dialect(&to_input, args.dialect);
    let to_tree =
        SyntaxTree::parse_with_dialect(&to_input.text, to_dialect).with_context(|| {
            format!(
                "destination file is not a valid S-expression document: {}",
                args.to_file.display()
            )
        })?;

    let target_index = top_level_path_index(&args.path, "move-form")?;
    if target_index >= from_tree.root_children().len() {
        anyhow::bail!("top-level path {} is out of range", args.path);
    }

    let selection = from_tree.select_path(&args.path)?;
    let view = selection.view();
    let span = selection.span();
    let head = list_head(&view).map(ToOwned::to_owned);
    let form_text = selection.text().to_owned();
    let from_rewritten = Edit::kill(&from_input.text, &from_tree, selection)?;

    let (to_rewritten, anchor_span) = insert_top_level_form(
        &to_input.text,
        &to_tree,
        &form_text,
        args.insert,
        args.anchor_path.as_ref(),
        "move-form",
    )?;

    SyntaxTree::parse_with_dialect(&from_rewritten, from_dialect).with_context(|| {
        format!(
            "source file would become invalid after moving form: {}",
            args.from_file.display()
        )
    })?;
    SyntaxTree::parse_with_dialect(&to_rewritten, to_dialect).with_context(|| {
        format!(
            "destination file would become invalid after receiving form: {}",
            args.to_file.display()
        )
    })?;

    let changed = from_rewritten != from_input.text || to_rewritten != to_input.text;
    let written = args.write && changed;
    if written {
        write_files_with_rollback([
            (args.from_file.clone(), from_rewritten.clone()),
            (args.to_file.clone(), to_rewritten.clone()),
        ])?;
    }

    let plan = MoveFormPlan {
        from_file: args.from_file,
        to_file: args.to_file,
        from_dialect,
        to_dialect,
        path: args.path,
        span,
        head,
        form_text,
        insert: args.insert,
        anchor_path: args.anchor_path,
        anchor_span,
        from_rewritten,
        to_rewritten,
        to_file_existed,
        changed,
        written,
    };
    print_move_form_plan(&plan, args.output)
}
