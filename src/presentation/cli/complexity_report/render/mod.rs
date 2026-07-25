use super::super::*;
use crate::application::usecase::complexity_report::{
    ComplexityReportFile, ComplexityReportPolicy,
};

mod json;
mod text;

/// One entry in the cross-file complexity leaderboard: a definition together
/// with the file it came from, so agents can jump straight to the worst
/// offenders without re-deriving file/definition pairing from separate lists.
pub(super) struct RankedComplexityEntry<'a> {
    pub(super) file: &'a std::path::Path,
    pub(super) dialect: Dialect,
    pub(super) item: &'a crate::application::usecase::complexity_report::ComplexityReportItem,
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

pub(in crate::presentation::cli) fn print_complexity_report(
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
