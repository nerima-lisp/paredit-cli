use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::value_propagation_report::usecase::{
    ValuePropagationPolicy, ValuePropagationReportFile,
};

pub fn print_value_propagation_report(
    reports: &[ValuePropagationReportFile],
    policy: &ValuePropagationPolicy,
    blocked_only: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => print_text(reports, policy, blocked_only),
        OutputFormat::Json => print_json(reports, policy, blocked_only)?,
    }
    Ok(())
}

fn print_text(
    reports: &[ValuePropagationReportFile],
    policy: &ValuePropagationPolicy,
    blocked_only: bool,
) {
    println!("files\t{}", reports.len());
    println!("propagated_count\t{}", policy.propagated_count);
    println!("blocked_count\t{}", policy.blocked_count);
    println!("coverage\t{:.3}", policy.coverage);
    for (reason, count) in &policy.blocked_by_reason {
        println!("blocked_by\t{}\t{count}", reason.label());
    }
    if let Some(min) = policy.min_coverage {
        println!("policy\tmin_coverage={min:.3}\tpassed={}", policy.passed);
    }

    for report in reports {
        if !report.dialect_modelled {
            println!(
                "unmodelled\t{}\t{}\tthe value layer models Common Lisp only",
                safe_text!(report.path.display()),
                report.dialect.label(),
            );
            continue;
        }
        if !blocked_only {
            for binding in &report.propagated {
                println!(
                    "propagated\t{}\t{}\t{}\t{}\treferences={}",
                    safe_text!(report.path.display()),
                    binding.line,
                    safe_text!(binding.name),
                    safe_text!(binding.value),
                    binding.reference_count,
                );
            }
        }
        for binding in &report.blocked {
            println!(
                "blocked\t{}\t{}\t{}\treason={}\tcause={}",
                safe_text!(report.path.display()),
                binding.line,
                safe_text!(binding.name),
                binding.reason.label(),
                binding.opacity_cause.unwrap_or("-"),
            );
        }
    }
}

fn print_json(
    reports: &[ValuePropagationReportFile],
    policy: &ValuePropagationPolicy,
    blocked_only: bool,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "propagated_count": policy.propagated_count,
            "blocked_count": policy.blocked_count,
            "coverage": policy.coverage,
            "blocked_by_reason": policy
                .blocked_by_reason
                .iter()
                .map(|(reason, count)| json!({
                    "reason": reason.label(),
                    "count": count,
                }))
                .collect::<Vec<_>>(),
            "policy": {
                "min_coverage": policy.min_coverage,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "dialect_modelled": report.dialect_modelled,
                    "coverage": report.coverage(),
                    "propagated": if blocked_only {
                        Vec::new()
                    } else {
                        report
                            .propagated
                            .iter()
                            .map(|binding| json!({
                                "name": binding.name,
                                "value": binding.value,
                                "binder": binding.binder,
                                "line": binding.line,
                                "span": {
                                    "start": binding.span.start().get(),
                                    "end": binding.span.end().get(),
                                },
                                "reference_count": binding.reference_count,
                            }))
                            .collect::<Vec<_>>()
                    },
                    "blocked": report
                        .blocked
                        .iter()
                        .map(|binding| json!({
                            "name": binding.name,
                            "reason": binding.reason.label(),
                            "binder": binding.binder,
                            "line": binding.line,
                            "span": {
                                "start": binding.span.start().get(),
                                "end": binding.span.end().get(),
                            },
                            "opacity_cause": binding.opacity_cause,
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
