use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::redundant_body_progn::usecase::{RedundantBodyPrognPolicy, RedundantBodyPrognSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_redundant_body_progn_report(
    summary: &RedundantBodyPrognSummary,
    policy: &RedundantBodyPrognPolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "implicit_progn_form_count\t{}",
                summary.implicit_progn_form_count
            );
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\tparent={}\tbody_form_count={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.parent),
                    item.body_form_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "implicit_progn_form_count": summary.implicit_progn_form_count,
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
                            "parent": &item.parent,
                            "body_form_count": item.body_form_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
