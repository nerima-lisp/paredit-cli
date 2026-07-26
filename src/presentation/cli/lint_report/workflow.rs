use anyhow::{Context, Result};

use crate::application::usecase::lint_report::{
    CATEGORIES, LintFinding, LintPolicyOptions, LintSuppressions, Severity, collect_lint_findings,
    collect_lint_fixes_for, evaluate_lint_policy, lint_gate_violations, resolve_active_rules,
    rule_category, rule_severity, summarize_lint_findings,
};
use crate::domain::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use crate::presentation::cli::lint_report::args::LintReportArgs;
use crate::presentation::cli::lint_report::baseline::{BaselineEntry, LintBaseline};
use crate::presentation::cli::lint_report::render::{
    LintFileFix, LintFix, LintFixPlanEntry, LintReplacement, LintSarifResult, LintStats,
    print_lint_fix_plan, print_lint_fix_report, print_lint_github_annotation, print_lint_report,
    print_lint_rule_catalog, print_lint_sarif, print_lint_stats, print_lint_unused_suppressions,
};
use crate::presentation::cli::shared::{
    apply_byte_span_edits, expand_input_files, read_input_dialect_and_tree, stable_text_hash,
    unified_diff, write_file_with_rollback,
};

/// The 1-based line and byte-based column of a byte offset in `text`.
fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(text.len());
    let bytes = text.as_bytes();
    let mut line = 1;
    let mut last_newline = None;
    for (index, byte) in bytes.iter().enumerate().take(clamped) {
        if *byte == b'\n' {
            line += 1;
            last_newline = Some(index);
        }
    }
    let column = match last_newline {
        Some(newline) => clamped - newline,
        None => clamped + 1,
    };
    (line, column)
}

/// A SARIF `primaryLocationLineHash` for a finding: a hash of the rule and the
/// trimmed content of its source line, so it stays stable when unrelated lines
/// are inserted or removed above it. An occurrence suffix disambiguates two
/// findings that share the same rule and line text.
fn line_fingerprint(
    rule: &str,
    text: &str,
    line: usize,
    seen: &mut std::collections::HashMap<String, usize>,
) -> String {
    let line_text = text
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim();
    let base = stable_text_hash(&format!("{rule}\n{line_text}"));
    let index = seen.entry(base.clone()).or_insert(0);
    let fingerprint = format!("{base}:{index}");
    *index += 1;
    fingerprint
}

/// Drops findings silenced by an inline `; paredit:ignore` directive in the
/// file's own source, so a suppression comment applies uniformly across every
/// output mode (report, SARIF, GitHub, and fix).
fn retain_unsuppressed(findings: Vec<LintFinding>, text: &str) -> Vec<LintFinding> {
    let suppressions = LintSuppressions::parse(text);
    if suppressions.is_empty() {
        return findings;
    }
    findings
        .into_iter()
        .filter(|finding| {
            let (line, _) = line_and_column(text, finding.span.start().get());
            !suppressions.is_suppressed(finding.rule, line)
        })
        .collect()
}

/// A finding's baseline identity hash: a hash of its *trimmed source line*, so
/// the identity survives unrelated edits elsewhere in the file (the rule and
/// path complete the key).
fn finding_content_hash(text: &str, finding: &LintFinding) -> String {
    let (line, _) = line_and_column(text, finding.span.start().get());
    let line_text = text
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim();
    stable_text_hash(line_text)
}

/// The gate-failure message for a set of reported finding rules, honoring both
/// `--fail-on-finding` (any finding) and `--fail-on <severity>` (findings at or
/// above a severity). Returns `None` when neither gate trips. Shared by the
/// SARIF and GitHub paths; the default report uses `evaluate_lint_policy`, which
/// applies the same two rules.
fn gate_message(finding_rules: &[&'static str], args: &LintReportArgs) -> Option<String> {
    let options = LintPolicyOptions::new(args.fail_on_finding, args.fail_on.map(Severity::from));
    let violations = lint_gate_violations(options, finding_rules);
    (!violations.is_empty()).then(|| violations.join("; "))
}

/// Loads the `--baseline` file, or `None` when the flag is absent.
fn load_baseline(args: &LintReportArgs) -> Result<Option<LintBaseline>> {
    match &args.baseline {
        None => Ok(None),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading baseline file {}", path.display()))?;
            Ok(Some(LintBaseline::parse(&text)?))
        }
    }
}

/// Drops findings recorded in `baseline` (matched by path + rule + line-content
/// hash), so only new findings survive. A no-op when `baseline` is `None`.
fn retain_unbaselined(
    findings: Vec<LintFinding>,
    text: &str,
    baseline: Option<&LintBaseline>,
) -> Vec<LintFinding> {
    let Some(baseline) = baseline else {
        return findings;
    };
    findings
        .into_iter()
        .filter(|finding| {
            let path = finding.path.display().to_string();
            let hash = finding_content_hash(text, finding);
            !baseline.contains(&path, finding.rule, &hash)
        })
        .collect()
}

/// The automatic fixes for one file, keyed by `(rule, finding start, finding
/// end)` so the fix engine can pair a finding with its rewrite.
///
/// The rules themselves now own their repairs (see
/// [`crate::domain::lint_report::collect_lint_fixes`]); this only reshapes the
/// domain's list into the map the fixpoint loop, SARIF writer, and fix plan
/// all index by. Later entries overwrite earlier ones on an identical key,
/// which is what a rule reporting twice on one span has always resolved to.
fn collect_lint_fixes(
    file: &std::path::Path,
    dialect: crate::domain::dialect::Dialect,
    tree: &crate::domain::sexpr::SyntaxTree,
    text: &str,
    active: &[&str],
) -> Result<std::collections::HashMap<(&'static str, usize, usize), LintFix>> {
    let mut fixes = std::collections::HashMap::new();
    for (rule, span, fix) in collect_lint_fixes_for(file, dialect, tree, text, active)? {
        let (start, end) = (span.start().get(), span.end().get());
        fixes.insert(
            (rule, start, end),
            LintFix {
                description: fix.description().to_owned(),
                replacements: fix
                    .replacements()
                    .map(|replacement| LintReplacement {
                        byte_offset: replacement.span().start().get(),
                        byte_length: replacement.span().len(),
                        text: replacement.text().to_owned(),
                    })
                    .collect(),
            },
        );
    }
    Ok(fixes)
}

pub(in crate::presentation::cli) fn lint_report(args: LintReportArgs) -> Result<()> {
    // Resolve the selected rules first so `--list-rules` can honor the same
    // `--rule`/`--exclude`/`--category` selectors as a scan (validation of rule
    // and category names happens here, before any file is read).
    let active = resolve_active_rules(&args.rules, &args.exclude, &args.categories)?;

    if args.list_rules {
        return print_lint_rule_catalog(&active, args.output);
    }

    let files = expand_input_files(&args.files, args.dialect)?;

    if args.sarif {
        return lint_report_sarif(&args, &files, &active);
    }

    if args.github {
        return lint_report_github(&args, &files, &active);
    }

    if args.stats {
        return lint_report_stats(&args, &files, &active);
    }

    if args.report_unused_suppressions {
        return lint_report_unused_suppressions(&args, &files);
    }

    if let Some(out_path) = args.write_baseline.clone() {
        return lint_report_write_baseline(&args, &files, &active, &out_path);
    }

    if args.fix_plan {
        return lint_report_fix_plan(&args, &files, &active);
    }

    if args.fix {
        return lint_report_fix(&args, &files, &active);
    }

    let baseline = load_baseline(&args)?;
    let mut findings = Vec::new();

    for file in &files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let file_findings = collect_lint_findings(file, dialect, &tree)?;
        let file_findings = retain_unsuppressed(file_findings, &input.text);
        let file_findings = retain_unbaselined(file_findings, &input.text, baseline.as_ref());
        findings.extend(file_findings);
    }

    let summary = summarize_lint_findings(findings, &active);
    let policy = evaluate_lint_policy(
        LintPolicyOptions::new(args.fail_on_finding, args.fail_on.map(Severity::from)),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_lint_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}

/// Emits findings from the active rules as SARIF 2.1.0, computing each
/// finding's line/column from its source file, then applies the same
/// `--fail-on-finding` gate as the standard report.
fn lint_report_sarif(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut results = Vec::new();
    // Disambiguates identical (rule, line-content) fingerprints so two findings
    // on look-alike lines get distinct stable ids.
    let mut fingerprint_seen: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let fixes = collect_lint_fixes(file, dialect, &tree, &input.text, active)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            let start = finding.span.start().get();
            let end = finding.span.end().get();
            let (start_line, start_column) = line_and_column(&input.text, start);
            let fingerprint =
                line_fingerprint(finding.rule, &input.text, start_line, &mut fingerprint_seen);
            let fix = fixes.get(&(finding.rule, start, end)).cloned();
            results.push(LintSarifResult {
                rule: finding.rule,
                message: finding.message,
                path: finding.path.display().to_string(),
                start_line,
                start_column,
                byte_offset: start,
                byte_length: end.saturating_sub(start),
                fingerprint,
                fix,
            });
        }
    }

    let finding_rules: Vec<&'static str> = results.iter().map(|result| result.rule).collect();
    print_lint_sarif(&results)?;

    if let Some(message) = gate_message(&finding_rules, args) {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {message}"
        )));
    }

    Ok(())
}

/// Emits the machine-readable fix plan: for each file, every fixable finding's
/// exact byte-region replacements, without writing anything. Uses the same
/// per-file structure as the SARIF path (so byte offsets are unambiguous) and
/// honors inline suppressions and `--baseline`, matching the set `--fix` would
/// touch on its first pass. Unlike `--fix`, this does not iterate to a fixpoint:
/// re-running after applying one entry surfaces any fix it unlocks. Exits 0.
fn lint_report_fix_plan(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let fixes = collect_lint_fixes(file, dialect, &tree, &input.text, active)?;
        let suppressions = LintSuppressions::parse(&input.text);
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            let start = finding.span.start().get();
            let end = finding.span.end().get();
            // A fix is keyed by (rule, form-span); a finding without one is a
            // report-only rule and simply contributes no plan entry.
            let Some(fix) = fixes.get(&(finding.rule, start, end)).cloned() else {
                continue;
            };
            // Skip a fix the suppression scan would silence, so the plan lists
            // exactly what `--fix` would apply.
            if !suppressions.is_empty() {
                let (line, _) = line_and_column(&input.text, start);
                if suppressions.is_suppressed(finding.rule, line) {
                    continue;
                }
            }
            entries.push(LintFixPlanEntry {
                rule: finding.rule,
                path: finding.path.display().to_string(),
                byte_offset: start,
                byte_length: end.saturating_sub(start),
                fix,
            });
        }
    }

    print_lint_fix_plan(&entries, args.output)?;
    Ok(())
}

/// The most fix passes to run over one file before giving up. Each pass applies
/// at least one fix and strictly shrinks the source (fixes only remove
/// wrappers/quotes), so the loop converges well within this bound; the cap is a
/// backstop against a pathological rule, not an expected limit.
const MAX_FIX_PASSES: usize = 64;

/// Applies every available auto-fix for the active rules to each input file,
/// iterating to a fixpoint so that fixes exposed by earlier fixes (e.g.
/// unwrapping `(progn (or x))` into `(or x)` then into `x`) are also applied.
/// Only rules with a registered fix contribute; the rest are ignored here.
///
/// With `args.diff` this is a preview: a unified diff of each changed file is
/// printed and nothing is written. With `args.check` nothing is written either,
/// but a non-empty pending set fails the run (exit 3) — a CI gate that a build
/// keeps green by having no auto-fixable lint left. Otherwise each rewrite is
/// reparsed and persisted with `write_file_with_rollback`, so a malformed result
/// can never overwrite good source.
fn lint_report_fix(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let mut file_fixes = Vec::new();
    let mut fixes_applied = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let mut text = input.text.clone();
        let mut tree = tree;
        let mut per_rule: std::collections::BTreeMap<&'static str, usize> =
            std::collections::BTreeMap::new();
        let mut applied = 0;

        for _ in 0..MAX_FIX_PASSES {
            let mut fixes = collect_lint_fixes(file, dialect, &tree, &text, active)?;
            // Re-parse suppressions each pass: line numbers shift as edits land,
            // but the directive comment and its form move together.
            let suppressions = LintSuppressions::parse(&text);
            if !suppressions.is_empty() {
                fixes.retain(|(rule, start, _end), _| {
                    let (line, _) = line_and_column(&text, *start);
                    !suppressions.is_suppressed(rule, line)
                });
            }
            if fixes.is_empty() {
                break;
            }

            // Choose a non-overlapping subset, preferring the earliest-starting
            // (outermost) fix on any overlap; nested fixes it shadows are caught
            // on the next pass once the outer form has been rewritten.
            // Each candidate occupies its finding span [start, end); its edits
            // (one, or several for a multi-region fix) all fall within it.
            let mut candidates: Vec<(&'static str, usize, usize, Vec<LintReplacement>)> = fixes
                .into_iter()
                .map(|((rule, start, end), fix)| (rule, start, end, fix.replacements))
                .collect();
            candidates.sort_by_key(|(_, start, end, _)| (*start, *end));

            let mut edits = Vec::new();
            let mut chosen_rules = Vec::new();
            let mut last_end = 0;
            for (rule, start, end, replacements) in candidates {
                if start < last_end {
                    continue;
                }
                last_end = end;
                for replacement in replacements {
                    let edit_start = replacement.byte_offset;
                    let edit_end = replacement.byte_offset + replacement.byte_length;
                    edits.push((
                        ByteSpan::new(ByteOffset::new(edit_start), ByteOffset::new(edit_end)),
                        replacement.text,
                    ));
                }
                chosen_rules.push(rule);
            }

            let rewritten = apply_byte_span_edits(&text, edits)?;
            // Guard the rewrite before adopting it, mirroring the write path.
            tree = SyntaxTree::parse_with_dialect(&rewritten, dialect)
                .context("refusing to fix: rewritten source does not reparse")?;
            text = rewritten;
            for rule in chosen_rules {
                *per_rule.entry(rule).or_insert(0) += 1;
                applied += 1;
            }
        }

        if applied > 0 && text != input.text {
            if args.diff {
                // Preview only: the unified diff is the payload (stdout, so it
                // pipes to a file/pager), and nothing is written.
                print!("{}", unified_diff(file, &input.text, &text));
            } else if !args.check {
                write_file_with_rollback(file.clone(), text)?;
            }
            fixes_applied += applied;
            file_fixes.push(LintFileFix {
                path: file.display().to_string(),
                applied,
                per_rule: per_rule.into_iter().collect(),
            });
        }
    }

    // --check is a CI gate: nothing is written, and a non-empty pending set
    // fails the build (exit 3). Combines with --diff, which already printed the
    // diffs above. Checked before the --diff early return so its exit code wins.
    if args.check {
        if fixes_applied > 0 {
            return Err(crate::presentation::cli::gate::gate_failure(format!(
                "{fixes_applied} auto-fixable finding(s) across {} file(s) are not applied; \
                 run `inspect lint --fix` to apply them",
                file_fixes.len()
            )));
        }
        eprintln!("no pending auto-fixes");
        return Ok(());
    }

    if args.diff {
        // Keep stdout pure diff (a JSON/text summary would corrupt it); the
        // one-line tally goes to stderr, and nothing was written.
        eprintln!(
            "{fixes_applied} fix(es) across {} file(s) — preview only, nothing written",
            file_fixes.len()
        );
        return Ok(());
    }

    print_lint_fix_report(&file_fixes, fixes_applied, args.output)
}

/// Writes the current findings (for the active rules, after suppression) to a
/// baseline file, so a later `--baseline` run can gate only on new findings.
/// Prints a one-line summary and exits 0.
fn lint_report_write_baseline(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    out_path: &std::path::Path,
) -> Result<()> {
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            entries.push(BaselineEntry {
                path: finding.path.display().to_string(),
                rule: finding.rule.to_owned(),
                hash: finding_content_hash(&input.text, &finding),
            });
        }
    }

    let baseline = LintBaseline::from_entries(entries);
    let entry_count = baseline.len();
    std::fs::write(out_path, baseline.to_json()?)
        .with_context(|| format!("writing baseline file {}", out_path.display()))?;

    match args.output {
        crate::presentation::cli::OutputFormat::Text => {
            println!("baseline_written\t{}", out_path.display());
            println!("entry_count\t{entry_count}");
        }
        crate::presentation::cli::OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "baseline_written": out_path.display().to_string(),
                    "entry_count": entry_count,
                }))?
            );
        }
    }

    Ok(())
}

/// Aggregates findings (for the active rules, after suppression and baseline)
/// into a rollup by severity, category, and rule — a lint-debt dashboard. Honors
/// the same `--rule`/`--category`/`--baseline` filters as the standard report.
fn lint_report_stats(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut by_rule: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut files_with_findings = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let mut file_had_finding = false;
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            *by_rule.entry(finding.rule).or_insert(0) += 1;
            file_had_finding = true;
        }
        if file_had_finding {
            files_with_findings += 1;
        }
    }

    let finding_count = by_rule.values().sum();

    // Severity rollup (error then warning), always both keys.
    let mut error_count = 0;
    let mut warning_count = 0;
    for (rule, count) in &by_rule {
        match rule_severity(rule) {
            Severity::Error => error_count += count,
            Severity::Warning => warning_count += count,
        }
    }
    let by_severity = vec![("error", error_count), ("warning", warning_count)];

    // Category rollup, one entry per category in CATEGORIES order.
    let by_category = CATEGORIES
        .iter()
        .map(|category| {
            let count = by_rule
                .iter()
                .filter(|(rule, _)| rule_category(rule) == Some(*category))
                .map(|(_, count)| *count)
                .sum();
            (*category, count)
        })
        .collect();

    // Per-rule rollup, most-frequent first (ties broken by rule name).
    let mut by_rule: Vec<(&'static str, usize)> = by_rule.into_iter().collect();
    by_rule.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let stats = LintStats {
        finding_count,
        files_scanned: files.len(),
        files_with_findings,
        by_severity,
        by_category,
        by_rule,
    };
    print_lint_stats(&stats, args.output)
}

/// Reports every inline `; paredit:ignore` directive that silences no finding
/// (a stale ignore or a typo'd rule name). Detection runs against *all* rules —
/// independent of `--rule`/`--exclude` — so an ignore is "unused" only when it
/// matches no finding the file actually has, and the run exits 3 if any are
/// found so CI can keep the ignore list clean.
fn lint_report_unused_suppressions(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
) -> Result<()> {
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        // Line -> the rules that reported a finding there (across every rule).
        let mut present: std::collections::HashMap<usize, std::collections::HashSet<&'static str>> =
            std::collections::HashMap::new();
        for finding in collect_lint_findings(file, dialect, &tree)? {
            let (line, _) = line_and_column(&input.text, finding.span.start().get());
            present.entry(line).or_default().insert(finding.rule);
        }

        let suppressions = LintSuppressions::parse(&input.text);
        for unused in suppressions.unused_directives(&present) {
            entries.push((file.display().to_string(), unused));
        }
    }

    let unused_count = entries.len();
    print_lint_unused_suppressions(&entries, args.output)?;

    if unused_count > 0 {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {unused_count} unused suppression(s)"
        )));
    }

    Ok(())
}

/// Emits findings from the active rules as GitHub Actions annotations
/// (`::error file=...,line=...,col=...::rule: message`), which the Actions
/// runner renders as inline annotations on the pull request diff. Applies the
/// same `--fail-on-finding` gate as the standard report.
fn lint_report_github(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut finding_rules: Vec<&'static str> = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            finding_rules.push(finding.rule);
            let (line, column) = line_and_column(&input.text, finding.span.start().get());
            print_lint_github_annotation(
                &finding.path.display().to_string(),
                line,
                column,
                finding.rule,
                &finding.message,
            );
        }
    }

    if let Some(message) = gate_message(&finding_rules, args) {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {message}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::usecase::lint_report::{FIXABLE_RULES, RULES};
    use crate::domain::dialect::Dialect;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    /// Guards that [`FIXABLE_RULES`] names exactly the rules `collect_lint_fixes`
    /// can actually produce a fix for. Each line below is the minimal trigger
    /// for one fixable rule; a non-fixable trigger (`if-arity`) is included to
    /// prove no stray fix leaks in.
    #[test]
    fn fixable_rules_match_the_fix_engine() {
        let source = concat!(
            "(list '5)\n",                                           // redundant-quote
            "(progn only)\n",                                        // redundant-progn
            "(progn a (progn b c))\n", // nested-progn (the inner progn)
            "(when q (progn s t))\n",  // redundant-body-progn
            "(let () (ela) (elb))\n",  // empty-let
            "(if c d nil)\n",          // redundant-if-nil
            "(funcall #'g m)\n",       // redundant-funcall
            "(the t whatever)\n",      // redundant-the
            "(funcall (lambda (fx) fx) 9)\n", // funcall-lambda
            "(mapcar #'(lambda (sq) sq) sqs)\n", // sharp-quoted-lambda
            "(identity h)\n",          // redundant-identity
            "(cons e nil)\n",          // cons-to-list
            "(reverse (reverse dr))\n", // double-reverse
            "(append (list al) ar)\n", // append-list-to-cons
            "(format nil \"~A\" fs)\n", // format-to-string
            "(format t \"~%\")\n",     // format-newline
            "(floor fq 1)\n",          // redundant-divisor
            "(- 0 amt)\n",             // verbose-negation
            "(list* la lb)\n",         // list-star-to-cons
            "(values-list (list va vb))\n", // values-list-of-list
            "(prog1 (p1x))\n",         // redundant-prog1
            "(subseq sz 0)\n",         // subseq-zero
            "(car (nthcdr cn cx))\n",  // car-nthcdr
            "(car (reverse crx))\n",   // car-reverse
            "(append anx nil)\n",      // append-nil
            "(multiple-value-list (values mva mvb))\n", // multiple-value-list-of-values
            "(typep tpx 'string)\n",   // typep-predicate
            "(coerce ctx t)\n",        // coerce-to-t
            "(gethash gdk gdh nil)\n", // gethash-default
            "(make-hash-table :test 'eql)\n", // make-hash-table-test
            "(let* ((a 1)) a)\n",      // redundant-let-star
            "(cond (ok (run)))\n",     // single-clause-cond
            "(cond (t (r1) (r2)))\n",  // cond-t-clause
            "(incf tally 1)\n",        // explicit-step-delta
            "(incf nsd -3)\n",         // negated-step-delta
            "(return-from blk nil)\n", // explicit-nil-return
            "(multiple-value-bind (mv) (vals) mv)\n", // single-value-bind
            "(or za (or pb qc))\n",    // nested-boolean
            "(when wa (when wb (wc)))\n", // nested-when
            "(unless ua (unless ub (uc)))\n", // nested-unless
            "(and x)\n",               // single-operand-boolean
            "(append solo)\n",         // single-operand-list-op
            "(* x)\n",                 // single-operand-arithmetic
            "(when (not r) y)\n",      // negated-when-unless
            "(if p q)\n",              // one-armed-if
            "(setf ctr (1+ ctr))\n",   // manual-incf
            "(setf lst (cons e lst))\n", // manual-push
            "(setf st (adjoin e st))\n", // manual-pushnew
            "(car (cdr z))\n",         // nested-cxr
            "(nth 0 zs)\n",            // nth-constant-index
            "(nthcdr 0 nz)\n",         // nthcdr-zero
            "(nthcdr 2 ns)\n",         // nthcdr-small-index
            "(apply #'g (list m))\n",  // redundant-apply
            "(find ret lst :test #'eql)\n", // redundant-eql-test
            "(find rsz lst :start 0)\n", // redundant-start-zero
            "(find ren lst :end nil)\n", // redundant-end-nil
            "(find rfe lst :from-end nil)\n", // redundant-from-end-nil
            "(remove rcn lst :count nil)\n", // redundant-count-nil
            "(string= (string-downcase sa) (string-downcase sb))\n", // string-case-fold
            "(char= (char-downcase ca) (char-downcase cb))\n", // char-case-fold
            "(string-upcase (string-downcase nsc))\n", // nested-string-case
            "(code-char (char-code ccc))\n", // code-char-char-code
            "(last ldc 1)\n",          // last-default-count
            "(butlast bdc 1)\n",       // butlast-default-count
            "(make-list mde :initial-element nil)\n", // make-list-default-element
            "(parse-integer pir :radix 10)\n", // parse-integer-default-radix
            "(getf gdn :key nil)\n",   // getf-default-nil
            "(make-array madk :adjustable nil)\n", // make-array-default-keyword
            "(char-upcase (char-downcase ncc))\n", // nested-char-case
            "(list* lsn1 lsn2 nil)\n", // list-star-nil
            "(sort rik #'< :key #'identity)\n", // redundant-identity-key
            "(= tally 0)\n",           // sign-comparison
            "(not (< a b))\n",         // negated-comparison
            "(if (not c) a b)\n",      // negated-if
            "(if iv iv jv)\n",         // if-to-or
            "(if iw nil t)\n",         // if-not
            "(if iu nil (iue))\n",     // if-to-unless
            "(prog2 (p2a) (p2b))\n",   // prog2-to-progn
            "(handler-case (hcx))\n",  // handler-case-no-clauses
            "(unwind-protect (upx))\n", // unwind-protect-no-cleanup
            "(+ osa 1)\n",             // one-step-arithmetic
            "(if t on off)\n",         // constant-if-test
            "(when t (bd))\n",         // constant-when-test
            "(and p t q)\n",           // redundant-boolean-identity
            "(and (not p) (not q))\n", // de-morgan
            "(equal w nil)\n",         // nil-comparison
            "(eq n 7)\n",              // eq-number-comparison
            "(eq c #\\a)\n",           // eq-char-comparison
            "(if a b c d)\n",          // if-arity — NOT fixable
        );
        let tree =
            crate::domain::sexpr::SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)
                .expect("parse fixture");
        let active: Vec<&str> = RULES.to_vec();
        let fixes = collect_lint_fixes(
            &PathBuf::from("fixture.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            &active,
        )
        .expect("collect fixes");

        let produced: BTreeSet<&str> = fixes.keys().map(|(rule, _, _)| *rule).collect();
        let declared: BTreeSet<&str> = FIXABLE_RULES.iter().copied().collect();
        assert_eq!(
            produced, declared,
            "collect_lint_fixes must produce a fix for exactly the FIXABLE_RULES"
        );
    }
}
