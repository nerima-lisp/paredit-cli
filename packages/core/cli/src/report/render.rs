//! The one printer every report in this package goes through.

use crate::args::OutputFormat;
use crate::shared::terminal_safe;
use anyhow::Result;
use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{FileFindings, Finding, ReportPolicy};

/// Prints a report in the requested format.
///
/// `command` names the report in its own gate message and JSON, so a consumer
/// aggregating several of these can tell which produced a finding without
/// tracking which process wrote it.
pub fn print_report<F: Finding>(
    command: &'static str,
    reports: &[FileFindings<F>],
    policy: &ReportPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => print_text(reports, policy),
        OutputFormat::Json => print_json(command, reports, policy)?,
    }
    Ok(())
}

fn print_text<F: Finding>(reports: &[FileFindings<F>], policy: &ReportPolicy) {
    println!("files\t{}", reports.len());
    println!("finding_count\t{}", policy.finding_count);
    for (name, value) in aggregate(reports) {
        println!("{name}\t{value}");
    }
    if let Some(gate) = policy.gate {
        println!("policy\t{gate}\tpassed={}", policy.passed);
    }

    for report in reports {
        if !report.dialect_modelled {
            println!(
                "unmodelled\t{}\t{}\tthis report covers Common Lisp only",
                terminal_safe(&report.path.display()),
                report.dialect.label(),
            );
            continue;
        }
        for finding in &report.findings {
            let mut row = vec![
                finding.kind().to_owned(),
                terminal_safe(&report.path.display()).to_string(),
                finding.line().to_string(),
            ];
            row.extend(
                finding
                    .text_columns()
                    .into_iter()
                    .map(|column| terminal_safe(&column).to_string()),
            );
            println!("{}", row.join("\t"));
        }
    }
}

fn print_json<F: Finding>(
    command: &'static str,
    reports: &[FileFindings<F>],
    policy: &ReportPolicy,
) -> Result<()> {
    let mut report = json!({
        "schema_version": 1,
        "report": command,
        "file_count": reports.len(),
        "finding_count": policy.finding_count,
        "policy": {
            "gate": policy.gate,
            "passed": policy.passed,
            "violations": &policy.violations,
        },
        "files": reports.iter().map(file_json::<F>).collect::<Vec<_>>(),
    });
    for (name, value) in aggregate(reports) {
        report[name] = value;
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn file_json<F: Finding>(report: &FileFindings<F>) -> Value {
    let mut object = json!({
        "path": report.path.display().to_string(),
        "dialect": report.dialect.label(),
        "dialect_modelled": report.dialect_modelled,
        "findings": report
            .findings
            .iter()
            .map(|finding| {
                let mut entry = json!({
                    "kind": finding.kind(),
                    "line": finding.line(),
                    "span": {
                        "start": finding.span().start().get(),
                        "end": finding.span().end().get(),
                    },
                });
                for (name, value) in finding.json_fields() {
                    entry[name] = value;
                }
                entry
            })
            .collect::<Vec<_>>(),
    });
    for (name, value) in &report.summary {
        object[*name] = value.clone();
    }
    object
}

/// Sums each per-file summary key across the whole run.
///
/// Numeric keys add; anything else is dropped rather than concatenated, since
/// there is no meaningful whole-run value for a per-file string. Key order
/// follows the first file that declared each key, so the top of the output does
/// not reshuffle when a later file happens to carry an extra count.
fn aggregate<F: Finding>(reports: &[FileFindings<F>]) -> Vec<(&'static str, Value)> {
    let mut order: Vec<&'static str> = Vec::new();
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for report in reports {
        for (name, value) in &report.summary {
            let Some(number) = value.as_u64() else {
                continue;
            };
            if !totals.contains_key(*name) {
                order.push(name);
            }
            *totals.entry((*name).to_owned()).or_default() += number;
        }
    }
    order
        .into_iter()
        .map(|name| (name, json!(totals[name])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};
    use std::path::PathBuf;

    #[derive(Debug)]
    struct Probe;

    impl Finding for Probe {
        fn kind(&self) -> &'static str {
            "probe"
        }
        fn span(&self) -> ByteSpan {
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(1))
        }
        fn line(&self) -> usize {
            1
        }
        fn text_columns(&self) -> Vec<String> {
            Vec::new()
        }
        fn json_fields(&self) -> Vec<(&'static str, Value)> {
            Vec::new()
        }
    }

    fn file(summary: Vec<(&'static str, Value)>) -> FileFindings<Probe> {
        FileFindings::new(
            PathBuf::from("t.lisp"),
            Dialect::CommonLisp,
            true,
            Vec::new(),
            summary,
        )
    }

    #[test]
    fn numeric_summary_keys_add_across_files() {
        let reports = [
            file(vec![("widgets", json!(2))]),
            file(vec![("widgets", json!(3))]),
        ];
        assert_eq!(aggregate(&reports), vec![("widgets", json!(5))]);
    }

    #[test]
    fn a_non_numeric_summary_key_is_dropped_rather_than_concatenated() {
        let reports = [file(vec![("note", json!("hello"))])];
        assert!(aggregate(&reports).is_empty());
    }

    #[test]
    fn aggregating_no_files_yields_no_keys() {
        let reports: [FileFindings<Probe>; 0] = [];
        assert!(aggregate(&reports).is_empty());
    }
}
