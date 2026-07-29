//! The report envelope's interoperability formats.
//!
//! Every report in this tool already produces the same thing: located findings
//! with a kind, a line, a byte span, and a message. That is precisely the shape
//! SARIF, JUnit XML, Code Climate JSON, CSV, and an HTML table each describe —
//! so the conversion is a projection, not an analysis, and belongs once beside
//! the envelope rather than once per report.
//!
//! Everything here flattens through [`Row`] first. Writing seven emitters
//! against `FileFindings<F>` directly would mean seven copies of "walk files,
//! skip the unmodelled ones, pull the finding's fields"; writing them against a
//! flat row means each emitter is only its own format's syntax.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::shared::terminal_safe;

use super::{FileFindings, Finding, FindingSeverity, ReportPolicy};

/// One finding, detached from the file that produced it.
#[derive(Debug)]
pub struct Row {
    pub path: String,
    pub dialect: &'static str,
    pub kind: &'static str,
    pub severity: FindingSeverity,
    pub line: usize,
    pub span_start: usize,
    pub span_end: usize,
    pub message: String,
    pub fields: Vec<(&'static str, Value)>,
}

/// A file this report's analysis does not model, and therefore did not read.
///
/// Carried separately from [`Row`] all the way to the emitters because the
/// distinction is the point: a modelled file with no findings is clean, an
/// unmodelled one is unexamined, and every one of these formats has a way to
/// say the second — SARIF a `note`, JUnit a `<skipped>`, the tables a section.
#[derive(Debug)]
pub struct Skipped {
    pub path: String,
    pub dialect: &'static str,
}

/// Everything the emitters need, computed once.
#[derive(Debug)]
pub struct Flattened {
    pub command: &'static str,
    pub rows: Vec<Row>,
    pub skipped: Vec<Skipped>,
    pub file_count: usize,
    pub gate: Option<&'static str>,
    pub gate_passed: bool,
    pub violations: Vec<String>,
}

impl Flattened {
    #[must_use]
    pub fn new<F: Finding>(
        command: &'static str,
        reports: &[FileFindings<F>],
        policy: &ReportPolicy,
    ) -> Self {
        let mut rows = Vec::new();
        let mut skipped = Vec::new();
        for report in reports {
            let path = report.path.display().to_string();
            if !report.dialect_modelled {
                skipped.push(Skipped {
                    path,
                    dialect: report.dialect.label(),
                });
                continue;
            }
            for finding in &report.findings {
                rows.push(Row {
                    path: path.clone(),
                    dialect: report.dialect.label(),
                    kind: finding.kind(),
                    severity: finding.severity(),
                    line: finding.line(),
                    span_start: finding.span().start().get(),
                    span_end: finding.span().end().get(),
                    message: finding.message(),
                    fields: finding.json_fields(),
                });
            }
        }
        Self {
            command,
            rows,
            skipped,
            file_count: reports.len(),
            gate: policy.gate,
            gate_passed: policy.passed,
            violations: policy.violations.clone(),
        }
    }

    /// The report-specific field names, in the order the first finding
    /// declared them.
    ///
    /// A union rather than the first row's list: a report whose findings are an
    /// enum can emit different keys per variant, and a column that exists for
    /// only some rows still needs a header.
    fn field_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        for row in &self.rows {
            for (name, _) in &row.fields {
                if !names.contains(name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// The rows grouped by path, preserving the order files were reported in.
    fn by_path(&self) -> Vec<(&str, Vec<&Row>)> {
        let mut order: Vec<&str> = Vec::new();
        let mut grouped: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
        for row in &self.rows {
            if !grouped.contains_key(row.path.as_str()) {
                order.push(row.path.as_str());
            }
            grouped.entry(row.path.as_str()).or_default().push(row);
        }
        order
            .into_iter()
            .map(|path| (path, grouped.remove(path).unwrap_or_default()))
            .collect()
    }
}

/// A finding's value for one field name, rendered flat.
fn field_text(row: &Row, name: &str) -> String {
    row.fields
        .iter()
        .find(|(field, _)| *field == name)
        .map_or_else(String::new, |(_, value)| scalar_text(value))
}

/// Renders a JSON value for a single table cell.
///
/// Strings lose their quotes — a CSV cell holding `"foo"` where the value is
/// `foo` is a false quoting level that a spreadsheet will not undo. Everything
/// else keeps its JSON spelling, since a nested array in one cell is better
/// than silently dropping it.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------- SARIF

/// SARIF 2.1.0, the format GitHub code scanning and Azure DevOps ingest.
///
/// Rules are advertised from the kinds actually present rather than from a
/// static catalog: unlike `lint`, these reports have no registry of every kind
/// they can emit, and a `rules` array is allowed to describe only what the run
/// produced.
#[must_use]
pub fn sarif(flat: &Flattened) -> Value {
    let mut kinds: Vec<&'static str> = Vec::new();
    for row in &flat.rows {
        if !kinds.contains(&row.kind) {
            kinds.push(row.kind);
        }
    }
    let rules = kinds
        .iter()
        .map(|kind| {
            json!({
                "id": rule_id(flat.command, kind),
                "name": kind,
                "shortDescription": { "text": format!("{} ({kind})", flat.command) },
                "properties": { "report": flat.command },
            })
        })
        .collect::<Vec<_>>();

    let mut results = flat
        .rows
        .iter()
        .map(|row| {
            let mut properties = json!({ "report": flat.command, "dialect": row.dialect });
            for (name, value) in &row.fields {
                properties[*name] = value.clone();
            }
            json!({
                "ruleId": rule_id(flat.command, row.kind),
                "level": row.severity.as_str(),
                "message": { "text": row.message },
                "properties": properties,
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": row.path },
                        "region": {
                            "startLine": row.line,
                            "byteOffset": row.span_start,
                            "byteLength": row.span_end.saturating_sub(row.span_start),
                        },
                    },
                }],
            })
        })
        .collect::<Vec<_>>();

    // An unmodelled file is reported, not omitted. A SARIF consumer that sees
    // no results for a file concludes the file is clean, and for a dialect this
    // report carries no analysis for that conclusion is wrong.
    results.extend(flat.skipped.iter().map(|skipped| {
        json!({
            "ruleId": rule_id(flat.command, UNMODELLED_KIND),
            "level": FindingSeverity::Note.as_str(),
            "kind": "informational",
            "message": { "text": format!(
                "{} carries no analysis for {}; this file was not examined",
                flat.command, skipped.dialect,
            ) },
            "locations": [{
                "physicalLocation": { "artifactLocation": { "uri": skipped.path } },
            }],
        })
    }));

    let mut driver = json!({
        "name": "paredit",
        "informationUri": "https://github.com/nerima-lisp/paredit-cli",
        "version": env!("CARGO_PKG_VERSION"),
        "rules": rules,
    });
    if !flat.skipped.is_empty() {
        driver["rules"].as_array_mut().expect("rules array").push(json!({
            "id": rule_id(flat.command, UNMODELLED_KIND),
            "name": UNMODELLED_KIND,
            "shortDescription": { "text": "the file's dialect is outside this report's analysis" },
        }));
    }

    json!({
        "version": "2.1.0",
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "runs": [{
            "tool": { "driver": driver },
            "invocations": [{
                "executionSuccessful": flat.gate_passed,
                "properties": {
                    "gate": flat.gate,
                    "violations": flat.violations,
                },
            }],
            "results": results,
        }],
    })
}

const UNMODELLED_KIND: &str = "unmodelled-dialect";

/// `inspect read-time-eval` + `read-eval` becomes `inspect/read-time-eval/read-eval`.
///
/// Slashes rather than dots: SARIF rule ids are opaque strings, but code
/// scanning UIs render a dotted id as a namespace and truncate it, and these
/// ids are already two levels deep before the kind.
fn rule_id(command: &str, kind: &str) -> String {
    format!("{}/{kind}", command.replace(' ', "/"))
}

// ---------------------------------------------------------------- JUnit

/// JUnit XML: one `<testsuite>` per file, one `<testcase>` per finding.
///
/// A clean file still emits a passing testcase and an unmodelled one a
/// `<skipped>`, because a JUnit panel showing nothing for a file is
/// indistinguishable from a file that was never given to the tool.
#[must_use]
pub fn junit(flat: &Flattened) -> String {
    let grouped = flat.by_path();
    let failures = flat.rows.len();
    // Saturating: the same path can be reported twice in one run (a caller may
    // list it twice, or a directory scan may reach it by two routes), which
    // collapses in `grouped` and would otherwise underflow this subtraction
    // into a panic on a debug build.
    let clean_files = flat
        .file_count
        .saturating_sub(grouped.len())
        .saturating_sub(flat.skipped.len());
    let tests = failures + clean_files + flat.skipped.len();

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"{}\" tests=\"{tests}\" failures=\"{failures}\" skipped=\"{}\">",
        xml(flat.command),
        flat.skipped.len(),
    );

    for (path, rows) in &grouped {
        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\">",
            xml(path),
            rows.len(),
            rows.len(),
        );
        for row in rows {
            let _ = writeln!(
                out,
                "    <testcase name=\"{}:{} {}\" classname=\"{}\">",
                xml(path),
                row.line,
                xml(row.kind),
                xml(flat.command),
            );
            let _ = writeln!(
                out,
                "      <failure message=\"{}\" type=\"{}\">{}</failure>",
                xml(&row.message),
                xml(row.kind),
                xml(&format!(
                    "{}:{} [{}] {} (bytes {}..{})",
                    path, row.line, row.kind, row.message, row.span_start, row.span_end,
                )),
            );
            let _ = writeln!(out, "    </testcase>");
        }
        let _ = writeln!(out, "  </testsuite>");
    }

    if !flat.skipped.is_empty() {
        let _ = writeln!(
            out,
            "  <testsuite name=\"unmodelled\" tests=\"{}\" failures=\"0\" skipped=\"{}\">",
            flat.skipped.len(),
            flat.skipped.len(),
        );
        for skipped in &flat.skipped {
            let _ = writeln!(
                out,
                "    <testcase name=\"{}\" classname=\"{}\">",
                xml(&skipped.path),
                xml(flat.command),
            );
            let _ = writeln!(
                out,
                "      <skipped message=\"{}\"/>",
                xml(&format!(
                    "{} carries no analysis for {}",
                    flat.command, skipped.dialect,
                )),
            );
            let _ = writeln!(out, "    </testcase>");
        }
        let _ = writeln!(out, "  </testsuite>");
    }

    let _ = writeln!(out, "</testsuites>");
    out
}

// --------------------------------------------------------- Code Climate

/// The Code Climate issue array GitLab's Code Quality panel reads.
///
/// The fingerprint is content-derived — report, path, kind, and message — and
/// deliberately excludes the line number, so an issue does not read as new
/// after an unrelated edit above it shifts the file down.
#[must_use]
pub fn code_climate(flat: &Flattened) -> Value {
    Value::Array(
        flat.rows
            .iter()
            .map(|row| {
                json!({
                    "type": "issue",
                    "check_name": rule_id(flat.command, row.kind),
                    "description": row.message,
                    "categories": ["Bug Risk"],
                    "severity": row.severity.code_climate(),
                    "fingerprint": fingerprint(flat.command, row),
                    "location": {
                        "path": row.path,
                        "lines": { "begin": row.line, "end": row.line },
                    },
                })
            })
            .collect(),
    )
}

fn fingerprint(command: &str, row: &Row) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in [command, row.path.as_str(), row.kind, row.message.as_str()] {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

// ------------------------------------------------------------ CSV / TSV

/// Delimiter-separated values with a header row.
///
/// CSV quotes per RFC 4180; TSV cannot quote, so its cells go through
/// [`terminal_safe`], which renders a tab or newline as an escape rather than
/// breaking the row. That difference is why one function takes the delimiter
/// instead of two functions sharing a body.
#[must_use]
pub fn delimited(flat: &Flattened, comma: bool) -> String {
    let extra = flat.field_names();
    let mut header = vec![
        "report".to_owned(),
        "path".to_owned(),
        "dialect".to_owned(),
        "kind".to_owned(),
        "severity".to_owned(),
        "line".to_owned(),
        "span_start".to_owned(),
        "span_end".to_owned(),
        "message".to_owned(),
    ];
    header.extend(extra.iter().map(|name| (*name).to_owned()));

    let mut out = String::new();
    let _ = writeln!(out, "{}", join(&header, comma));

    for row in &flat.rows {
        let mut cells = vec![
            flat.command.to_owned(),
            row.path.clone(),
            row.dialect.to_owned(),
            row.kind.to_owned(),
            row.severity.as_str().to_owned(),
            row.line.to_string(),
            row.span_start.to_string(),
            row.span_end.to_string(),
            row.message.clone(),
        ];
        cells.extend(extra.iter().map(|name| field_text(row, name)));
        let _ = writeln!(out, "{}", join(&cells, comma));
    }

    for skipped in &flat.skipped {
        let mut cells = vec![
            flat.command.to_owned(),
            skipped.path.clone(),
            skipped.dialect.to_owned(),
            UNMODELLED_KIND.to_owned(),
            FindingSeverity::Note.as_str().to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            format!("{} carries no analysis for this dialect", flat.command),
        ];
        cells.resize(header.len(), String::new());
        let _ = writeln!(out, "{}", join(&cells, comma));
    }

    out
}

fn join(cells: &[String], comma: bool) -> String {
    let rendered = cells.iter().map(|cell| {
        if comma {
            csv_field(cell)
        } else {
            terminal_safe(cell).to_string()
        }
    });
    rendered
        .collect::<Vec<_>>()
        .join(if comma { "," } else { "\t" })
}

/// RFC 4180 quoting: quote when the cell holds a delimiter, a quote, or a line
/// break, and double any embedded quote.
fn csv_field(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_owned()
    }
}

// ------------------------------------------------------------- Markdown

/// A Markdown table, sized for a pull request comment.
#[must_use]
pub fn markdown(flat: &Flattened) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", md(flat.command));
    let _ = writeln!(
        out,
        "{} finding{} across {} file{}.\n",
        flat.rows.len(),
        plural(flat.rows.len()),
        flat.file_count,
        plural(flat.file_count),
    );
    if let Some(gate) = flat.gate {
        let _ = writeln!(
            out,
            "Gate `{gate}`: **{}**\n",
            if flat.gate_passed { "passed" } else { "failed" },
        );
    }

    if flat.rows.is_empty() {
        let _ = writeln!(out, "No findings.\n");
    } else {
        let _ = writeln!(out, "| file | line | kind | severity | detail |");
        let _ = writeln!(out, "| --- | ---: | --- | --- | --- |");
        for row in &flat.rows {
            let _ = writeln!(
                out,
                "| `{}` | {} | `{}` | {} | {} |",
                md(&row.path),
                row.line,
                md(row.kind),
                row.severity.as_str(),
                md(&row.message),
            );
        }
        let _ = writeln!(out);
    }

    if !flat.skipped.is_empty() {
        let _ = writeln!(out, "## Not examined\n");
        for skipped in &flat.skipped {
            let _ = writeln!(
                out,
                "- `{}` — {} is outside this report's analysis",
                md(&skipped.path),
                md(skipped.dialect),
            );
        }
        let _ = writeln!(out);
    }

    out
}

const fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// Escapes the Markdown metacharacters that break a table cell.
///
/// A pipe would end the cell and a backtick would end the code span; the rest
/// of Markdown's punctuation renders harmlessly inside one, so escaping it all
/// would only make the source noisier for a human reading the raw text.
fn md(text: &str) -> String {
    terminal_safe(text)
        .to_string()
        .replace('|', "\\|")
        .replace('`', "\\`")
}

// --------------------------------------------------------------- GitHub

/// GitHub Actions workflow commands, one per finding.
///
/// The runner renders these inline on the pull request diff. The severity word
/// is load-bearing: `::error` marks the check failed and `::warning` does not,
/// so a report whose findings are observations rather than defects annotates
/// without turning the check red.
#[must_use]
pub fn github(flat: &Flattened) -> String {
    let mut out = String::new();
    for row in &flat.rows {
        let command = match row.severity {
            FindingSeverity::Error => "error",
            FindingSeverity::Warning => "warning",
            FindingSeverity::Note => "notice",
        };
        let _ = writeln!(
            out,
            "::{command} file={},line={},title={}::{}",
            annotation_property(&row.path),
            row.line,
            annotation_property(&format!("{} {}", flat.command, row.kind)),
            annotation_data(&row.message),
        );
    }
    for skipped in &flat.skipped {
        let _ = writeln!(
            out,
            "::notice file={},title={}::{}",
            annotation_property(&skipped.path),
            annotation_property(&format!("{} not examined", flat.command)),
            annotation_data(&format!(
                "{} carries no analysis for {}",
                flat.command, skipped.dialect,
            )),
        );
    }
    out
}

/// Percent-encodes an annotation *message*: the runner splits on `%`, `\r`, and
/// `\n`, so a message containing any of them would truncate or misparse.
fn annotation_data(value: &str) -> String {
    terminal_safe(value)
        .to_string()
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Percent-encodes an annotation *property* value, which additionally encodes
/// the `,` and `:` that delimit the command's own properties.
fn annotation_property(value: &str) -> String {
    annotation_data(value)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

// ----------------------------------------------------------------- HTML

/// A standalone HTML page: one file, no assets, openable from a CI artifact.
#[must_use]
pub fn html(flat: &Flattened) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "<!DOCTYPE html>");
    let _ = writeln!(out, "<html lang=\"en\">");
    let _ = writeln!(out, "<head>");
    let _ = writeln!(out, "<meta charset=\"utf-8\">");
    let _ = writeln!(out, "<title>paredit {} report</title>", xml(flat.command));
    let _ = writeln!(out, "<style>{HTML_STYLE}</style>");
    let _ = writeln!(out, "</head>");
    let _ = writeln!(out, "<body>");
    let _ = writeln!(out, "<h1>paredit {}</h1>", xml(flat.command));
    let _ = writeln!(
        out,
        "<p class=\"summary\">{} finding{} across {} file{}.</p>",
        flat.rows.len(),
        plural(flat.rows.len()),
        flat.file_count,
        plural(flat.file_count),
    );
    if let Some(gate) = flat.gate {
        let _ = writeln!(
            out,
            "<p class=\"gate {}\">gate <code>{}</code>: {}</p>",
            if flat.gate_passed { "pass" } else { "fail" },
            xml(gate),
            if flat.gate_passed { "passed" } else { "failed" },
        );
    }

    if flat.rows.is_empty() {
        let _ = writeln!(out, "<p>No findings.</p>");
    } else {
        let _ = writeln!(out, "<table>");
        let _ = writeln!(
            out,
            "<thead><tr><th>file</th><th>line</th><th>kind</th>\
             <th>severity</th><th>detail</th></tr></thead>"
        );
        let _ = writeln!(out, "<tbody>");
        for row in &flat.rows {
            let _ = writeln!(
                out,
                "<tr class=\"{}\"><td><code>{}</code></td><td class=\"n\">{}</td>\
                 <td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                row.severity.as_str(),
                xml(&row.path),
                row.line,
                xml(row.kind),
                row.severity.as_str(),
                xml(&row.message),
            );
        }
        let _ = writeln!(out, "</tbody></table>");
    }

    if !flat.skipped.is_empty() {
        let _ = writeln!(out, "<h2>Not examined</h2><ul>");
        for skipped in &flat.skipped {
            let _ = writeln!(
                out,
                "<li><code>{}</code> — {} is outside this report's analysis</li>",
                xml(&skipped.path),
                xml(skipped.dialect),
            );
        }
        let _ = writeln!(out, "</ul>");
    }

    let _ = writeln!(out, "</body>");
    let _ = writeln!(out, "</html>");
    out
}

const HTML_STYLE: &str = "\
body{font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;margin:2rem;color:#111}\
h1{font-size:1.2rem}h2{font-size:1rem;margin-top:2rem}\
table{border-collapse:collapse;width:100%}\
th,td{border-bottom:1px solid #ddd;padding:.35rem .6rem;text-align:left;vertical-align:top}\
th{background:#f6f6f6}td.n{text-align:right}\
tr.error td{background:#fff5f5}tr.note td{color:#666}\
.gate.fail{color:#b00}.gate.pass{color:#070}\
code{background:#f2f2f2;padding:0 .2em;border-radius:3px}";

/// Escapes text for an XML or HTML text node or double-quoted attribute.
///
/// [`terminal_safe`] runs first, not for the terminal but because XML 1.0
/// forbids most C0 control characters outright — there is no escape for them,
/// so a raw one in a path makes the document unparseable. Rendering them as
/// `\u{1b}` keeps the document well-formed and the byte visible.
fn xml(text: &str) -> String {
    terminal_safe(text)
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};
    use std::path::PathBuf;

    #[derive(Debug)]
    struct Probe {
        line: usize,
        detail: &'static str,
    }

    impl Finding for Probe {
        fn kind(&self) -> &'static str {
            "probe"
        }
        fn span(&self) -> ByteSpan {
            ByteSpan::new(ByteOffset::new(self.line), ByteOffset::new(self.line + 4))
        }
        fn line(&self) -> usize {
            self.line
        }
        fn text_columns(&self) -> Vec<String> {
            vec![self.detail.to_owned()]
        }
        fn json_fields(&self) -> Vec<(&'static str, Value)> {
            vec![("detail", json!(self.detail))]
        }
    }

    fn flat(findings: Vec<Probe>, modelled: bool) -> Flattened {
        let reports = [FileFindings::new(
            PathBuf::from("a,b.lisp"),
            if modelled {
                Dialect::CommonLisp
            } else {
                Dialect::Fennel
            },
            modelled,
            findings,
            Vec::new(),
        )];
        let policy =
            ReportPolicy::fail_on_any(Some("--fail-on-any"), &reports, |_| "a,b.lisp".to_owned());
        Flattened::new("inspect probe", &reports, &policy)
    }

    #[test]
    fn an_unmodelled_file_is_reported_rather_than_left_looking_clean() {
        let flattened = flat(Vec::new(), false);
        assert!(flattened.rows.is_empty());
        assert_eq!(flattened.skipped.len(), 1);

        let sarif = sarif(&flattened);
        let results = sarif["runs"][0]["results"].as_array().expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["level"], "note");

        assert!(junit(&flattened).contains("<skipped"));
        assert!(delimited(&flattened, true).contains("unmodelled-dialect"));
        assert!(html(&flattened).contains("Not examined"));
    }

    #[test]
    fn a_csv_cell_holding_the_delimiter_is_quoted() {
        let csv = delimited(
            &flat(
                vec![Probe {
                    line: 3,
                    detail: "x",
                }],
                true,
            ),
            true,
        );
        assert!(csv.contains("\"a,b.lisp\""), "{csv}");
    }

    #[test]
    fn a_tsv_cell_cannot_break_its_row() {
        let flattened = flat(
            vec![Probe {
                line: 3,
                detail: "one\ttwo\nthree",
            }],
            true,
        );
        let tsv = delimited(&flattened, false);
        assert_eq!(tsv.lines().count(), 2, "header plus one row: {tsv}");
    }

    #[test]
    fn a_report_specific_field_becomes_a_column() {
        let csv = delimited(
            &flat(
                vec![Probe {
                    line: 3,
                    detail: "x",
                }],
                true,
            ),
            true,
        );
        let header = csv.lines().next().expect("header");
        assert!(header.ends_with(",detail"), "{header}");
    }

    #[test]
    fn code_climate_fingerprints_survive_a_line_shift() {
        let first = code_climate(&flat(
            vec![Probe {
                line: 3,
                detail: "x",
            }],
            true,
        ));
        let second = code_climate(&flat(
            vec![Probe {
                line: 9,
                detail: "x",
            }],
            true,
        ));
        assert_eq!(first[0]["fingerprint"], second[0]["fingerprint"]);
        assert_ne!(
            first[0]["location"]["lines"]["begin"],
            second[0]["location"]["lines"]["begin"]
        );
    }

    #[test]
    fn junit_counts_a_clean_modelled_file_as_a_passing_test() {
        let flattened = flat(Vec::new(), true);
        let xml = junit(&flattened);
        assert!(xml.contains("tests=\"1\""), "{xml}");
        assert!(xml.contains("failures=\"0\""), "{xml}");
    }

    #[test]
    fn markup_metacharacters_in_a_message_cannot_escape_their_cell() {
        let flattened = flat(
            vec![Probe {
                line: 1,
                detail: "<script>|`",
            }],
            true,
        );
        let page = html(&flattened);
        assert!(page.contains("&lt;script&gt;"), "{page}");
        assert!(!page.contains("<script>"), "{page}");
        let table = markdown(&flattened);
        assert!(table.contains("\\|"), "{table}");
    }
}
