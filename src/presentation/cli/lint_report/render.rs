use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::json;

use crate::application::usecase::lint_report::{
    CATEGORIES, LintPolicy, LintSummary, RULE_DOCS, RULES, RulePreset, RuleTag, Severity,
    rule_category, rule_description, rule_dialects, rule_explanation, rule_is_fixable,
    rule_settings, rule_severity, rule_tags,
};
use crate::presentation::cli::OutputFormat;
use crate::presentation::cli::lint_report::custom::{CustomRules, RuleMetaResolver};

/// The stable identity of each finding in a summary, keyed by its position.
///
/// Computed by the workflow (which holds each file's source) and handed to the
/// renderer, because the id is derived from the reported form's text and a
/// `LintFinding` carries only a span.
pub(super) type FindingIds = Vec<String>;

pub(super) fn print_lint_report(
    summary: &LintSummary,
    policy: &LintPolicy,
    ids: &FindingIds,
    meta: &RuleMetaResolver<'_>,
    output: OutputFormat,
) -> Result<()> {
    let severity_of = |rule: &str| meta.severity(rule).as_str();
    match output {
        OutputFormat::Text => {
            println!("finding_count\t{}", summary.finding_count);
            for (rule, count) in &summary.per_rule {
                println!("rule\t{rule}\t{count}");
            }
            if policy.fail_on_finding {
                println!("policy\tfail_on_finding=true\tpassed={}", policy.passed);
            }
            for (index, finding) in summary.findings.iter().enumerate() {
                println!(
                    "finding\t{}\t{}\t{}\tfixable={}\t{}\t{}\t{}\t{}",
                    safe_text!(finding.rule),
                    severity_of(finding.rule),
                    meta.category(finding.rule).unwrap_or(""),
                    meta.is_fixable(finding.rule),
                    safe_text!(finding.path.display()),
                    finding.span.start().get(),
                    safe_text!(finding.message),
                    safe_text!(ids.get(index).map_or("", String::as_str)),
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
                            "category": meta.category(rule),
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
                        .enumerate()
                        .map(|(index, finding)| json!({
                            // A content-derived identity that survives
                            // reindentation and unrelated edits above it, for
                            // baselines and suppression tooling.
                            "id": ids.get(index),
                            "rule": finding.rule,
                            "severity": severity_of(finding.rule),
                            "category": meta.category(finding.rule),
                            "fixable": meta.is_fixable(finding.rule),
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

/// Prints everything known about one rule, for `--explain <rule>`.
///
/// The metadata block is always present; the rationale, example, and caveat
/// appear only for the rules that declare them. A rule that declares none is
/// still worth explaining — the dialect list alone answers the most common
/// "why did this find nothing?" — so the command never refuses for want of
/// prose.
pub(super) fn print_lint_explanation(rule: &str, output: OutputFormat) -> Result<()> {
    let explanation = rule_explanation(rule);
    let tags = rule_tags(rule).names();
    let dialects = rule_dialects(rule);
    let settings = rule_settings(rule);
    let description = rule_description(rule).unwrap_or_default();

    match output {
        OutputFormat::Text => {
            println!("rule\t{}", safe_text!(rule));
            println!("category\t{}", rule_category(rule).unwrap_or(""));
            println!("severity\t{}", rule_severity(rule).as_str());
            println!("fixable\t{}", rule_is_fixable(rule));
            println!("tags\t{}", tags.join(","));
            println!("dialects\t{}", dialects.join(","));
            println!("description\t{}", safe_text!(description));
            if let Some(explanation) = explanation {
                println!("rationale\t{}", safe_text!(explanation.rationale()));
                if let Some(example) = explanation.example() {
                    println!("example_bad\t{}", safe_text!(example.bad()));
                    println!("example_good\t{}", safe_text!(example.good()));
                }
                if !explanation.caveat().is_empty() {
                    println!("caveat\t{}", safe_text!(explanation.caveat()));
                }
            }
            for setting in settings {
                println!(
                    "setting\t{}\t{}\t{}",
                    safe_text!(setting.key()),
                    setting.default(),
                    safe_text!(setting.description()),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "rule": rule,
                    "category": rule_category(rule),
                    "severity": rule_severity(rule).as_str(),
                    "fixable": rule_is_fixable(rule),
                    "tags": tags,
                    "dialects": dialects,
                    "description": description,
                    "rationale": explanation.map(|e| e.rationale()),
                    "example": explanation.and_then(|e| e.example()).map(|example| json!({
                        "bad": example.bad(),
                        "good": example.good(),
                    })),
                    "caveat": explanation
                        .map(|e| e.caveat())
                        .filter(|caveat| !caveat.is_empty()),
                    "settings": settings
                        .iter()
                        .map(|setting| json!({
                            "key": setting.key(),
                            "default": setting.default(),
                            "description": setting.description(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Prints the `--preset` ladder with how many rules each rung admits.
pub(super) fn print_lint_presets(
    counts: &[(RulePreset, usize)],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            for (preset, count) in counts {
                println!(
                    "preset\t{}\t{count}\t{}",
                    preset.as_str(),
                    safe_text!(preset.description()),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "default": RulePreset::default().as_str(),
                    "presets": counts
                        .iter()
                        .map(|(preset, count)| json!({
                            "preset": preset.as_str(),
                            "rule_count": count,
                            "description": preset.description(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Prints the `--tag` values with the rules carrying each.
pub(super) fn print_lint_tags(output: OutputFormat) -> Result<()> {
    let tagged: Vec<(RuleTag, Vec<&'static str>)> = RuleTag::ALL
        .into_iter()
        .map(|tag| {
            let rules: Vec<&'static str> = RULES
                .iter()
                .copied()
                .filter(|rule| rule_tags(rule).contains(tag))
                .collect();
            (tag, rules)
        })
        .collect();

    match output {
        OutputFormat::Text => {
            for (tag, rules) in &tagged {
                println!(
                    "tag\t{}\t{}\t{}",
                    tag.as_str(),
                    rules.len(),
                    rules.join(",")
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "tags": tagged
                        .iter()
                        .map(|(tag, rules)| json!({
                            "tag": tag.as_str(),
                            "rule_count": rules.len(),
                            "rules": rules,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Writes the whole rule reference as Markdown, one section per rule grouped by
/// category.
///
/// Generated rather than hand-written so a rule cannot ship with metadata the
/// documentation contradicts: everything below is read from the same `RULE_DOCS`
/// the report and `--list-rules` read. The output is the document, so it ignores
/// `--output` and goes to stdout for redirection.
pub(super) fn print_lint_docs(active: &[&str]) -> Result<()> {
    println!("# Lint rules");
    println!();
    println!(
        "{} rules, generated from the registry. Every heading is a `--rule` value \
         and every subheading group is a `--category` value.",
        active.len()
    );

    let mut by_category: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for rule in active {
        by_category
            .entry(rule_category(rule).unwrap_or("uncategorized"))
            .or_default()
            .push(rule);
    }

    for (category, rules) in &by_category {
        println!();
        println!("## {category}");
        for rule in rules {
            println!();
            println!("### `{rule}`");
            println!();
            println!("{}", rule_description(rule).unwrap_or_default());
            println!();
            println!("| | |");
            println!("| --- | --- |");
            println!("| severity | `{}` |", rule_severity(rule).as_str());
            println!(
                "| auto-fixable | {} |",
                if rule_is_fixable(rule) { "yes" } else { "no" }
            );
            let tags = rule_tags(rule).names();
            if !tags.is_empty() {
                println!("| tags | {} |", tags.join(", "));
            }
            println!("| dialects | {} |", rule_dialects(rule).join(", "));

            if let Some(explanation) = rule_explanation(rule) {
                println!();
                println!("{}", explanation.rationale());
                if let Some(example) = explanation.example() {
                    println!();
                    println!("```lisp");
                    println!(";; reported");
                    println!("{}", example.bad());
                    println!();
                    println!(";; instead");
                    println!("{}", example.good());
                    println!("```");
                }
                if !explanation.caveat().is_empty() {
                    println!();
                    println!("**Not reported:** {}", explanation.caveat());
                }
            }

            let settings = rule_settings(rule);
            if !settings.is_empty() {
                println!();
                println!("| setting | default | meaning |");
                println!("| --- | --- | --- |");
                for setting in settings {
                    println!(
                        "| `--rule-arg {rule}.{}=…` | `{}` | {} |",
                        setting.key(),
                        setting.default(),
                        setting.description(),
                    );
                }
            }
        }
    }

    Ok(())
}

/// One rule's measured cost, for `--timings`.
pub(super) struct LintTiming {
    pub rule: &'static str,
    pub micros: u128,
    pub invocations: u64,
    /// Share of the measured total, as a percentage rounded to two decimals.
    pub share: f64,
}

/// Prints the per-rule cost breakdown, slowest first.
pub(super) fn print_lint_timings(
    timings: &[LintTiming],
    total_micros: u128,
    files_scanned: usize,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("total_micros\t{total_micros}");
            println!("files_scanned\t{files_scanned}");
            for timing in timings {
                println!(
                    "rule\t{}\t{}\t{}\t{:.2}",
                    safe_text!(timing.rule),
                    timing.micros,
                    timing.invocations,
                    timing.share,
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "total_micros": total_micros,
                    "files_scanned": files_scanned,
                    "rules": timings
                        .iter()
                        .map(|timing| json!({
                            "rule": timing.rule,
                            "micros": timing.micros,
                            "invocations": timing.invocations,
                            "share_percent": timing.share,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// One file's suppression cleanup, for `--remove-unused-suppressions`.
pub(super) struct LintSuppressionRemoval {
    pub path: String,
    pub removed: usize,
    /// `(comment line, the rule names dropped)`; empty names for a bare
    /// directive.
    pub directives: Vec<(usize, Vec<String>)>,
}

/// Reports the stale `paredit:ignore` directives deleted or narrowed in place.
pub(super) fn print_lint_suppression_removal(
    files: &[LintSuppressionRemoval],
    removed: usize,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("suppressions_removed\t{removed}");
            println!("files_changed\t{}", files.len());
            for file in files {
                for (line, rules) in &file.directives {
                    println!(
                        "removed\t{}\t{line}\t{}",
                        safe_text!(file.path),
                        safe_text!(if rules.is_empty() {
                            "*".to_owned()
                        } else {
                            rules.join(",")
                        }),
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "suppressions_removed": removed,
                    "files_changed": files.len(),
                    "files": files
                        .iter()
                        .map(|file| json!({
                            "path": file.path,
                            "removed": file.removed,
                            "directives": file.directives
                                .iter()
                                .map(|(line, rules)| json!({
                                    "comment_line": line,
                                    // null => a bare directive, removed whole.
                                    "rules": (!rules.is_empty()).then_some(rules),
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

/// Prints the catalog of lint rules with their category and description, for
/// `inspect lint --list-rules`. Only the rules in `active` are listed, so the
/// same `--rule`/`--exclude`/`--category` selectors that pick rules for scanning
/// also narrow the catalog (e.g. `--list-rules --category dead-code`). With no
/// selectors, `active` is every rule. The rule names are exactly the values
/// accepted by `--rule`/`--exclude`, and the categories those accepted by
/// `--category`.
pub(super) fn print_lint_rule_catalog(
    active: &[&str],
    custom: &CustomRules,
    output: OutputFormat,
) -> Result<()> {
    let listed: Vec<&(&str, &str, &str)> = RULE_DOCS
        .iter()
        .filter(|(rule, _, _)| active.contains(rule))
        .collect();
    // A project's own rules are listed beside the shipped ones, tagged so the
    // two are distinguishable: a caller asking "what will run?" is asking about
    // both, and a caller asking "what shipped?" needs to be able to tell.
    let project: Vec<(&'static str, &'static str, String, Severity, bool)> = custom
        .catalog()
        .map(|(name, category, description, severity, fixable)| {
            (name, category, description.to_owned(), severity, fixable)
        })
        .collect();
    match output {
        OutputFormat::Text => {
            println!("rule_count\t{}", listed.len());
            println!("custom_rule_count\t{}", project.len());
            println!("categories\t{}", CATEGORIES.join(","));
            for (rule, category, description) in &listed {
                println!(
                    "rule\t{}\t{}\t{}\tfixable={}\ttags={}\t{}",
                    safe_text!(rule),
                    safe_text!(category),
                    rule_severity(rule).as_str(),
                    rule_is_fixable(rule),
                    safe_text!(rule_tags(rule).names().join(",")),
                    safe_text!(description),
                );
            }
            for (rule, category, description, severity, fixable) in &project {
                println!(
                    "custom-rule\t{}\t{}\t{}\tfixable={}\t{}",
                    safe_text!(rule),
                    safe_text!(category),
                    severity.as_str(),
                    fixable,
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
                    "custom_rules": project
                        .iter()
                        .map(|(rule, category, description, severity, fixable)| json!({
                            "rule": rule,
                            "category": category,
                            "severity": severity.as_str(),
                            "description": description,
                            "fixable": fixable,
                        }))
                        .collect::<Vec<_>>(),
                    "categories": CATEGORIES,
                    "rules": listed
                        .iter()
                        .map(|(rule, category, description)| json!({
                            "rule": rule,
                            "category": category,
                            "severity": rule_severity(rule).as_str(),
                            "description": description,
                            "fixable": rule_is_fixable(rule),
                            "tags": rule_tags(rule).names(),
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
                    "unused\t{}\t{}\t{}\t{}\t{}\t{}",
                    safe_text!(path),
                    entry.comment_line,
                    entry.target_line,
                    safe_text!(rules),
                    entry.scope.as_str(),
                    entry.problem.as_str(),
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
                            "scope": entry.scope.as_str(),
                            "problem": entry.problem.as_str(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Reports inline suppression directives whose `-until` date has passed, for
/// `--report-expired-suppressions`. Shaped like
/// [`print_lint_unused_suppressions`] — same carrier struct, a different
/// problem — but kept as its own function rather than a shared one so
/// neither's field names depend on the other's.
pub(super) fn print_lint_expired_suppressions(
    entries: &[(
        String,
        crate::application::usecase::lint_report::UnusedSuppression,
    )],
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("expired_suppression_count\t{}", entries.len());
            for (path, entry) in entries {
                let rules = match &entry.unused_rules {
                    None => "*".to_owned(),
                    Some(rules) => rules.join(","),
                };
                println!(
                    "expired\t{}\t{}\t{}\t{}\t{}",
                    safe_text!(path),
                    entry.comment_line,
                    entry.target_line,
                    safe_text!(rules),
                    entry.scope.as_str(),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "expired_suppression_count": entries.len(),
                    "expired_suppressions": entries
                        .iter()
                        .map(|(path, entry)| json!({
                            "path": path,
                            "comment_line": entry.comment_line,
                            "target_line": entry.target_line,
                            // null => a bare directive; otherwise the named rules.
                            "rules": entry.unused_rules,
                            "scope": entry.scope.as_str(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }

    Ok(())
}

/// Lists every inline suppression directive, used or not, for
/// `--report-suppressions` — the full survey, one step past
/// `--report-unused-suppressions`'s stale-only view.
pub(super) fn print_lint_suppression_inventory(
    entries: &[(
        String,
        crate::application::usecase::lint_report::SuppressionInventoryEntry,
    )],
    output: OutputFormat,
) -> Result<()> {
    let unused_count = entries.iter().filter(|(_, entry)| !entry.used).count();
    match output {
        OutputFormat::Text => {
            println!("suppression_count\t{}", entries.len());
            println!("unused_count\t{unused_count}");
            for (path, entry) in entries {
                let rules = match &entry.rules {
                    None => "*".to_owned(),
                    Some(rules) => rules.join(","),
                };
                println!(
                    "suppression\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    safe_text!(path),
                    entry.comment_line,
                    entry.target_line,
                    safe_text!(rules),
                    entry.scope.as_str(),
                    entry.used,
                    safe_text!(entry.reason.as_deref().unwrap_or("")),
                    safe_text!(entry.expires.as_deref().unwrap_or("")),
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "suppression_count": entries.len(),
                    "unused_count": unused_count,
                    "suppressions": entries
                        .iter()
                        .map(|(path, entry)| json!({
                            "path": path,
                            "comment_line": entry.comment_line,
                            "target_line": entry.target_line,
                            "target_end_line": entry.target_end_line,
                            "rules": entry.rules,
                            "scope": entry.scope.as_str(),
                            "used": entry.used,
                            "reason": entry.reason,
                            "expires": entry.expires,
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
    deferred: usize,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("fixes_applied\t{fixes_applied}");
            println!("files_changed\t{}", files.len());
            println!("fixes_deferred\t{deferred}");
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
                    // Fixes an overlapping one shadowed on the final pass. A
                    // non-zero count means re-running would apply more; it is
                    // not an error, and the fixpoint loop already re-ran until
                    // it stopped shrinking.
                    "fixes_deferred": deferred,
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
    /// The content-derived finding id, published as a second partial
    /// fingerprint so a consumer can correlate a SARIF result with the same
    /// finding in the JSON report or a baseline.
    pub finding_id: String,
    pub fix: Option<LintFix>,
}

/// Prints lint findings as a SARIF 2.1.0 log for CI code-scanning ingestion.
/// The driver advertises every rule (id, category, description) from
/// [`RULE_DOCS`]; each result references its `ruleId` and physical location.
pub(super) fn print_lint_sarif(
    results: &[LintSarifResult],
    meta: &RuleMetaResolver<'_>,
) -> Result<()> {
    let rules = RULE_DOCS
        .iter()
        .map(|(rule, category, description)| {
            let explanation = rule_explanation(rule);
            let mut properties = json!({
                "category": category,
                "fixable": rule_is_fixable(rule),
                "severity": meta.severity(rule).as_str(),
                "tags": rule_tags(rule).names(),
            });
            // SARIF's `fullDescription` is exactly the long-form text
            // `--explain` prints, so a code-scanning UI shows the same
            // rationale the CLI does rather than repeating the one-liner.
            if let Some(explanation) = explanation {
                properties["rationale"] = json!(explanation.rationale());
            }
            let mut value = json!({
                "id": rule,
                "shortDescription": { "text": description },
                "properties": properties,
            });
            if let Some(explanation) = explanation {
                value["fullDescription"] = json!({ "text": explanation.rationale() });
            }
            value
        })
        .collect::<Vec<_>>();

    let sarif_results = results
        .iter()
        .map(|result| {
            let mut value = json!({
                "ruleId": result.rule,
                "level": meta.severity(result.rule).as_str(),
                "message": { "text": result.message },
                "partialFingerprints": {
                    "primaryLocationLineHash": result.fingerprint,
                    "pareditFindingId": result.finding_id,
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
    /// The finding's content-derived id, so an agent applying one entry at a
    /// time can name which finding it just repaired.
    pub finding_id: String,
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
                            "id": entry.finding_id,
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

/// Prints one GitHub Actions annotation for a finding. The runner renders these
/// inline on the pull request diff at the given file/line/column.
///
/// The command word follows the finding's severity — as overridden by
/// `--deny`/`--warn`, not as shipped — because that is the whole observable
/// difference the flags make in this output mode: GitHub renders `::warning`
/// annotations without failing the check.
pub(super) fn print_lint_github_annotation(
    path: &str,
    line: usize,
    column: usize,
    rule: &str,
    message: &str,
    severity: Severity,
) {
    let command = match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    println!(
        "::{command} file={},line={},col={}::{}",
        escape_annotation_property(path),
        line,
        column,
        escape_annotation_data(&format!("{rule}: {message}")),
    );
}
