use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::constant_report::usecase::{ConstantReportFile, ConstantReportPolicy};

pub fn print_constant_report(
    reports: &[ConstantReportFile],
    policy: &ConstantReportPolicy,
    min_saved_bytes: i64,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => print_text(reports, policy, min_saved_bytes),
        OutputFormat::Json => print_json(reports, policy, min_saved_bytes)?,
    }
    Ok(())
}

fn print_text(reports: &[ConstantReportFile], policy: &ConstantReportPolicy, min: i64) {
    println!("files\t{}", reports.len());
    println!("foldable_count\t{}", policy.foldable_count);
    println!("constant_count\t{}", policy.constant_count);
    println!("saved_bytes\t{}", policy.saved_bytes);
    if policy.fail_on_foldable {
        println!("policy\tfail_on_foldable=true\tpassed={}", policy.passed);
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
        for constant in &report.constants {
            println!(
                "constant\t{}\t{}\t{}",
                safe_text!(report.path.display()),
                safe_text!(constant.name),
                safe_text!(constant.value),
            );
        }
        for item in report
            .foldable
            .iter()
            .filter(|item| item.saved_bytes >= min)
        {
            println!(
                "foldable\t{}\t{}\t{}\t{}\t=> {}\tkind={}\tsaved={}",
                safe_text!(report.path.display()),
                item.line,
                item.span.start().get(),
                safe_text!(item.text),
                safe_text!(item.value),
                item.kind,
                item.saved_bytes,
            );
        }
    }
}

fn print_json(
    reports: &[ConstantReportFile],
    policy: &ConstantReportPolicy,
    min: i64,
) -> CliResult<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "file_count": reports.len(),
            "foldable_count": policy.foldable_count,
            "constant_count": policy.constant_count,
            "saved_bytes": policy.saved_bytes,
            "min_saved_bytes": min,
            "policy": {
                "fail_on_foldable": policy.fail_on_foldable,
                "passed": policy.passed,
                "violations": &policy.violations,
            },
            "files": reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "dialect_modelled": report.dialect_modelled,
                    "constants": report
                        .constants
                        .iter()
                        .map(|constant| json!({
                            "name": constant.name,
                            "value": constant.value,
                        }))
                        .collect::<Vec<_>>(),
                    "foldable": report
                        .foldable
                        .iter()
                        .filter(|item| item.saved_bytes >= min)
                        .map(|item| json!({
                            "line": item.line,
                            "span": {
                                "start": item.span.start().get(),
                                "end": item.span.end().get(),
                            },
                            "text": item.text,
                            "value": item.value,
                            "kind": item.kind,
                            "saved_bytes": item.saved_bytes,
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
