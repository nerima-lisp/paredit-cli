use anyhow::Result;

use super::super::super::types::PackageReportFile;
use crate::presentation::cli::package::render::json::shared::span_json;
use serde_json::json;

pub(in crate::presentation::cli::package::render) fn print_package_report(
    reports: &[PackageReportFile],
) -> Result<()> {
    let defpackage_count = reports
        .iter()
        .map(|report| report.report.defpackages.len())
        .sum::<usize>();
    let in_package_count = reports
        .iter()
        .map(|report| report.report.in_packages.len())
        .sum::<usize>();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "defpackage_count": defpackage_count,
            "in_package_count": in_package_count,
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "defpackages": report
                        .report
                        .defpackages
                        .iter()
                        .map(|defpackage| json!({
                            "path": defpackage.path.as_str(),
                            "span": span_json(defpackage.span),
                            "name": defpackage.name.as_str(),
                            "nicknames": defpackage.nicknames.as_slice(),
                            "uses": defpackage.uses.as_slice(),
                            "exports": defpackage.exports.as_slice(),
                            "imports": defpackage
                                .imports
                                .iter()
                                .map(|import| json!({
                                    "package": import.package.as_str(),
                                    "symbols": import.symbols.as_slice(),
                                }))
                                .collect::<Vec<_>>(),
                            "option_count": defpackage.option_count,
                        }))
                        .collect::<Vec<_>>(),
                    "in_packages": report
                        .report
                        .in_packages
                        .iter()
                        .map(|in_package| json!({
                            "path": in_package.path.as_str(),
                            "span": span_json(in_package.span),
                            "name": in_package.name.as_str(),
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
