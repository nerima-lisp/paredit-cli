use crate::complexity_report::usecase::{ComplexityReportFile, ComplexityReportPolicy};
use anyhow::Result;
use paredit_core_cli::args::OutputFormat;
use paredit_core_syntax::dialect::Dialect;

mod json;
mod text;

/// One entry in the cross-file complexity leaderboard: a definition together
/// with the file it came from, so agents can jump straight to the worst
/// offenders without re-deriving file/definition pairing from separate lists.
// Public since the extraction: crate-internal visibility cannot cross a
// crate boundary, so this lint applies for the first time.
#[derive(Debug)]
pub struct RankedComplexityEntry<'a> {
    pub file: &'a std::path::Path,
    pub dialect: Dialect,
    pub item: &'a crate::complexity_report::usecase::ComplexityReportItem,
}

fn ranked_entries(
    reports: &[ComplexityReportFile],
    top: Option<usize>,
) -> Vec<RankedComplexityEntry<'_>> {
    let mut entries = reports
        .iter()
        .flat_map(|report| {
            report
                .definitions
                .iter()
                .map(move |item| RankedComplexityEntry {
                    file: report.path.as_path(),
                    dialect: report.dialect,
                    item,
                })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        right
            .item
            .complexity_score
            .cmp(&left.item.complexity_score)
            .then_with(|| left.file.cmp(right.file))
            .then_with(|| left.item.path.cmp(&right.item.path))
    });

    if let Some(top) = top {
        entries.truncate(top);
    }

    entries
}

pub fn print_complexity_report(
    reports: &[ComplexityReportFile],
    policy: &ComplexityReportPolicy,
    top: Option<usize>,
    output: OutputFormat,
) -> Result<()> {
    let ranked = ranked_entries(reports, top);
    match output {
        OutputFormat::Text => text::print_complexity_report(reports, policy, &ranked),
        OutputFormat::Json => json::print_complexity_report(reports, policy, &ranked)?,
    }

    Ok(())
}
