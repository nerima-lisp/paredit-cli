use std::collections::BTreeMap;

use crate::signature_report::domain::SignatureCallStatus;
use paredit_core_edit::refactor_plan::{RawRefactorRisk, RefactorPlanSummary, RefactorRiskLevel};

use super::{ImpactReportFile, ImpactRisk, ImpactRiskLevel};

#[must_use]
pub fn impact_status_counts(reports: &[ImpactReportFile]) -> BTreeMap<SignatureCallStatus, usize> {
    let mut by_status = BTreeMap::new();
    for report in reports {
        for call in &report.calls {
            *by_status.entry(call.status).or_insert(0) += 1;
        }
    }
    by_status
}

#[must_use]
pub fn summarize_impact_reports(files: &[ImpactReportFile]) -> RefactorPlanSummary {
    RefactorPlanSummary {
        file_count: files.len(),
        definition_count: files
            .iter()
            .map(|file| file.definitions.len())
            .sum::<usize>(),
        reference_count: files
            .iter()
            .map(|file| file.references.len())
            .sum::<usize>(),
        call_count: files.iter().map(|file| file.calls.len()).sum::<usize>(),
        inbound_edge_count: files
            .iter()
            .map(|file| file.inbound_edges.len())
            .sum::<usize>(),
        outbound_edge_count: files
            .iter()
            .map(|file| file.outbound_edges.len())
            .sum::<usize>(),
        non_call_reference_count: files
            .iter()
            .map(|file| file.non_call_reference_count)
            .sum::<usize>(),
        signature_mismatch_count: files
            .iter()
            .flat_map(|file| &file.calls)
            .filter(|call| call.status != SignatureCallStatus::Exact)
            .count(),
        safe_to_automate: false,
    }
}

#[must_use]
pub fn raw_refactor_risks(summary: &RefactorPlanSummary) -> Vec<RawRefactorRisk> {
    let mut by_status = BTreeMap::new();
    if summary.signature_mismatch_count > 0 {
        by_status.insert(
            SignatureCallStatus::UnknownDefinition,
            summary.signature_mismatch_count,
        );
    }

    impact_risks(
        summary.definition_count,
        summary.inbound_edge_count,
        summary.non_call_reference_count,
        &by_status,
    )
    .into_iter()
    .map(|risk| RawRefactorRisk {
        level: RefactorRiskLevel::from(risk.level),
        code: risk.code,
        message: risk.message,
        count: risk.count,
    })
    .collect()
}

#[must_use]
pub fn impact_risks(
    definition_count: usize,
    inbound_edge_count: usize,
    non_call_reference_count: usize,
    by_status: &BTreeMap<SignatureCallStatus, usize>,
) -> Vec<ImpactRisk> {
    let mut risks = Vec::new();

    if definition_count == 0 {
        risks.push(ImpactRisk {
            level: ImpactRiskLevel::Error,
            code: "no-definition",
            message: "no matching definition found".to_owned(),
            count: 0,
        });
    } else if definition_count > 1 {
        risks.push(ImpactRisk {
            level: ImpactRiskLevel::Warning,
            code: "ambiguous-definition",
            message: format!("{definition_count} matching definitions found"),
            count: definition_count,
        });
    }

    if inbound_edge_count > 0 {
        risks.push(ImpactRisk {
            level: ImpactRiskLevel::Warning,
            code: "inbound-callers",
            message: format!("{inbound_edge_count} inbound call edge(s) reference the symbol"),
            count: inbound_edge_count,
        });
    }

    if non_call_reference_count > 0 {
        risks.push(ImpactRisk {
            level: ImpactRiskLevel::Warning,
            code: "non-call-references",
            message: format!(
                "{non_call_reference_count} non-call reference(s) may need manual review"
            ),
            count: non_call_reference_count,
        });
    }

    let signature_mismatches = by_status
        .iter()
        .filter(|(status, _)| **status != SignatureCallStatus::Exact)
        .map(|(_, count)| *count)
        .sum::<usize>();
    if signature_mismatches > 0 {
        risks.push(ImpactRisk {
            level: ImpactRiskLevel::Warning,
            code: "signature-mismatch",
            message: format!("{signature_mismatches} call(s) do not match the known signature"),
            count: signature_mismatches,
        });
    }

    risks
}
