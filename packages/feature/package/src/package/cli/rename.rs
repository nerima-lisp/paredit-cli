use paredit_core_cli::CliResult;

use crate::error::PackageCommandError;

use crate::package::usecase as package_usecase;

use super::{
    render::print_rename_package_plan,
    types::{RenamePackageArgs, RenamePackageFilePlan},
};
use paredit_core_cli::shared::{read_input_and_dialect, write_file_with_rollback};

pub fn rename_package(args: RenamePackageArgs) -> CliResult<()> {
    let mut plans = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (input, dialect) = read_input_and_dialect(Some(file.clone()), args.dialect)?;
        let usecase_plan =
            package_usecase::plan_rename_package(package_usecase::RenamePackageRequest {
                input: &input.text,
                dialect,
                from: &args.from,
                to: &args.to,
            })
            .map_err(|source| PackageCommandError::Plan {
                operation: "rename-package",
                path: file.display().to_string(),
                source,
            })?;
        let changed = usecase_plan.changed;
        let written = args.write && changed;

        if written {
            write_file_with_rollback(file.clone(), usecase_plan.rewritten.clone())?;
        }

        plans.push(RenamePackageFilePlan {
            path: file.clone(),
            dialect,
            occurrences: usecase_plan.occurrences,
            changed,
            written,
        });
    }

    print_rename_package_plan(&plans, &args.from, &args.to, args.write, args.output)
}
