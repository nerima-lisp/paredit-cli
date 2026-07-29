use std::path::PathBuf;

use super::super::*;
use super::args::AddIgnoreDeclarationArgs;
use crate::presentation::cli::shared::{
    expand_input_files, read_input_dialect_and_tree, unified_diff, write_files_with_rollback,
};
use paredit_feature_function_parameter::unused_parameter_report::domain::{
    IgnoreDeclarationPlan, plan_ignore_declarations,
};

/// Inserts `(declare (ignore ...))` for every parameter `inspect
/// unused-parameters` reports as unused.
///
/// The write side of that report, and the reason it needed one: the report
/// could name the problem and nothing in the tool could fix it, so acting on
/// it meant hand-editing every definition it listed.
pub(in crate::presentation::cli) fn add_ignore_declaration(
    args: AddIgnoreDeclarationArgs,
) -> CliResult<()> {
    let files = expand_input_files(&args.files, args.dialect)?;
    let mut plans = Vec::with_capacity(files.len());

    for file in &files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        plans.push((
            input.text.clone(),
            plan_ignore_declarations(file.clone(), dialect, &tree, &input.text)?,
        ));
    }

    if args.diff {
        for (before, plan) in &plans {
            if plan.rewritten != *before {
                print!("{}", unified_diff(&plan.path, before, &plan.rewritten));
            }
        }
    } else {
        print_plans(&plans, args.output)?;
    }

    if args.write {
        let written: Vec<(PathBuf, String)> = plans
            .iter()
            .filter(|(before, plan)| plan.rewritten != *before)
            .map(|(_, plan)| (plan.path.clone(), plan.rewritten.clone()))
            .collect();
        if !written.is_empty() {
            write_files_with_rollback(written)?;
        }
    }

    Ok(())
}

fn print_plans(plans: &[(String, IgnoreDeclarationPlan)], output: OutputFormat) -> CliResult<()> {
    let declaration_count: usize = plans.iter().map(|(_, plan)| plan.items.len()).sum();
    let parameter_count: usize = plans
        .iter()
        .flat_map(|(_, plan)| &plan.items)
        .map(|item| item.parameter_names.len())
        .sum();

    match output {
        OutputFormat::Text => {
            for (_, plan) in plans {
                for item in &plan.items {
                    println!(
                        "{}: {} {} -> (declare (ignore {}))",
                        plan.path.display(),
                        item.definition_path,
                        item.definition_name.as_deref().unwrap_or("<anonymous>"),
                        item.parameter_names.join(" ")
                    );
                }
            }
            println!("{declaration_count} declarations, {parameter_count} parameters");
        }
        OutputFormat::Json => {
            let files = plans
                .iter()
                .map(|(before, plan)| {
                    json!({
                        "path": plan.path.display().to_string(),
                        "dialect": plan.dialect.label(),
                        "changed": plan.rewritten != *before,
                        "declarations": plan.items.iter().map(|item| json!({
                            "definition_path": item.definition_path,
                            "definition_name": item.definition_name,
                            "parameter_names": item.parameter_names,
                            "insert_at": item.insert_at,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "declaration_count": declaration_count,
                    "parameter_count": parameter_count,
                    "files": files,
                }))?
            );
        }
    }
    Ok(())
}
