use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::redundant_let_star::usecase::{RedundantLetStarPolicy, RedundantLetStarSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_redundant_let_star_report(
    summary: &RedundantLetStarSummary,
    policy: &RedundantLetStarPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("let_star_form_count\t{}", summary.let_star_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tbindings={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    item.binding_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "let_star_form_count": summary.let_star_form_count,
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
                            "binding_count": item.binding_count,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
