use paredit_core_cli::CliResult;

use super::args::RemoveUnusedDefinitionsArgs;
use super::render::print_remove_unused_definitions_plan;
use crate::definition_report::usecase::{DefinitionReportItem, collect_definition_forms};
use crate::remove_unused_definition::usecase::{
    RemoveUnusedDefinitionInputFile, RemoveUnusedDefinitionsRequest, UnusedDefinitionDefinition,
    plan_remove_unused_definitions,
};
use paredit_core_cli::shared::{
    analyze_files, note_partial_file_failures, total_file_failure, write_files_with_rollback,
};
use paredit_feature_package::error::PackageCommandError;
use paredit_feature_package::package_report::usecase::build_package_report;

pub fn remove_unused_definitions(args: RemoveUnusedDefinitionsArgs) -> CliResult<()> {
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, input| {
        let (package, definitions) = collect_definition_forms(tree, dialect)?;
        let package_report =
            build_package_report(tree, dialect).map_err(|source| PackageCommandError::Inspect {
                path: file.display().to_string(),
                source,
            })?;

        CliResult::Ok((
            RemoveUnusedDefinitionInputFile {
                path: file.to_path_buf(),
                dialect,
                package,
                definitions: definitions
                    .iter()
                    .map(to_unused_definition_definition)
                    .collect(),
                atoms: tree.atom_occurrences(),
                text: input.text.clone(),
                root_view: tree.root_view(),
            },
            package_report.defpackages,
        ))
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    let (input_files, package_definitions): (Vec<_>, Vec<_>) =
        analysis.succeeded.into_iter().unzip();

    let plan = plan_remove_unused_definitions(RemoveUnusedDefinitionsRequest {
        files: input_files,
        package_definitions: package_definitions.into_iter().flatten().collect(),
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
