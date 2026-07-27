use anyhow::Result;

use crate::domain::sexpr::SymbolName;
use crate::presentation::cli::args::OutputFormat;

use super::types::{
    AddExportPlan, MergePackageOptionsPlan, PackageReportFile, RenamePackageFilePlan,
    SortPackageExportsPlan, SortPackageOptionsPlan,
};

mod json;
mod text;

pub(super) fn print_merge_package_options_plan(
    plan: &MergePackageOptionsPlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_merge_package_options_plan(plan),
        OutputFormat::Json => json::refactor::print_merge_package_options_plan(plan),
    }
}

pub(super) fn print_sort_package_options_plan(
    plan: &SortPackageOptionsPlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_sort_package_options_plan(plan),
        OutputFormat::Json => json::refactor::print_sort_package_options_plan(plan),
    }
}

pub(super) fn print_sort_package_exports_plan(
    plan: &SortPackageExportsPlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_sort_package_exports_plan(plan),
        OutputFormat::Json => json::refactor::print_sort_package_exports_plan(plan),
    }
}

pub(super) fn print_package_report(
    reports: &[PackageReportFile],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_package_report(reports),
        OutputFormat::Json => json::report::print_package_report(reports),
    }
}

pub(super) fn print_rename_package_plan(
    plans: &[RenamePackageFilePlan],
    from: &SymbolName,
    to: &SymbolName,
    write: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_rename_package_plan(plans, from, to, write),
        OutputFormat::Json => json::refactor::print_rename_package_plan(plans, from, to, write),
    }
}

pub(super) fn print_add_export_plan(plan: &AddExportPlan, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_add_export_plan(plan),
        OutputFormat::Json => json::refactor::print_add_export_plan(plan),
    }
}
