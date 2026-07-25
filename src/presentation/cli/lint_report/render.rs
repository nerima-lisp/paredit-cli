use anyhow::Result;
use serde_json::json;

use crate::application::usecase::lint_report::{
    CATEGORIES, LintPolicy, LintSummary, RULE_DOCS, rule_category, rule_description,
    rule_is_fixable, rule_severity,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_lint_report(
    summary: &LintSummary,
    policy: &LintPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("finding_count\t{}", summary.finding_count);
            for (rule, count) in &summary.per_rule {
                println!("rule\t{}\t{}", rule, count);
            }
            if policy.fail_on_finding {
                println!("policy\tfail_on_finding=true\tpassed={}", policy.passed);
            }
            for finding in &summary.findings {
                println!(
                    "finding\t{}\t{}\t{}\tfixable={}\t{}\t{}\t{}",
                    safe_text!(finding.rule),
                    rule_severity(finding.rule).as_str(),
                    rule_category(finding.rule).unwrap_or(""),
                    rule_is_fixable(finding.rule),
                    safe_text!(finding.path.display()),
                    finding.span.start().get(),
                    safe_text!(finding.message),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "finding_count": summary.finding_count,
                    "per_rule": summary.per_rule
                        .iter()
                        .map(|(rule, count)| json!({
                            "rule": rule,
                            "count": count,
                            "category": rule_category(rule),
                            "description": rule_description(rule),
                        }))
                        .collect::<Vec<_>>(),
                    "policy": {
                        "fail_on_finding": policy.fail_on_finding,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "findings": summary.findings
                        .iter()
                        .map(|finding| json!({
                            "rule": finding.rule,
                            "severity": rule_severity(finding.rule).as_str(),
                            "category": rule_category(finding.rule),
                            "fixable": rule_is_fixable(finding.rule),
                            "path": finding.path.display().to_string(),
                            "span": {
                                "start": finding.span.start().get(),
                                "end": finding.span.end().get(),
                            },
                            "message": &finding.message,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Prints the catalog of lint rules with their category and description, for
/// `inspect lint --list-rules`. Only the rules in `active` are listed, so the
/// same `--rule`/`--exclude`/`--category` selectors that pick rules for scanning
/// also narrow the catalog (e.g. `--list-rules --category dead-code`). With no
/// selectors, `active` is every rule. The rule names are exactly the values
/// accepted by `--rule`/`--exclude`, and the categories those accepted by
/// `--category`.
pub(super) fn print_lint_rule_catalog(active: &[&str], output: OutputFormat) -> Result<()> {
    let listed: Vec<&(&str, &str, &str)> = RULE_DOCS
        .iter()
        .filter(|(rule, _, _)| active.contains(rule))
        .collect();
    match output {
        OutputFormat::Text => {
            println!("rule_count\t{}", listed.len());
            println!("categories\t{}", CATEGORIES.join(","));
            for (rule, category, description) in &listed {
                println!(
                    "rule\t{}\t{}\t{}\tfixable={}\t{}",
                    safe_text!(rule),
                    safe_text!(category),
                    rule_severity(rule).as_str(),
                    rule_is_fixable(rule),
                    safe_text!(description),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "rule_count": listed.len(),
                    "categories": CATEGORIES,
                    "rules": listed
                        .iter()
                        .map(|(rule, category, description)| json!({
                            "rule": rule,
                            "category": category,
                            "severity": rule_severity(rule).as_str(),
                            "description": description,
                            "fixable": rule_is_fixable(rule),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// A rollup of finding counts for `inspect lint --stats`.
pub(super) struct LintStats {
    pub finding_count: usize,
    pub files_scanned: usize,
    pub files_with_findings: usize,
    /// `("error", n)` then `("warning", m)`.
    pub by_severity: Vec<(&'static str, usize)>,
    /// One entry per category, in [`CATEGORIES`] order.
    pub by_category: Vec<(&'static str, usize)>,
    /// Only rules with findings, most-frequent first.
    pub by_rule: Vec<(&'static str, usize)>,
}

/// Prints the `--stats` rollup: totals plus per-severity, per-category, and
/// per-rule counts, so lint debt can be gauged and prioritized at a glance.
pub(super) fn print_lint_stats(stats: &LintStats, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("finding_count\t{}", stats.finding_count);
            println!("files_scanned\t{}", stats.files_scanned);
            println!("files_with_findings\t{}", stats.files_with_findings);
            for (severity, count) in &stats.by_severity {
                println!("severity\t{severity}\t{count}");
            }
            for (category, count) in &stats.by_category {
                println!("category\t{category}\t{count}");
            }
            for (rule, count) in &stats.by_rule {
                println!("rule\t{rule}\t{count}");
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "finding_count": stats.finding_count,
                    "files_scanned": stats.files_scanned,
                    "files_with_findings": stats.files_with_findings,
                    "by_severity": stats.by_severity
                        .iter()
                        .map(|(severity, count)| json!({ "severity": severity, "count": count }))
                        .collect::<Vec<_>>(),
                    "by_category": stats.by_category
                        .iter()
                        .map(|(category, count)| json!({ "category": category, "count": count }))
                        .collect::<Vec<_>>(),
                    "by_rule": stats.by_rule
                        .iter()
                        .map(|(rule, count)| json!({ "rule": rule, "count": count }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Reports inline `; paredit:ignore` directives that silenced no finding, so a
/// stale ignore or a typo'd rule name can be removed. `entries` pairs each
/// file path with one unused directive.
pub(super) fn print_lint_unused_suppressions(
    entries: &[(
        String,
        crate::application::usecase::lint_report::UnusedSuppression,
    )],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("unused_suppression_count\t{}", entries.len());
            for (path, entry) in entries {
                let rules = match &entry.unused_rules {
                    None => "*".to_owned(),
                    Some(rules) => rules.join(","),
                };
                println!(
                    "unused\t{}\t{}\t{}\t{}",
                    safe_text!(path),
                    entry.comment_line,
                    entry.target_line,
                    safe_text!(rules),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "unused_suppression_count": entries.len(),
                    "unused_suppressions": entries
                        .iter()
                        .map(|(path, entry)| json!({
                            "path": path,
                            "comment_line": entry.comment_line,
                            "target_line": entry.target_line,
                            // null => a bare directive; otherwise the stale rules.
                            "rules": entry.unused_rules,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// One byte-region edit within a fix: replace `[byte_offset, byte_offset +
/// byte_length)` with `text`.
#[derive(Clone)]
pub(super) struct LintReplacement {
    pub byte_offset: usize,
    pub byte_length: usize,
    pub text: String,
}

/// An automatic fix for a finding: one or more byte-region edits applied
/// together. Most rules need a single edit; some (e.g. flipping `when`/`unless`
/// while unwrapping its negated test) need several disjoint edits at once.
/// Surfaced as a SARIF `fix` so consumers (e.g. GitHub code scanning) can offer
/// a one-click apply.
#[derive(Clone)]
pub(super) struct LintFix {
    pub description: String,
    pub replacements: Vec<LintReplacement>,
}

/// The per-file outcome of an `inspect lint --fix` run: how many auto-fixes
/// were applied to `path` and the per-rule breakdown. Only files that actually
/// changed are reported.
pub(super) struct LintFileFix {
    pub path: String,
    pub applied: usize,
    pub per_rule: Vec<(&'static str, usize)>,
}

/// Reports the outcome of applying auto-fixes in place: the grand total of
/// fixes applied, how many files changed, and the per-file/per-rule breakdown.
pub(super) fn print_lint_fix_report(
    files: &[LintFileFix],
    fixes_applied: usize,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("fixes_applied\t{fixes_applied}");
            println!("files_changed\t{}", files.len());
            for file in files {
                for (rule, count) in &file.per_rule {
                    println!(
                        "fix\t{}\t{}\t{}",
                        safe_text!(file.path),
                        safe_text!(rule),
                        count,
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "fixes_applied": fixes_applied,
                    "files_changed": files.len(),
                    "files": files
                        .iter()
                        .map(|file| json!({
                            "path": file.path,
                            "applied": file.applied,
                            "per_rule": file.per_rule
                                .iter()
                                .map(|(rule, count)| json!({ "rule": rule, "count": count }))
                                .collect::<Vec<_>>(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// One SARIF result: a single finding located in a source file, with its
/// 1-based line/column (computed by the workflow), its byte span, a
/// line-content fingerprint that stays stable when unrelated lines shift, and
/// an optional automatic fix.
pub(super) struct LintSarifResult {
    pub rule: &'static str,
    pub message: String,
    pub path: String,
    pub start_line: usize,
    pub start_column: usize,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub fingerprint: String,
    pub fix: Option<LintFix>,
}

/// Prints lint findings as a SARIF 2.1.0 log for CI code-scanning ingestion.
/// The driver advertises every rule (id, category, description) from
/// [`RULE_DOCS`]; each result references its `ruleId` and physical location.
pub(super) fn print_lint_sarif(results: &[LintSarifResult]) -> Result<()> {
    let rules = RULE_DOCS
        .iter()
        .map(|(rule, category, description)| {
            json!({
                "id": rule,
                "shortDescription": { "text": description },
                "properties": {
                    "category": category,
                    "fixable": rule_is_fixable(rule),
                    "severity": rule_severity(rule).as_str(),
                },
            })
        })
        .collect::<Vec<_>>();

    let sarif_results = results
        .iter()
        .map(|result| {
            let mut value = json!({
                "ruleId": result.rule,
                "level": rule_severity(result.rule).as_str(),
                "message": { "text": result.message },
                "partialFingerprints": {
                    "primaryLocationLineHash": result.fingerprint,
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": result.path },
                        "region": {
                            "startLine": result.start_line,
                            "startColumn": result.start_column,
                            "byteOffset": result.byte_offset,
                            "byteLength": result.byte_length,
                        },
                    },
                }],
            });
            if let Some(fix) = &result.fix {
                let replacements = fix
                    .replacements
                    .iter()
                    .map(|replacement| {
                        json!({
                            "deletedRegion": {
                                "byteOffset": replacement.byte_offset,
                                "byteLength": replacement.byte_length,
                            },
                            "insertedContent": { "text": replacement.text },
                        })
                    })
                    .collect::<Vec<_>>();
                value["fixes"] = json!([{
                    "description": { "text": fix.description },
                    "artifactChanges": [{
                        "artifactLocation": { "uri": result.path },
                        "replacements": replacements,
                    }],
                }]);
            }
            value
        })
        .collect::<Vec<_>>();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "version": "2.1.0",
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "runs": [{
                "tool": { "driver": { "name": "paredit", "rules": rules } },
                "results": sarif_results,
            }],
        }))?
    );

    Ok(())
}

/// One entry in the `--fix-plan`: a fixable finding and the exact byte-region
/// edits that would repair it, without applying them. The byte offsets are into
/// the file at `path` as read, so a consumer can apply a single fix in isolation
/// (then re-run to expose any fixes the first unlocks).
pub(super) struct LintFixPlanEntry {
    pub rule: &'static str,
    pub path: String,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub fix: LintFix,
}

/// Prints the machine-readable fix plan: every fixable finding's replacements,
/// so an editor or agent can preview or apply fixes one at a time without the
/// in-place `--fix` run. Mirrors the SARIF `fixes` payload in the plain schema.
pub(super) fn print_lint_fix_plan(
    entries: &[LintFixPlanEntry],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("fix_count\t{}", entries.len());
            for entry in entries {
                for replacement in &entry.fix.replacements {
                    println!(
                        "fix\t{}\t{}\t{}\t{}\t{}",
                        safe_text!(entry.path),
                        safe_text!(entry.rule),
                        replacement.byte_offset,
                        replacement.byte_length,
                        safe_text!(replacement.text),
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "fix_count": entries.len(),
                    "fixes": entries
                        .iter()
                        .map(|entry| json!({
                            "rule": entry.rule,
                            "path": entry.path,
                            "description": entry.fix.description,
                            "span": {
                                "start": entry.byte_offset,
                                "end": entry.byte_offset + entry.byte_length,
                            },
                            "replacements": entry.fix.replacements
                                .iter()
                                .map(|replacement| json!({
                                    "byte_offset": replacement.byte_offset,
                                    "byte_length": replacement.byte_length,
                                    "text": replacement.text,
                                }))
                                .collect::<Vec<_>>(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Escapes a GitHub Actions annotation *data* value (the message): percent,
/// carriage return, and newline must be percent-encoded so the runner does not
/// misparse the command.
fn escape_annotation_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a GitHub Actions annotation *property* value (e.g. `file=`), which
/// additionally encodes the `,` and `:` that delimit the command's properties.
fn escape_annotation_property(value: &str) -> String {
    escape_annotation_data(value)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

/// Prints one GitHub Actions `::error::` annotation for a finding. The runner
/// renders these inline on the pull request diff at the given file/line/column.
pub(super) fn print_lint_github_annotation(
    path: &str,
    line: usize,
    column: usize,
    rule: &str,
    message: &str,
) {
    println!(
        "::error file={},line={},col={}::{}",
        escape_annotation_property(path),
        line,
        column,
        escape_annotation_data(&format!("{rule}: {message}")),
    );
}
