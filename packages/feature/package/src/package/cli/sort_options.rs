use anyhow::{Context, Result};

use crate::application::usecase::package as package_usecase;

use super::super::{read_input_and_dialect, write_file_with_rollback};
use super::{
    render::print_sort_package_options_plan,
    types::{SortPackageOptionsArgs, SortPackageOptionsPlan},
};

pub(in crate::presentation::cli) fn sort_package_options(
    args: SortPackageOptionsArgs,
) -> Result<()> {
    let (input, dialect) = read_input_and_dialect(Some(args.file.clone()), args.dialect)?;
    let usecase_plan =
        package_usecase::plan_sort_package_options(package_usecase::SortPackageOptionsRequest {
            input: &input.text,
            dialect,
            package: args.package.as_ref(),
            order: args.order.into(),
        })
        .with_context(|| {
            format!(
                "failed to plan sort-package-options for {}",
                args.file.display()
            )
        })?;
    let changed = usecase_plan.changed;
    let written = args.write && changed;

    if written {
        write_file_with_rollback(args.file.clone(), usecase_plan.rewritten.clone())?;
    }

    let plan = SortPackageOptionsPlan {
        path: args.file,
        dialect,
        packages: usecase_plan.packages,
        changed,
        written,
        rewritten: usecase_plan.rewritten,
    };

    print_sort_package_options_plan(&plan, args.output)
}
