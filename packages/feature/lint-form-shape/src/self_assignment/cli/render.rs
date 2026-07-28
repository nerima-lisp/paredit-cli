use anyhow::Result;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::self_assignment::usecase::{SelfAssignmentPolicy, SelfAssignmentSummary};
use paredit_core_cli::args::OutputFormat;

pub fn print_self_assignment_report(
    summary: &SelfAssignmentSummary,
    policy: &SelfAssignmentPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("assignment_form_count\t{}", summary.assignment_form_count);
            println!("self_assignment_count\t{}", summary.self_assignments.len());
            if policy.fail_on_self_assignment {
                println!(
                    "policy\tfail_on_self_assignment=true\tpassed={}",
                    policy.passed
                );
            }
            for item in &summary.self_assignments {
                println!(
                    "self-assignment\t{}\t{}\top={}\tplace={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.operator),
                    safe_text!(item.place),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "assignment_form_count": summary.assignment_form_count,
                    "self_assignment_count": summary.self_assignments.len(),
                    "policy": {
                        "fail_on_self_assignment": policy.fail_on_self_assignment,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "self_assignments": summary.self_assignments
                        .iter()
                        .map(|item| json!({
                            "path": item.path.display().to_string(),
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "operator": &item.operator,
                            "place": &item.place,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
