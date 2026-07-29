use paredit_core_cli::CliResult;

use crate::error::PackageCommandError;
use paredit_core_cli::shared::read_input_dialect_and_tree;

use crate::package_report::usecase::build_package_report;

use super::{
    render::print_package_report,
    types::{PackageReportArgs, PackageReportFile},
};

pub fn package_report(args: PackageReportArgs) -> CliResult<()> {
    let mut reports = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let report = build_package_report(&tree, dialect).map_err(|source| {
            PackageCommandError::Inspect {
                path: file.display().to_string(),
                source,
            }
        })?;

        reports.push(PackageReportFile {
            path: file.clone(),
            dialect,
            report,
        });
    }

    print_package_report(&reports, args.output)
}
