//! The one printer every report in this package goes through.

use crate::args::ReportFormat;
use crate::shared::terminal_safe;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::IsTerminal;

use serde_json::{Value, json};

use super::interop::{self, Flattened};
use super::{FileFindings, Finding, ReportPolicy};

/// Prints a report in the requested format.
///
/// `command` names the report in its own gate message and JSON, so a consumer
/// aggregating several of these can tell which produced a finding without
/// tracking which process wrote it.
///
/// The two native formats read the `FileFindings` tree directly, since both
/// print per-file structure the interop formats have no place for (the summary
/// aggregation, the per-file dialect notice). Everything else goes through
/// [`Flattened`], which is the same findings with the file grouping dissolved.
pub fn print_report<F: Finding>(
    command: &'static str,
    reports: &[FileFindings<F>],
    policy: &ReportPolicy,
    output: ReportFormat,
) -> Result<()> {
    match output {
        ReportFormat::Text => print_text(reports, policy),
        ReportFormat::Json => print_json(command, reports, policy)?,
        other => {
            let flat = Flattened::new(command, reports, policy);
            match other {
                ReportFormat::Sarif => {
                    println!("{}", serde_json::to_string_pretty(&interop::sarif(&flat))?);
                }
                ReportFormat::CodeClimate => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&interop::code_climate(&flat))?
                    );
                }
                ReportFormat::Junit => print!("{}", interop::junit(&flat)),
                ReportFormat::Csv => print!("{}", interop::delimited(&flat, true)),
                ReportFormat::Tsv => print!("{}", interop::delimited(&flat, false)),
                ReportFormat::Html => print!("{}", interop::html(&flat)),
                ReportFormat::Markdown => print!("{}", interop::markdown(&flat)),
                ReportFormat::Github => print!("{}", interop::github(&flat)),
                // Handled above; repeating them here rather than using a
                // wildcard keeps a format added later a compile error.
                ReportFormat::Text | ReportFormat::Json => unreachable!(),
            }
        }
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

    // Tab-separated rows are the contract every script and every one of this
    // workspace's `assert_cmd` tests reads — `cargo test` never runs with a
    // terminal on the other end, so that path is unconditionally preserved.
    // A human at a live terminal gets an aligned, width-clamped table
    // instead, built from the exact same rows.
    let human = std::io::stdout().is_terminal();
    let mut finding_rows = Vec::new();

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
            if human {
                finding_rows.push(row);
            } else {
                println!("{}", row.join("\t"));
            }
        }
    }

    if human {
        for line in aligned_table_lines(&finding_rows, crate::terminal::width()) {
            println!("{line}");
        }
    }
}

/// Column-aligns `rows` (padding every column but the last, which is left
/// ragged since it is usually a message and padding it would only add
/// trailing whitespace) and clips each resulting line to `terminal_width`.
///
/// Clips rather than wraps: a wrapped row would need to keep collapsing back
/// to "which finding is this" for whatever comes after it, and a `Finding`'s
/// text columns are a summary already truncated by [`bounded_preview`]-style
/// limits upstream — the rare line still too wide for the terminal is
/// exactly the one a human least needs every last byte of on screen.
///
/// [`bounded_preview`]: crate::shared::bounded_preview
fn aligned_table_lines(rows: &[Vec<String>], terminal_width: usize) -> Vec<String> {
    let Some(column_count) = rows.iter().map(Vec::len).max() else {
        return Vec::new();
    };
    let mut widths = vec![0usize; column_count];
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    rows.iter()
        .map(|row| {
            let mut line = String::new();
            let last = row.len().saturating_sub(1);
            for (index, cell) in row.iter().enumerate() {
                if index > 0 {
                    line.push_str("  ");
                }
                if index == last {
                    line.push_str(cell);
                } else {
                    write!(line, "{cell:<width$}", width = widths[index])
                        .expect("writing to a String is infallible");
                }
            }
            clip_to_width(&line, terminal_width)
        })
        .collect()
}

/// Clips `line` to `width` display columns, marking the cut with `…`.
///
/// Below four columns there is no sensible line left once the marker is
/// accounted for, so an unreasonably narrow terminal gets the unclipped line
/// rather than something even less readable.
fn clip_to_width(line: &str, width: usize) -> String {
    let char_count = line.chars().count();
    if char_count <= width || width < 4 {
        return line.to_owned();
    }
    let kept: String = line.chars().take(width - 1).collect();
    format!("{kept}…")
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

    #[test]
    fn aligned_table_lines_pads_every_column_but_the_last() {
        let rows = vec![
            vec!["short".to_owned(), "a.lisp".to_owned(), "hi".to_owned()],
            vec![
                "much-longer".to_owned(),
                "b.lisp".to_owned(),
                "yo".to_owned(),
            ],
        ];
        let lines = aligned_table_lines(&rows, 80);
        assert_eq!(lines.len(), 2);
        // The first column is padded to the width of "much-longer" (11) plus
        // the two-space separator; "a.lisp"/"b.lisp" are already at their
        // column's max width, so only the separator follows them. The last
        // column ("hi"/"yo") is never padded, since nothing follows it.
        assert_eq!(
            lines[0],
            format!("short{}a.lisp  hi", " ".repeat(11 - 5 + 2))
        );
        assert_eq!(lines[1], "much-longer  b.lisp  yo");
    }

    #[test]
    fn aligned_table_lines_of_no_rows_is_empty() {
        assert!(aligned_table_lines(&[], 80).is_empty());
    }

    #[test]
    fn aligned_table_lines_tolerates_ragged_rows() {
        let rows = vec![vec!["a".to_owned(), "b".to_owned()], vec!["c".to_owned()]];
        // Neither row indexes past its own length; nothing panics.
        assert_eq!(aligned_table_lines(&rows, 80).len(), 2);
    }

    #[test]
    fn clip_to_width_leaves_a_short_line_alone() {
        assert_eq!(clip_to_width("short", 80), "short");
        assert_eq!(clip_to_width("exact", 5), "exact");
    }

    #[test]
    fn clip_to_width_marks_a_cut_line_with_an_ellipsis() {
        let clipped = clip_to_width("this line is much too long to fit", 10);
        assert_eq!(clipped, "this line…");
        assert_eq!(clipped.chars().count(), 10);
    }

    #[test]
    fn clip_to_width_leaves_a_line_alone_below_the_sensible_minimum() {
        // Nothing useful is left of a line clipped to fewer than four
        // columns, so an absurdly narrow terminal gets the line as-is.
        assert_eq!(clip_to_width("still too long", 3), "still too long");
    }
}
