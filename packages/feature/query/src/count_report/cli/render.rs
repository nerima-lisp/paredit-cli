use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::count_report::usecase::CountReport;

pub fn print_count_report(
    report: &CountReport,
    per_file: bool,
    include_empty: bool,
    output: OutputFormat,
) -> Result<()> {
    let files: Vec<_> = report
        .files
        .iter()
        .filter(|file| include_empty || !file.is_empty())
        .collect();

    match output {
        OutputFormat::Text => {
            println!(
                "files\t{}\tpatterns\t{}\ttotal\t{}",
                report.files.len(),
                report.patterns.len(),
                report.grand_total()
            );
            for (pattern, total) in report.patterns.iter().zip(&report.totals) {
                println!("pattern\t{total}\t{}", safe_text!(pattern));
            }
            if per_file {
                for file in files {
                    println!(
                        "file\t{}\t{}",
                        file.path.display(),
                        file.counts
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\t")
                    );
                }
            }
        }
        OutputFormat::Json => {
            let mut payload = json!({
                "schema_version": 1,
                "fileCount": report.files.len(),
                "total": report.grand_total(),
                "patterns": report
                    .patterns
                    .iter()
                    .zip(&report.totals)
                    .map(|(pattern, total)| json!({ "query": pattern, "count": total }))
                    .collect::<Vec<_>>(),
            });
            if per_file {
                payload["files"] = json!(
                    files
                        .iter()
                        .map(|file| json!({
                            "path": file.path.display().to_string(),
                            "dialect": file.dialect.label(),
                            "counts": file.counts,
                            "total": file.counts.iter().sum::<usize>(),
                        }))
                        .collect::<Vec<_>>()
                );
            }
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
    }
    Ok(())
}
