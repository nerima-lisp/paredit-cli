use paredit_core_cli::CliResult;
use paredit_core_cli::args::MoveInsert;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Edit, SyntaxTree};

use super::args::MoveFormArgs;
use super::render::move_form::print_move_form_plan;
use super::shared::{insert_top_level_form, same_file_path, top_level_path_index};
use super::types::MoveFormPlan;
use paredit_core_cli::shared::{
    detect_dialect, list_head, package_context_before_top_level, read_file_or_empty,
    read_input_dialect_and_tree, write_files_with_rollback,
};

pub fn move_form(args: MoveFormArgs) -> CliResult<()> {
    let same_file = same_file_path(&args.from_file, &args.to_file);
    if same_file {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "--from-file and --to-file must refer to different files",
        )
        .into());
    }
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

    let (from_input, from_dialect, from_tree) =
        read_input_dialect_and_tree(Some(args.from_file.clone()), args.dialect)?;
    let (to_input, to_file_existed) = read_file_or_empty(&args.to_file)?;
    let to_dialect = detect_dialect(&to_input, args.dialect);
    if from_dialect != to_dialect {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "move-form requires source and destination to use the same dialect",
        )
        .into());
    }
    let to_tree = SyntaxTree::parse_with_dialect(&to_input.text, to_dialect).map_err(|source| {
        crate::error::DefinitionMovementError::DestinationNotAnSexprDocument {
            path: args.to_file.display().to_string(),
            source,
        }
    })?;

    let target_index = top_level_path_index(&args.path, "move-form")?;
    if target_index >= from_tree.root_children().len() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::SelectionPathNotReachable,
            format!("top-level path {} is out of range", args.path),
        )
        .into());
    }

    let selection = from_tree.select_path(&args.path)?;
    let view = selection.view();
    let span = selection.span();
    let head = list_head(&view).map(ToOwned::to_owned);
    if from_dialect == Dialect::CommonLisp
        && head.as_deref().is_some_and(|head| {
            paredit_core_syntax::common_lisp::common_lisp_operator_head_eq(head, "in-package")
        })
    {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
            "move-form cannot move a package declaration without changing surrounding package contexts",
        )
        .into());
    }
    let form_text = selection.text().to_owned();
    let from_rewritten = Edit::kill(&from_input.text, &from_tree, selection)?;

    let insertion_index = match args.insert {
        MoveInsert::Append => to_tree.root_children().len(),
        MoveInsert::Before | MoveInsert::After => {
            let index = top_level_path_index(args.anchor_path.as_ref().unwrap(), "move-form")?;
            // Resolve the anchor before reading its package context.
            to_tree.select_path(args.anchor_path.as_ref().unwrap())?;
            index + usize::from(args.insert == MoveInsert::After)
        }
    };
    let from_package = package_context_before_top_level(&from_tree, from_dialect, target_index)?;
    let to_package = package_context_before_top_level(&to_tree, to_dialect, insertion_index)?;
    let inserted_form = match from_package {
        Some(package)
            if to_dialect == Dialect::CommonLisp && to_package.as_ref() != Some(&package) =>
        {
            let mut inserted = format!("(in-package {package})\n\n{form_text}");
            if args.insert != MoveInsert::Append {
                let Some(previous_package) = to_package else {
                    return Err(paredit_core_cli::error::FeatureRefusal::message(
                        paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
                        "move-form cannot restore an unknown destination package after relative insertion",
                    )
                    .into());
                };
                inserted.push_str(&format!("\n\n(in-package {previous_package})"));
            }
            inserted
        }
        _ => form_text.clone(),
    };

    let (to_rewritten, anchor_span) = insert_top_level_form(
        &to_input.text,
        &to_tree,
        &inserted_form,
        args.insert,
        args.anchor_path.as_ref(),
        "move-form",
    )?;

    SyntaxTree::parse_with_dialect(&from_rewritten, from_dialect).map_err(|source| {
        crate::error::DefinitionMovementError::WouldBecomeInvalid {
            side: "source",
            action: "moving",
            what: "form",
            path: args.from_file.display().to_string(),
            source,
        }
    })?;
    SyntaxTree::parse_with_dialect(&to_rewritten, to_dialect).map_err(|source| {
        crate::error::DefinitionMovementError::WouldBecomeInvalid {
            side: "destination",
            action: "receiving",
            what: "form",
            path: args.to_file.display().to_string(),
            source,
        }
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
