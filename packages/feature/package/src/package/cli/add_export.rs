use paredit_core_cli::CliResult;

use crate::error::PackageCommandError;

use crate::package::usecase as package_usecase;

use super::{
    render::print_add_export_plan,
    types::{AddExportArgs, AddExportPlan},
};
use paredit_core_cli::shared::{read_input_and_dialect, write_file_with_rollback};

pub fn add_export(args: AddExportArgs) -> CliResult<()> {
    let (input, dialect) = read_input_and_dialect(Some(args.file.clone()), args.dialect)?;
    let usecase_plan = package_usecase::plan_add_export(package_usecase::AddExportRequest {
        input: &input.text,
        dialect,
        package: args.package.as_ref(),
        symbol: &args.symbol,
    })
    .map_err(|source| PackageCommandError::Plan {
        operation: "add-export",
        path: args.file.display().to_string(),
        source,
    })?;
    let changed = usecase_plan.changed;
    let written = args.write && changed;

    if written {
        write_file_with_rollback(args.file.clone(), usecase_plan.rewritten.clone())?;
    }

    let plan = AddExportPlan {
        path: args.file,
        dialect,
        package: usecase_plan.package,
        symbol: usecase_plan.symbol,
        defpackage_path: usecase_plan.defpackage_path,
        defpackage_span: usecase_plan.defpackage_span,
        export_span: usecase_plan.export_span,
        insertion_span: usecase_plan.insertion_span,
        already_exported: usecase_plan.already_exported,
        changed,
        written,
        rewritten: usecase_plan.rewritten,
    };

    print_add_export_plan(&plan, args.output)
}
