use paredit_core_cli::CliResult;

use crate::error::PackageCommandError;

use crate::package::usecase as package_usecase;

use super::{
    render::print_merge_package_options_plan,
    types::{MergePackageOptionsArgs, MergePackageOptionsPlan},
};
use paredit_core_cli::shared::{read_input_and_dialect, write_file_with_rollback};

pub fn merge_package_options(args: MergePackageOptionsArgs) -> CliResult<()> {
    let (input, dialect) = read_input_and_dialect(Some(args.file.clone()), args.dialect)?;
    let usecase_plan =
        package_usecase::plan_merge_package_options(package_usecase::MergePackageOptionsRequest {
            input: &input.text,
            dialect,
            package: args.package.as_ref(),
        })
        .map_err(|source| PackageCommandError::Plan {
            operation: "merge-package-options",
            path: args.file.display().to_string(),
            source,
        })?;
    let changed = usecase_plan.changed;
    let written = args.write && changed;

    if written {
        write_file_with_rollback(args.file.clone(), usecase_plan.rewritten.clone())?;
    }

    let plan = MergePackageOptionsPlan {
        path: args.file,
        dialect,
        merges: usecase_plan.merges,
        changed,
        written,
        rewritten: usecase_plan.rewritten,
    };

    print_merge_package_options_plan(&plan, args.output)
}
