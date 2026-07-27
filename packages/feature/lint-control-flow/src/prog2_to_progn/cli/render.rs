use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::prog2_to_progn::usecase::{Prog2ToPrognPolicy, Prog2ToPrognSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_prog2_to_progn_report(
    summary: &Prog2ToPrognSummary,
    policy: &Prog2ToPrognPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("prog2_form_count\t{}", summary.prog2_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "prog2_form_count": summary.prog2_form_count,
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
                            "head_span": {
                                "start": item.head_span.start().get(),
                                "end": item.head_span.end().get(),
                            },
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
