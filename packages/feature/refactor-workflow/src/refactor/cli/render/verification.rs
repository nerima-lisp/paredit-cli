use super::super::super::*;
use super::super::types::verification::RefactorVerification;

pub(in crate::presentation::cli) fn print_refactor_verification(
    verification: &RefactorVerification,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("operation\t{}", verification.operation.label());
            println!("phase\t{}", verification.phase.label());
            println!("symbol\t{}", safe_text!(verification.symbol));
            println!(
                "new_symbol\t{}",
                safe_text!(verification.new_symbol.as_deref().unwrap_or("<none>"))
            );
            println!("passed\t{}", verification.passed);
            println!("target_kind\t{}", verification.target_kind.label());
            println!(
                "before_safe_to_automate\t{}",
                verification.before.safe_to_automate
            );
            println!("before_files\t{}", verification.before.file_count);
            println!(
                "before_definition_count\t{}",
                verification.before.definition_count
            );
            println!(
                "before_reference_count\t{}",
                verification.before.reference_count
            );
            println!("before_call_count\t{}", verification.before.call_count);
            println!(
                "before_signature_mismatch_count\t{}",
                verification.before.signature_mismatch_count
            );

            if let Some(after) = verification.after {
                println!("after_safe_to_automate\t{}", after.safe_to_automate);
                println!("after_files\t{}", after.file_count);
                println!("after_definition_count\t{}", after.definition_count);
                println!("after_reference_count\t{}", after.reference_count);
                println!("after_call_count\t{}", after.call_count);
                println!(
                    "after_signature_mismatch_count\t{}",
                    after.signature_mismatch_count
                );
            }

            for check in &verification.checks {
                println!(
                    "check\t{}\t{}\tpassed={}\tcount={}\t{}",
                    check.level.label(),
                    safe_text!(check.code),
                    check.passed,
                    check.count,
                    safe_text!(check.message)
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "operation": verification.operation.label(),
                "phase": verification.phase.label(),
                "symbol": verification.symbol.as_str(),
                "new_symbol": verification.new_symbol.as_deref(),
                "passed": verification.passed,
                "target_kind": verification.target_kind.label(),
                "before": refactor_summary_json(verification.before),
                "after": verification.after.map(refactor_summary_json),
                "checks": verification
                    .checks
                    .iter()
                    .map(|check| json!({
                        "code": check.code,
                        "level": check.level.label(),
                        "passed": check.passed,
                        "message": check.message.as_str(),
                        "count": check.count,
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

fn refactor_summary_json(summary: RefactorPlanSummary) -> serde_json::Value {
    json!({
        "safe_to_automate": summary.safe_to_automate,
        "file_count": summary.file_count,
        "definition_count": summary.definition_count,
        "reference_count": summary.reference_count,
        "call_count": summary.call_count,
        "inbound_edge_count": summary.inbound_edge_count,
        "outbound_edge_count": summary.outbound_edge_count,
        "non_call_reference_count": summary.non_call_reference_count,
        "signature_mismatch_count": summary.signature_mismatch_count,
    })
}
