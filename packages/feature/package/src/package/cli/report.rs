use paredit_core_cli::CliResult;

use crate::error::PackageCommandError;
use paredit_core_cli::shared::{analyze_files, note_partial_file_failures, total_file_failure};

use crate::package_report::usecase::build_package_report;

use super::{
    render::print_package_report,
    types::{PackageReportArgs, PackageReportFile},
};

pub fn package_report(args: PackageReportArgs) -> CliResult<()> {
    let analysis = analyze_files(&args.files, args.dialect, |file, dialect, tree, _| {
        let report =
            build_package_report(tree, dialect).map_err(|source| PackageCommandError::Inspect {
                path: file.display().to_string(),
                source,
            })?;

        CliResult::Ok(PackageReportFile {
            path: file.clone(),
            dialect,
            report,
        })
    });
    if analysis.is_total_failure() {
        return Err(total_file_failure(analysis.failed).into());
    }
    note_partial_file_failures(&analysis.failed);

    print_package_report(&analysis.succeeded, args.output)
}
