use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::negated_comparison::usecase::{NegatedComparisonPolicy, NegatedComparisonSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_negated_comparison_report(
    summary: &NegatedComparisonSummary,
    policy: &NegatedComparisonPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("negation_form_count\t{}", summary.negation_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tcomplement={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.complement),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "negation_form_count": summary.negation_form_count,
                    "violation_count": summary.violations.len(),
                    "policy": {
                        "fail_on_violation": policy.fail_on_violation,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "violations": summary.violations
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "complement": item.complement,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
