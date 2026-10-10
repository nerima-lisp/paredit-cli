use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::report::ReportPolicy;
use paredit_core_cli::safe_text;
use serde_json::json;

use crate::test_risk_report::domain::{COVERAGE_BASIS, LIMITATION, TestRiskReport};

pub fn print_test_risk_report(
    report: &TestRiskReport,
    policy: &ReportPolicy,
    top: Option<usize>,
    output: OutputFormat,
) -> CliResult<()> {
    let ranked = &report.ranked[..top.unwrap_or(report.ranked.len()).min(report.ranked.len())];
    let function_count: usize = report
        .files
        .iter()
        .filter_map(|file| file.function_count)
        .sum();
    let linked_count: usize = report
        .files
        .iter()
        .filter_map(|file| file.linked_function_count)
        .sum();
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "command": "inspect test-risk",
                "coverage_basis": COVERAGE_BASIS,
                "limitation": LIMITATION,
                "analysis_complete": report.files.iter().all(|file| file.modelled),
                "file_count": report.files.len(),
                "modelled_function_count": function_count,
                "linked_function_count": linked_count,
                "unlinked_function_count": report.ranked.len(),
                "displayed_count": ranked.len(),
                "policy": {
                    "gate": policy.gate,
                    "scope": "modelled-functions",
                    "finding_count": policy.finding_count,
                    "passed": policy.passed,
                    "violations": policy.violations,
                },
                "ranked": ranked.iter().enumerate().map(|(index, entry)| json!({
                    "rank": index + 1,
                    "file": entry.file,
                    "dialect": entry.dialect.label(),
                    "name": entry.name,
                    "category": "function",
                    "link_status": "no-naming-convention-test-link",
                    "path": entry.complexity.path,
                    "line": entry.line,
                    "span": {"start": entry.complexity.span.start().get(), "end": entry.complexity.span.end().get()},
                    "max_depth": entry.complexity.max_depth,
                    "atom_count": entry.complexity.atom_count,
                    "list_count": entry.complexity.list_count,
                    "complexity_score": entry.complexity.complexity_score,
                })).collect::<Vec<_>>(),
                "files": report.files.iter().map(|file| json!({
                    "path": file.path,
                    "dialect": file.dialect.label(),
                    "modelled": file.modelled,
                    "function_count": file.function_count,
                    "linked_function_count": file.linked_function_count,
                    "unlinked_function_count": file.unlinked_function_count,
                    "unmodelled_reason": (!file.modelled).then_some("test-name linkage is modelled only for Common Lisp and Emacs Lisp"),
                })).collect::<Vec<_>>(),
            }))?
        ),
        OutputFormat::Text => {
            println!("coverage_basis\t{COVERAGE_BASIS}");
            println!("limitation\t{LIMITATION}");
            println!("files\t{}", report.files.len());
            println!(
                "modelled_functions\t{function_count}\tlinked={linked_count}\tunlinked={}",
                report.ranked.len()
            );
            println!(
                "policy\tscope=modelled-functions\tgate={}\tpassed={}",
                policy.gate.unwrap_or("none"),
                policy.passed
            );
            for file in &report.files {
                println!(
                    "file\t{}\t{}\tmodelled={}\tfunctions={}",
                    safe_text!(file.path.display()),
                    file.dialect.label(),
                    file.modelled,
                    file.function_count
                        .map_or_else(|| "unknown".to_owned(), |count| count.to_string())
                );
            }
            for (index, entry) in ranked.iter().enumerate() {
                println!(
                    "rank\t{}\t{}\t{}\t{}\tline={}\tscore={}\tdepth={}\tlists={}\tatoms={}",
                    index + 1,
                    safe_text!(entry.file.display()),
                    entry.dialect.label(),
                    safe_text!(entry.name),
                    entry.line,
                    entry.complexity.complexity_score,
                    entry.complexity.max_depth,
                    entry.complexity.list_count,
                    entry.complexity.atom_count
                );
            }
        }
    }
    Ok(())
}
