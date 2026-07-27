use anyhow::{Context, Result};

use crate::application::usecase::package as package_usecase;

use super::super::{read_input_and_dialect, write_file_with_rollback};
use super::{
    render::print_merge_package_options_plan,
    types::{MergePackageOptionsArgs, MergePackageOptionsPlan},
};

pub(in crate::presentation::cli) fn merge_package_options(
    args: MergePackageOptionsArgs,
) -> Result<()> {
    let (input, dialect) = read_input_and_dialect(Some(args.file.clone()), args.dialect)?;
    let usecase_plan =
        package_usecase::plan_merge_package_options(package_usecase::MergePackageOptionsRequest {
            input: &input.text,
            dialect,
            package: args.package.as_ref(),
        })
        .with_context(|| {
            format!(
                "failed to plan merge-package-options for {}",
                args.file.display()
            )
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
