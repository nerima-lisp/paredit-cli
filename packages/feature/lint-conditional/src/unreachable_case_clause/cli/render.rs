use paredit_core_cli::CliResult;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::unreachable_case_clause::usecase::{
    UnreachableCaseClausePolicy, UnreachableCaseClauseSummary,
};
use paredit_core_cli::args::OutputFormat;

pub fn print_unreachable_case_clause_report(
    summary: &UnreachableCaseClauseSummary,
    policy: &UnreachableCaseClausePolicy,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("case_form_count\t{}", summary.case_form_count);
            println!("violation_count\t{}", summary.violations.len());
            if policy.fail_on_violation {
                println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
            }
            for item in &summary.violations {
                println!(
                    "violation\t{}\t{}\thead={}\tunreachable_count={}",
                    safe_text!(item.path.display()),
                    item.span.start().get(),
                    safe_text!(item.head),
                    item.unreachable_count,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "case_form_count": summary.case_form_count,
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
                            "head": &item.head,
                            "unreachable_count": item.unreachable_count,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}
