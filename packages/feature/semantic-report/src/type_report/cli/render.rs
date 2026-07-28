use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::type_report::domain::type_label;
use crate::type_report::usecase::{TypeReportFile, TypeReportPolicy};

pub fn print_type_report(
    reports: &[TypeReportFile],
    policy: &TypeReportPolicy,
    contradictions_only: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => print_text(reports, policy, contradictions_only),
        OutputFormat::Json => print_json(reports, policy, contradictions_only)?,
    }
    Ok(())
}

fn print_text(reports: &[TypeReportFile], policy: &TypeReportPolicy, contradictions_only: bool) {
    println!("files\t{}", reports.len());
    println!("typed_binding_count\t{}", policy.typed_binding_count);
    println!("typed_expression_count\t{}", policy.typed_expression_count);
    println!("contradiction_count\t{}", policy.contradiction_count);
    if policy.fail_on_contradiction {
        println!(
            "policy\tfail_on_contradiction=true\tpassed={}",
            policy.passed
        );
    }

    for report in reports {
        if !report.dialect_modelled {
            println!(
                "unmodelled\t{}\t{}\tthe semantic layer models Common Lisp only",
                safe_text!(report.path.display()),
                report.dialect.label(),
            );
            continue;
        }

        for binding in &report.bindings {
            if contradictions_only && !binding.contradictory {
                continue;
            }
            println!(
                "binding\t{}\t{}\t{}\t{}\tdeclared={}\tinferred={}\tcontradictory={}",
                safe_text!(report.path.display()),
                binding.line,
                binding.span.start().get(),
                safe_text!(binding.name),
                type_label(binding.declared),
                type_label(binding.inferred),
                binding.contradictory,
            );
        }

        for expression in &report.expressions {
            if contradictions_only && !expression.contradictory {
                continue;
            }
            println!(
                "expression\t{}\t{}\t{}\ttype={}\tcontradictory={}\t{}",
                safe_text!(report.path.display()),
                expression.line,
                expression.span.start().get(),
                expression.ty.as_str(),
                expression.contradictory,
                safe_text!(expression.text),
            );
        }
    }
}

fn print_json(
    reports: &[TypeReportFile],
    policy: &TypeReportPolicy,
    contradictions_only: bool,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "typed_binding_count": policy.typed_binding_count,
            "typed_expression_count": policy.typed_expression_count,
            "contradiction_count": policy.contradiction_count,
            "contradictions_only": contradictions_only,
            "policy": {
                "fail_on_contradiction": policy.fail_on_contradiction,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "dialect_modelled": report.dialect_modelled,
                    "untyped_binding_count": report.untyped_binding_count,
                    "bindings": report
                        .bindings
                        .iter()
                        .filter(|binding| !contradictions_only || binding.contradictory)
                        .map(|binding| json!({
                            "name": binding.name,
                            "declared": type_label(binding.declared),
                            "inferred": type_label(binding.inferred),
                            "binder": binding.binder,
                            "line": binding.line,
                            "span": {
                                "start": binding.span.start().get(),
                                "end": binding.span.end().get(),
                            },
                            "contradictory": binding.contradictory,
                        }))
                        .collect::<Vec<_>>(),
                    "expressions": report
                        .expressions
                        .iter()
                        .filter(|expression| !contradictions_only || expression.contradictory)
                        .map(|expression| json!({
                            "type": expression.ty.as_str(),
                            "line": expression.line,
                            "span": {
                                "start": expression.span.start().get(),
                                "end": expression.span.end().get(),
                            },
                            "text": expression.text,
                            "contradictory": expression.contradictory,
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
