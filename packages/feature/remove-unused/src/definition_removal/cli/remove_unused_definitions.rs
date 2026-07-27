use anyhow::{Context, Result};

use super::super::shared::{read_input_dialect_and_tree, write_files_with_rollback};
use super::args::RemoveUnusedDefinitionsArgs;
use super::render::print_remove_unused_definitions_plan;
use crate::application::usecase::definition_report::{
    DefinitionReportItem, collect_definition_forms,
};
use crate::application::usecase::package_report::build_package_report;
use crate::application::usecase::remove_unused_definition::{
    RemoveUnusedDefinitionInputFile, RemoveUnusedDefinitionsRequest, UnusedDefinitionDefinition,
    plan_remove_unused_definitions,
};

pub(in crate::presentation::cli) fn remove_unused_definitions(
    args: RemoveUnusedDefinitionsArgs,
) -> Result<()> {
    let mut input_files = Vec::with_capacity(args.files.len());
    let mut package_definitions = Vec::new();

    for file in &args.files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let (package, definitions) = collect_definition_forms(&tree, dialect)?;
        let package_report = build_package_report(&tree, dialect)
            .with_context(|| format!("failed to inspect packages in {}", file.display()))?;
        package_definitions.extend(package_report.defpackages);

        input_files.push(RemoveUnusedDefinitionInputFile {
            path: file.clone(),
            dialect,
            package,
            definitions: definitions
                .iter()
                .map(to_unused_definition_definition)
                .collect(),
            atoms: tree.atom_occurrences(),
            text: input.text,
        });
    }

    let plan = plan_remove_unused_definitions(RemoveUnusedDefinitionsRequest {
        files: input_files,
        package_definitions,
        include_protected: args.include_protected,
        include_exported: args.include_exported,
    })?;

    let written = args.write && plan.changed;
    if written {
        let mut written_files = Vec::new();
        for file in &plan.files {
            if file.changed {
                written_files.push((file.path.clone(), file.rewritten.clone()));
            }
        }
        write_files_with_rollback(written_files)?;
    }

    print_remove_unused_definitions_plan(&plan, written, args.output)
}

fn to_unused_definition_definition(
    definition: &DefinitionReportItem,
) -> UnusedDefinitionDefinition {
    UnusedDefinitionDefinition {
        path: definition.path.clone(),
        span: definition.span,
        head: definition.head.clone(),
        name: definition.name.clone(),
        category: definition.category,
        parameter_count: definition.parameter_count,
        body_form_count: definition.body_form_count,
        package: definition.package.clone(),
    }
}
