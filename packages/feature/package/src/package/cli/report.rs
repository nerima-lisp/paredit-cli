use anyhow::{Context, Result};
use paredit_core_cli::shared::read_input_dialect_and_tree;

use crate::package_report::usecase::build_package_report;

use super::{
    render::print_package_report,
    types::{PackageReportArgs, PackageReportFile},
};

pub fn package_report(args: PackageReportArgs) -> Result<()> {
    let mut reports = Vec::with_capacity(args.files.len());

    for file in &args.files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let report = build_package_report(&tree, dialect)
            .with_context(|| format!("failed to inspect packages in {}", file.display()))?;

        reports.push(PackageReportFile {
            path: file.clone(),
            dialect,
            report,
        });
    }

    print_package_report(&reports, args.output)
}
