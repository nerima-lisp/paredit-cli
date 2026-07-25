use anyhow::Result;
use serde_json::json;

use crate::application::usecase::workspace_report::types::{
    WorkspaceFileStatus, WorkspaceReportPlan,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_workspace_report(
    plan: &WorkspaceReportPlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => print_text_workspace_report(plan),
        OutputFormat::Json => print_json_workspace_report(plan)?,
    }

    Ok(())
}

fn print_text_workspace_report(plan: &WorkspaceReportPlan) {
    let summary = &plan.summary;
    println!(
        "roots\t{}",
        safe_text!(
            plan.roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    );
    println!("files\t{}", summary.file_count);
    println!("parsed\t{}", summary.parsed_count);
    println!("parse_errors\t{}", summary.parse_error_count);
    println!("bytes\t{}", summary.byte_count);
    println!("top_level_forms\t{}", summary.top_level_form_count);
    println!("atoms\t{}", summary.atom_count);
    println!("definitions\t{}", summary.definition_count);
    println!("calls\t{}", summary.call_count);
    println!("max_complexity_score\t{}", summary.max_complexity_score);
    println!("skipped_unknown\t{}", plan.skipped_unknown_count);
    println!("skipped_hidden\t{}", plan.skipped_hidden_count);
    println!("skipped_generated\t{}", plan.skipped_generated_count);
    println!("skipped_symlink\t{}", plan.skipped_symlink_count);
    for (dialect, count) in &summary.dialect_counts {
        println!("dialect\t{dialect}\t{count}");
    }
    for (status, count) in &summary.status_counts {
        println!("status\t{status}\t{count}");
    }
    for report in &plan.reports {
        println!(
            "{}\t{}\t{}\tdefinitions={}\tcalls={}\tmax_complexity_score={}",
            safe_text!(report.path.display()),
            report.dialect.label(),
            report.status.label(),
            report.definition_count,
            report.call_count,
            report.max_complexity_score
        );
    }
}

fn print_json_workspace_report(plan: &WorkspaceReportPlan) -> Result<()> {
    let summary = &plan.summary;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "roots": plan.roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>(),
            "file_count": summary.file_count,
            "parsed_count": summary.parsed_count,
            "parse_error_count": summary.parse_error_count,
            "byte_count": summary.byte_count,
            "top_level_form_count": summary.top_level_form_count,
            "atom_count": summary.atom_count,
            "definition_count": summary.definition_count,
            "call_count": summary.call_count,
            "max_complexity_score": summary.max_complexity_score,
            "dialects": summary.dialect_counts
                .iter()
                .map(|(dialect, count)| json!({
                    "dialect": dialect,
                    "count": count,
                }))
                .collect::<Vec<_>>(),
            "statuses": summary.status_counts
                .iter()
                .map(|(status, count)| json!({
                    "status": status,
                    "count": count,
                }))
                .collect::<Vec<_>>(),
            "skipped": {
                "unknown": plan.skipped_unknown_count,
                "hidden": plan.skipped_hidden_count,
                "generated": plan.skipped_generated_count,
                "symlink": plan.skipped_symlink_count,
            },
            "files": plan.reports
                .iter()
                .map(|report| json!({
                    "path": report.path.display().to_string(),
                    "dialect": report.dialect.label(),
                    "status": report.status.label(),
                    "error": match &report.status {
                        WorkspaceFileStatus::Parsed => None,
                        WorkspaceFileStatus::ParseError(error) => Some(error.as_str()),
                    },
                    "byte_count": report.byte_count,
                    "top_level_form_count": report.top_level_form_count,
                    "atom_count": report.atom_count,
                    "definition_count": report.definition_count,
                    "call_count": report.call_count,
                    "max_complexity_score": report.max_complexity_score,
                    "package": report.package.as_deref(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
