use anyhow::{Context, Result};

use crate::application::usecase::package as package_usecase;

use super::super::{read_input_and_dialect, write_file_with_rollback};
use super::{
    render::print_rename_package_plan,
    types::{RenamePackageArgs, RenamePackageFilePlan},
};

pub(in crate::presentation::cli) fn rename_package(args: RenamePackageArgs) -> Result<()> {
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
            .with_context(|| format!("failed to plan rename-package for {}", file.display()))?;
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
