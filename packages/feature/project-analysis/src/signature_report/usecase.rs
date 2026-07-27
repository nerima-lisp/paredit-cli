//! Compatibility facade for the signature report domain service.

#[cfg(test)]
mod tests;

pub use crate::domain::signature_report::{
    SignatureCallItem, SignatureCallStatus, SignatureDefinitionItem, SignatureReportFile,
    SignatureReportPolicy, SignatureReportSource, build_signature_reports, classify_signature_call,
};

#[must_use]
pub fn evaluate_signature_report_policy(
    reports: &[SignatureReportFile],
    fail_on_mismatch: bool,
    require_definitions: Option<usize>,
    require_calls: Option<usize>,
) -> SignatureReportPolicy {
    let definition_count = reports
        .iter()
        .map(|report| report.definitions.len())
        .sum::<usize>();
    let statuses = reports
        .iter()
        .flat_map(|report| &report.calls)
        .map(|item| item.status)
        .collect::<Vec<SignatureCallStatus>>();

    crate::domain::signature_report::evaluate_signature_report_policy(
        definition_count,
        &statuses,
        fail_on_mismatch,
        require_definitions,
        require_calls,
    )
}
