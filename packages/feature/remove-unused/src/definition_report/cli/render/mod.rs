use super::super::*;
use crate::application::usecase::definition_report::{
    DefinitionReportFile, UnusedDefinitionFile, UnusedDefinitionPolicy,
};

mod json;
mod text;

struct DefinitionReportSummary {
    definition_count: usize,
    by_category: BTreeMap<DefinitionCategory, usize>,
}

impl DefinitionReportSummary {
    fn from_reports(reports: &[DefinitionReportFile]) -> Self {
        let definition_count = reports
            .iter()
            .map(|report| report.definitions.len())
            .sum::<usize>();
        let mut by_category = BTreeMap::<DefinitionCategory, usize>::new();
        for definition in reports.iter().flat_map(|report| &report.definitions) {
            *by_category.entry(definition.category).or_default() += 1;
        }

        Self {
            definition_count,
            by_category,
        }
    }
}

fn unused_candidate_count(report: &UnusedDefinitionFile) -> usize {
    report
        .definitions
        .iter()
        .filter(|item| item.references.is_empty())
        .count()
}

pub(in crate::presentation::cli) fn print_definition_report(
    reports: &[DefinitionReportFile],
    output: OutputFormat,
) -> Result<()> {
    let summary = DefinitionReportSummary::from_reports(reports);
    match output {
        OutputFormat::Text => text::print_definition_report(reports, &summary),
        OutputFormat::Json => json::print_definition_report(reports, &summary)?,
    }

    Ok(())
}

pub(in crate::presentation::cli) fn print_unused_definition_report(
    reports: &[UnusedDefinitionFile],
    policy: &UnusedDefinitionPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => text::print_unused_definition_report(reports, policy),
        OutputFormat::Json => json::print_unused_definition_report(reports, policy)?,
    }

    Ok(())
}
