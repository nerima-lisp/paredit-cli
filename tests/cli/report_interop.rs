//! The interoperability output formats the shared report envelope provides.
//!
//! These are asserted once, against one report, rather than per report: the
//! conversion happens in `paredit-core-cli`'s envelope and every report reaches
//! it by the same path, so a per-report copy of these assertions would test the
//! same 600 lines thirty-one times. What *is* worth checking per report is that
//! it accepts the formats at all, and `every_envelope_report_offers_the_interop_formats`
//! does that from the capability catalog rather than by enumeration.

use super::*;

const FIXTURE: &str = "(defun render-pane (limit)\n\
     \x20 ;; TODO(ada): cache the pane\n\
     \x20 (dotimes (i limit) i))\n";

fn fixture(name: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, FIXTURE).expect("write lisp fixture");
    file
}

fn todo_report(name: &str, format: &str) -> String {
    let assert = paredit()
        .args(["inspect", "todo", "--output", format])
        .arg(fixture(name))
        .assert()
        .success();
    String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 stdout")
}

#[test]
fn cli_report_sarif_locates_the_finding_and_names_its_rule() {
    let sarif = todo_report("interop-sarif", "sarif");
    let value: serde_json::Value = serde_json::from_str(&sarif).expect("sarif parses as json");

    assert_eq!(value["version"], "2.1.0");
    assert_eq!(value["runs"][0]["tool"]["driver"]["name"], "paredit");
    let result = &value["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "inspect/todo/TODO");
    assert_eq!(result["level"], "warning");
    assert_eq!(
        result["locations"][0]["physicalLocation"]["region"]["startLine"],
        2
    );
    // The rule the result references must be declared by the driver, or a
    // SARIF consumer has a dangling ruleId.
    let rules = value["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("driver rules");
    assert!(rules.iter().any(|rule| rule["id"] == "inspect/todo/TODO"));
}

#[test]
fn cli_report_junit_reports_one_testcase_per_finding() {
    let xml = todo_report("interop-junit", "junit");
    assert!(
        xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
        "{xml}"
    );
    assert!(xml.contains("tests=\"1\" failures=\"1\""), "{xml}");
    assert!(xml.contains("<failure message="), "{xml}");
    assert!(xml.contains("type=\"TODO\""), "{xml}");
}

#[test]
fn cli_report_code_climate_emits_a_bare_issue_array() {
    let json = todo_report("interop-code-climate", "code-climate");
    let value: serde_json::Value = serde_json::from_str(&json).expect("code climate parses");
    let issues = value.as_array().expect("a bare array, as GitLab expects");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["type"], "issue");
    assert_eq!(issues[0]["severity"], "minor");
    assert_eq!(issues[0]["location"]["lines"]["begin"], 2);
    assert!(
        issues[0]["fingerprint"]
            .as_str()
            .is_some_and(|hex| hex.len() == 64),
        "a blake3 fingerprint, so the panel can dedupe: {}",
        issues[0]
    );
}

#[test]
fn cli_report_csv_leads_with_a_header_row() {
    let csv = todo_report("interop-csv", "csv");
    let mut lines = csv.lines();
    let header = lines.next().expect("header row");
    assert!(
        header.starts_with("report,path,dialect,kind,severity,line,"),
        "{header}"
    );
    // The report's own JSON fields become trailing columns.
    assert!(header.contains(",marker"), "{header}");
    assert_eq!(lines.count(), 1, "one finding, one row: {csv}");
}

#[test]
fn cli_report_tsv_uses_the_same_columns_with_tabs() {
    let tsv = todo_report("interop-tsv", "tsv");
    let header = tsv.lines().next().expect("header row");
    assert_eq!(header.split('\t').next(), Some("report"));
    assert!(header.contains('\t'), "{header}");
    assert!(!header.contains(','), "{header}");
}

#[test]
fn cli_report_markdown_renders_a_table() {
    let markdown = todo_report("interop-markdown", "markdown");
    assert!(markdown.starts_with("# inspect todo"), "{markdown}");
    assert!(
        markdown.contains("| file | line | kind | severity | detail |"),
        "{markdown}"
    );
    assert!(markdown.contains("| `TODO` |"), "{markdown}");
}

#[test]
fn cli_report_html_is_a_standalone_document() {
    let html = todo_report("interop-html", "html");
    assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
    assert!(
        html.contains("<style>"),
        "no external asset to fetch: {html}"
    );
    assert!(html.contains("<td><code>TODO</code></td>"), "{html}");
}

/// An unmodelled dialect must be visible in every format, not silently absent.
///
/// This is the failure mode these formats invite: a CI panel showing zero
/// findings for a Fennel file that a Common Lisp-only report never looked at
/// reads as a clean bill of health.
#[test]
fn cli_report_interop_formats_disclose_an_unexamined_file() {
    let dir = fresh_temp_dir("interop-unmodelled");
    let file = dir.join("core.fnl");
    fs::write(&file, "(fn render [x] x)\n").expect("write fennel fixture");

    for (format, needle) in [
        ("sarif", "unmodelled-dialect"),
        ("junit", "<skipped"),
        ("csv", "unmodelled-dialect"),
        ("markdown", "Not examined"),
        ("html", "Not examined"),
    ] {
        paredit()
            .args(["inspect", "restarts", "--output", format])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains(needle));
    }
}

/// The gate's verdict has to reach a format that has somewhere to put it.
///
/// SARIF's `executionSuccessful` is that place. Without it a code-scanning
/// upload of a failed run is indistinguishable from a passing one, since the
/// non-zero exit status does not travel inside the log.
#[test]
fn cli_report_sarif_records_a_failed_gate_in_the_invocation() {
    let assert = paredit()
        .args(["inspect", "todo", "--output", "sarif", "--fail-on-marker"])
        .arg(fixture("interop-sarif-gate"))
        .assert()
        .code(3);
    let sarif: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("sarif parses");
    assert_eq!(
        sarif["runs"][0]["invocations"][0]["executionSuccessful"],
        false
    );
    assert_eq!(
        sarif["runs"][0]["invocations"][0]["properties"]["gate"],
        "--fail-on-marker"
    );
}

#[test]
fn cli_report_github_annotates_the_finding_line() {
    let annotations = todo_report("interop-github", "github");
    assert!(annotations.starts_with("::warning file="), "{annotations}");
    assert!(annotations.contains(",line=2,"), "{annotations}");
    assert!(
        annotations.contains("title=inspect todo TODO"),
        "{annotations}"
    );
}

/// A message holding an annotation delimiter must not be able to forge a
/// second workflow command or truncate the first.
#[test]
fn cli_report_github_escapes_the_annotation_delimiters() {
    let dir = fresh_temp_dir("interop-github-escape");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun f ()\n  ;; TODO: a,b::c 100%\n  nil)\n").expect("write lisp fixture");

    let assert = paredit()
        .args(["inspect", "todo", "--output", "github"])
        .arg(&file)
        .assert()
        .success();
    let annotations = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8");
    assert_eq!(annotations.lines().count(), 1, "{annotations}");
    assert!(annotations.contains("100%25"), "{annotations}");
}

/// `inspect lint` reaches the same formats through `--emit`.
///
/// A separate option from `--output` because lint's `--output` also governs a
/// dozen catalogue modes; this asserts the scan path routes through the shared
/// emitters and that the two older flags still mean what they meant.
#[test]
fn cli_lint_emit_renders_findings_in_the_interchange_formats() {
    let dir = fresh_temp_dir("interop-lint-emit");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun g (x) (if (not x) 1 2))\n").expect("write lisp fixture");

    for (format, needle) in [
        ("junit", "<testsuites name=\"inspect lint\""),
        ("code-climate", "\"type\": \"issue\""),
        ("csv", "report,path,dialect,kind,severity,line,"),
        ("tsv", "negated-if"),
        ("html", "<!DOCTYPE html>"),
        ("markdown", "# inspect lint"),
        ("github", "::warning file="),
        ("sarif", "\"version\": \"2.1.0\""),
    ] {
        paredit()
            .args(["inspect", "lint", "--emit", format])
            .arg(&file)
            .assert()
            .success()
            .stdout(predicate::str::contains(needle));
    }
}

#[test]
fn cli_lint_emit_sarif_matches_the_older_sarif_flag() {
    let dir = fresh_temp_dir("interop-lint-emit-sarif");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun g (x) (if (not x) 1 2))\n").expect("write lisp fixture");

    let by_flag = paredit()
        .args(["inspect", "lint", "--sarif"])
        .arg(&file)
        .assert()
        .success();
    let by_emit = paredit()
        .args(["inspect", "lint", "--emit", "sarif"])
        .arg(&file)
        .assert()
        .success();
    assert_eq!(by_flag.get_output().stdout, by_emit.get_output().stdout);
}

#[test]
fn cli_lint_emit_carries_the_gate_verdict_into_the_exit_status() {
    let dir = fresh_temp_dir("interop-lint-emit-gate");
    let file = dir.join("core.lisp");
    fs::write(&file, "(defun g (x) (if (not x) 1 2))\n").expect("write lisp fixture");

    paredit()
        .args(["inspect", "lint", "--emit", "csv", "--fail-on-finding"])
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("negated-if"));
}

/// Every report that goes through the envelope must advertise the whole format
/// set, and every report that does not must advertise none of it.
///
/// Read from `inspect capabilities` rather than from a list here, because the
/// catalog is what an agent reads. A report wired to the envelope but left on
/// the two-value `--output` would look, to an agent, like a report that cannot
/// produce SARIF — and it would be right, which is the bug.
#[test]
fn cli_capabilities_reports_the_interop_formats_per_command() {
    let assert = paredit()
        .args(["inspect", "capabilities", "--output", "json"])
        .assert()
        .success();
    let catalog: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("catalog parses");

    let inspect = catalog["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|command| command["name"] == "inspect")
        .expect("the inspect namespace");

    let formats = |leaf: &str| -> Vec<String> {
        inspect["commands"]
            .as_array()
            .expect("leaves")
            .iter()
            .find(|command| command["name"] == leaf)
            .unwrap_or_else(|| panic!("no `inspect {leaf}` in the catalog"))["args"]
            .as_array()
            .expect("args")
            .iter()
            .find(|arg| arg["id"] == "output")
            .expect("an --output argument")["possible_values"]
            .as_array()
            .expect("possible values")
            .iter()
            .map(|value| value.as_str().unwrap_or_default().to_owned())
            .collect()
    };

    const INTEROP: [&str; 10] = [
        "text",
        "json",
        "sarif",
        "junit",
        "code-climate",
        "csv",
        "tsv",
        "html",
        "markdown",
        "github",
    ];
    assert_eq!(formats("todo"), INTEROP);
    assert_eq!(formats("restarts"), INTEROP);
    // `outline` prints a tree, not a finding list. It keeps the two-value
    // `--output` so the catalog does not promise a SARIF it cannot produce.
    assert_eq!(formats("outline"), vec!["text", "json"]);
}
