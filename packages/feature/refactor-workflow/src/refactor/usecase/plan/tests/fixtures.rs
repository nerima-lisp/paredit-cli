use super::*;

pub(super) fn summary() -> RefactorPlanSummary {
    RefactorPlanSummary {
        file_count: 2,
        definition_count: 1,
        reference_count: 3,
        call_count: 2,
        inbound_edge_count: 0,
        outbound_edge_count: 0,
        non_call_reference_count: 0,
        signature_mismatch_count: 0,
        safe_to_automate: true,
    }
}

pub(super) fn gates(
    blocking_gate_count: usize,
    nonblocking_gate_count: usize,
) -> Vec<RefactorPlanGate> {
    let mut gates = Vec::new();
    for index in 0..blocking_gate_count {
        gates.push(RefactorPlanGate {
            level: RefactorRiskLevel::Error,
            code: "blocking-risk",
            message: format!("blocking risk {index}"),
            count: index + 1,
            blocks_automation: true,
        });
    }
    for index in 0..nonblocking_gate_count {
        gates.push(RefactorPlanGate {
            level: RefactorRiskLevel::Warning,
            code: "advisory-risk",
            message: format!("advisory risk {index}"),
            count: index + 1,
            blocks_automation: false,
        });
    }
    gates
}
