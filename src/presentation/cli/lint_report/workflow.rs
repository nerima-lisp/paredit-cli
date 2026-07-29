use anyhow::{Context, Result};

use crate::application::usecase::lint_report::{
    CATEGORIES, FindingId, LintFinding, LintPassRequest, LintPolicyOptions, LintSuppressions,
    RULES, RuleFilter, RuleFixFor, RulePreset, RuleSettings, RuleTag, RuleTimings, Severity,
    SeverityOverrides, apply_severity_override, collect_lint_findings, evaluate_lint_policy,
    lint_gate_violations, resolve_active_rules, rule_setting, rule_tags, rule_timing_report,
    run_lint_pass, summarize_lint_findings,
};
use crate::domain::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use crate::presentation::cli::lint_report::args::LintReportArgs;
use crate::presentation::cli::lint_report::baseline::{BaselineEntry, LintBaseline};
use crate::presentation::cli::lint_report::custom::{self, CustomRules, RuleMetaResolver};
use crate::presentation::cli::lint_report::render::{
    FindingIds, LintFileFix, LintFix, LintFixPlanEntry, LintReplacement, LintSarifResult,
    LintStats, LintSuppressionRemoval, LintTiming, print_lint_docs, print_lint_explanation,
    print_lint_fix_plan, print_lint_fix_report, print_lint_github_annotation, print_lint_presets,
    print_lint_report, print_lint_rule_catalog, print_lint_sarif, print_lint_stats,
    print_lint_suppression_removal, print_lint_tags, print_lint_timings,
    print_lint_unused_suppressions,
};
use crate::presentation::cli::shared::{
    analyze_files, apply_byte_span_edits, expand_input_files, read_input_dialect_and_tree,
    stable_text_hash, unified_diff, write_file_with_rollback,
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

/// Drops findings silenced by an inline `paredit:ignore` directive in the
/// file's own source, so a suppression comment applies uniformly across every
/// output mode (report, SARIF, GitHub, and fix).
///
/// The tree is what resolves `paredit:ignore-next-form` to the span of the form
/// it protects; without it that scope would degrade to one line.
fn retain_unsuppressed(
    findings: Vec<LintFinding>,
    text: &str,
    tree: &SyntaxTree,
) -> Vec<LintFinding> {
    let suppressions = LintSuppressions::parse_in_tree(text, tree);
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

/// The content-derived id of each finding, in the order they are reported.
///
/// The occurrence counter that disambiguates two findings with the same rule
/// and the same normalized form text is scoped per `(path, rule, fingerprint)`,
/// so it is assigned here — where the whole file's findings are in hand —
/// rather than by each rule, which sees one node at a time.
fn assign_finding_ids(findings: &[LintFinding], text: &str) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    findings
        .iter()
        .map(|finding| {
            let form = text
                .get(finding.span.start().get()..finding.span.end().get())
                .unwrap_or_default();
            // Key on the id a zero occurrence would produce: two findings
            // collide exactly when that string matches, which is the same test
            // `FindingId` itself applies.
            let base = FindingId::new(finding.rule, form, 0);
            let occurrence = seen.entry(base.as_str().to_owned()).or_insert(0);
            let id = FindingId::new(finding.rule, form, *occurrence);
            *occurrence += 1;
            id.as_str().to_owned()
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
fn gate_message(
    finding_rules: &[&'static str],
    args: &LintReportArgs,
    overrides: &SeverityOverrides,
) -> Option<String> {
    let options = LintPolicyOptions::new(args.fail_on_finding, args.fail_on.map(Severity::from));
    let violations = lint_gate_violations(overrides, options, finding_rules);
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
    settings: &RuleSettings,
) -> Result<FixMap> {
    Ok(fix_map(
        run_lint_pass(
            file,
            dialect,
            tree,
            text,
            LintPassRequest {
                active,
                settings: Some(settings),
                measure: false,
            },
        )?
        .fixes,
    ))
}

/// Fixes keyed by `(rule, finding start, finding end)`, which is how the
/// fixpoint loop, the SARIF writer, and the fix plan all pair a finding with
/// its rewrite.
type FixMap = std::collections::HashMap<(&'static str, usize, usize), LintFix>;

fn fix_map(fixes: Vec<RuleFixFor>) -> FixMap {
    let mut map = std::collections::HashMap::new();
    for (rule, span, fix) in fixes {
        let (start, end) = (span.start().get(), span.end().get());
        map.insert(
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
    map
}

/// The `--deny`/`--warn` promotions and demotions this run asked for.
///
/// Resolved before any file is read, so a typo'd rule or category name fails
/// the run rather than quietly changing nothing — the whole point of the flag
/// is to change what fails CI.
fn resolve_severity_overrides(args: &LintReportArgs) -> Result<SeverityOverrides> {
    let mut overrides = SeverityOverrides::new();
    for selector in &args.warn {
        apply_severity_override(&mut overrides, selector, Severity::Warning)?;
    }
    // `--deny` is applied second so it wins a same-selector tie, matching the
    // reading that the stricter flag is the deliberate one.
    for selector in &args.deny {
        apply_severity_override(&mut overrides, selector, Severity::Error)?;
    }
    Ok(overrides)
}

/// Parses every `--rule-arg <rule>.<key>=<value>` against the rules' declared
/// knobs.
///
/// Each part is checked: the rule must exist, it must declare that key, and the
/// value must be an integer. A silently ignored override would leave a CI gate
/// running with a threshold nobody set.
fn resolve_rule_settings(args: &LintReportArgs) -> Result<RuleSettings> {
    let mut settings = RuleSettings::new();
    for argument in &args.rule_args {
        let (target, value) = argument.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("malformed --rule-arg {argument:?}; expected <rule>.<key>=<value>")
        })?;
        let (rule, key) = target.rsplit_once('.').ok_or_else(|| {
            anyhow::anyhow!("malformed --rule-arg {argument:?}; expected <rule>.<key>=<value>")
        })?;
        if !RULES.contains(&rule) {
            anyhow::bail!("unknown lint rule {rule:?} in --rule-arg {argument:?}");
        }
        let Some(declared) = rule_setting(rule, key) else {
            let valid: Vec<&str> = crate::application::usecase::lint_report::rule_settings(rule)
                .iter()
                .map(|setting| setting.key())
                .collect();
            anyhow::bail!(
                "lint rule {rule:?} has no setting {key:?}; valid settings: {}",
                if valid.is_empty() {
                    "(none)".to_owned()
                } else {
                    valid.join(", ")
                }
            );
        };
        let parsed: i64 = value.parse().with_context(|| {
            format!(
                "--rule-arg {argument:?} needs an integer value (the default is {})",
                declared.default()
            )
        })?;
        settings.set(rule, key, parsed);
    }
    Ok(settings)
}

pub(in crate::presentation::cli) fn lint_report(args: LintReportArgs) -> Result<()> {
    // Resolve the selected rules first so the catalogue-only modes honor the
    // same `--rule`/`--exclude`/`--category`/`--tag`/`--preset` selectors as a
    // scan. Every name is validated here, before any file is read.
    let filter = RuleFilter {
        only: &args.rules,
        exclude: &args.exclude,
        categories: &args.categories,
        tags: &args.tags,
        preset: args.preset.into(),
        experimental: args.experimental,
    };
    let active = resolve_active_rules(&filter)?;
    let overrides = resolve_severity_overrides(&args)?;
    let settings = resolve_rule_settings(&args)?;
    // Loaded before any file is read, so a rule file that does not parse fails
    // the run rather than contributing nothing to a green one.
    let custom = custom::load(args.custom_rules.as_deref())?;
    let meta = RuleMetaResolver::new(&overrides, &custom);

    if args.test_rules {
        return lint_report_test_rules(&custom);
    }

    if let Some(rule) = &args.explain {
        if !RULES.contains(&rule.as_str()) {
            anyhow::bail!(
                "unknown lint rule {rule:?}; run `inspect lint --list-rules` for the catalogue"
            );
        }
        return print_lint_explanation(rule, args.output);
    }

    if args.list_presets {
        let counts = RulePreset::ALL
            .into_iter()
            .map(|preset| {
                let scoped = RuleFilter {
                    preset,
                    experimental: args.experimental,
                    ..RuleFilter::default()
                };
                resolve_active_rules(&scoped).map(|rules| (preset, rules.len()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return print_lint_presets(&counts, args.output);
    }

    if args.list_tags {
        return print_lint_tags(args.output);
    }

    if args.docs {
        return print_lint_docs(&active);
    }

    if args.list_rules {
        return print_lint_rule_catalog(&active, &custom, args.output);
    }

    let files = expand_input_files(&args.files, args.dialect)?;

    if args.timings {
        return lint_report_timings(&args, &files, &active, &settings);
    }

    if args.sarif {
        return lint_report_sarif(&args, &files, &active, &meta, &custom, &settings);
    }

    if args.github {
        return lint_report_github(&args, &files, &active, &meta, &custom);
    }

    if args.stats {
        return lint_report_stats(&args, &files, &active, &meta, &custom);
    }

    if args.remove_unused_suppressions {
        return lint_report_remove_unused_suppressions(&args, &files);
    }

    if args.report_unused_suppressions {
        return lint_report_unused_suppressions(&args, &files);
    }

    if let Some(out_path) = args.write_baseline.clone() {
        return lint_report_write_baseline(&args, &files, &active, &out_path);
    }

    if args.fix_plan {
        return lint_report_fix_plan(&args, &files, &active, &settings);
    }

    if args.fix {
        return lint_report_fix(&args, &files, &active, &custom, &settings);
    }

    let baseline = load_baseline(&args)?;
    let mut findings = Vec::new();
    let mut ids: FindingIds = Vec::new();

    // The 170-rule pass over each file is the heaviest per-file work in this
    // tool and has no dependency between files, so it runs on every core.
    // `analyze_files` returns results in input order, which is what keeps the
    // report byte-identical however the workers were scheduled.
    let per_file = analyze_files(&files, args.dialect, |file, dialect, tree, input| {
        let file_findings = run_lint_pass(
            file,
            dialect,
            tree,
            &input.text,
            LintPassRequest {
                active: &[],
                settings: Some(&settings),
                measure: false,
            },
        )?
        .findings;
        let file_findings = merge_custom(file_findings, &custom, file, tree, &input.text);
        let file_findings = retain_unsuppressed(file_findings, &input.text, tree);
        let file_findings = retain_unbaselined(file_findings, &input.text, baseline.as_ref());
        // Ids are assigned per file, against that file's source, and the
        // summary keeps the findings in the order they arrive — so the two
        // lists stay aligned as long as `summarize_lint_findings` only ever
        // filters, which it does.
        let kept: Vec<LintFinding> = file_findings
            .into_iter()
            .filter(|finding| active.contains(&finding.rule) || custom.is_rule(finding.rule))
            .collect();
        let file_ids = assign_finding_ids(&kept, &input.text);
        Ok((kept, file_ids))
    })?;

    for (kept, file_ids) in per_file {
        ids.extend(file_ids);
        findings.extend(kept);
    }

    let summary = summarize_lint_findings(findings, &active_with_custom(&active, &custom));
    let policy = evaluate_lint_policy(
        &overrides,
        LintPolicyOptions::new(args.fail_on_finding, args.fail_on.map(Severity::from)),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_lint_report(&summary, &policy, &ids, &meta, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}

/// Reports what each rule cost across the scanned files, slowest first.
///
/// Findings are computed and thrown away: the point is the clock, and running
/// the real pass is the only way to measure the real cost. Files are scanned
/// with every rule the selection admits, so the numbers describe the run the
/// caller would otherwise have made.
///
/// Deliberately serial, unlike the report and SARIF paths. Sixteen workers
/// contending for memory bandwidth measure the machine rather than the rules,
/// and a per-rule cost that changes with `--jobs` is not a per-rule cost.
fn lint_report_timings(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    settings: &RuleSettings,
) -> Result<()> {
    let mut total: Option<RuleTimings> = None;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let result = run_lint_pass(
            file,
            dialect,
            &tree,
            &input.text,
            LintPassRequest {
                active,
                settings: Some(settings),
                measure: true,
            },
        )?;
        let Some(measured) = result.timings else {
            continue;
        };
        match &mut total {
            Some(accumulated) => accumulated.merge(&measured),
            None => total = Some(measured),
        }
    }

    let Some(total) = total else {
        return print_lint_timings(&[], 0, files.len(), args.output);
    };
    let total_micros = total.total().as_micros();
    let mut rows: Vec<LintTiming> = rule_timing_report(&total)
        .into_iter()
        .filter(|(rule, _, invocations)| *invocations > 0 && active.contains(rule))
        .map(|(rule, elapsed, invocations)| {
            let micros = elapsed.as_micros();
            LintTiming {
                rule,
                micros,
                invocations,
                #[allow(clippy::cast_precision_loss)]
                share: if total_micros == 0 {
                    0.0
                } else {
                    (micros as f64 / total_micros as f64) * 100.0
                },
            }
        })
        .collect();
    // Slowest first, ties broken by name so two runs of the same input print
    // the same table.
    rows.sort_by(|a, b| b.micros.cmp(&a.micros).then(a.rule.cmp(b.rule)));

    print_lint_timings(&rows, total_micros, files.len(), args.output)
}

/// Deletes the `paredit:ignore` directives that silence nothing, and narrows
/// the ones only partly stale, writing each changed file through the rollback
/// writer.
///
/// Detection runs against *all* rules — independent of `--rule`/`--exclude` —
/// for the same reason [`lint_report_unused_suppressions`] does: an ignore is
/// stale only when the file has no finding it could have silenced, and a
/// narrowed rule selection would make perfectly live ignores look dead and
/// delete them.
fn lint_report_remove_unused_suppressions(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
) -> Result<()> {
    let mut changed = Vec::new();
    let mut removed_total = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let present = findings_by_line(file, dialect, &tree, &input.text)?;
        let suppressions = LintSuppressions::parse_in_tree(&input.text, &tree);
        let edits = suppressions.removal_edits(&present);
        if edits.is_empty() {
            continue;
        }

        let mut text = input.text.clone();
        // Back to front, so an earlier edit's offsets stay valid.
        for edit in edits.iter().rev() {
            text.replace_range(edit.start..edit.end, &edit.text);
        }
        // Removing a comment cannot unbalance a form, but the guard is what
        // makes that a checked fact rather than an assumption.
        SyntaxTree::parse_with_dialect(&text, dialect)
            .context("refusing to edit: source with suppressions removed does not reparse")?;

        let directives: Vec<(usize, Vec<String>)> = edits
            .iter()
            .map(|edit| {
                (
                    edit.comment_line,
                    edit.removed_rules.clone().unwrap_or_default(),
                )
            })
            .collect();
        removed_total += directives.len();
        changed.push(LintSuppressionRemoval {
            path: file.display().to_string(),
            removed: directives.len(),
            directives,
        });
        write_file_with_rollback(file.clone(), text)?;
    }

    print_lint_suppression_removal(&changed, removed_total, args.output)
}

/// Line -> the rules that reported a finding there, across every rule.
fn findings_by_line(
    file: &std::path::Path,
    dialect: crate::domain::dialect::Dialect,
    tree: &SyntaxTree,
    text: &str,
) -> Result<std::collections::HashMap<usize, std::collections::HashSet<&'static str>>> {
    let mut present: std::collections::HashMap<usize, std::collections::HashSet<&'static str>> =
        std::collections::HashMap::new();
    for finding in collect_lint_findings(file, dialect, tree)? {
        let (line, _) = line_and_column(text, finding.span.start().get());
        present.entry(line).or_default().insert(finding.rule);
    }
    Ok(present)
}

/// Emits findings from the active rules as SARIF 2.1.0, computing each
/// finding's line/column from its source file, then applies the same
/// `--fail-on-finding` gate as the standard report.
fn lint_report_sarif(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    meta: &RuleMetaResolver<'_>,
    custom: &CustomRules,
    settings: &RuleSettings,
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut results = Vec::new();
    // Disambiguates identical (rule, line-content) fingerprints so two findings
    // on look-alike lines get distinct stable ids.
    let mut fingerprint_seen: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    // The analysis is parallel; the *fingerprint* assignment is not, and must
    // not be. `fingerprint_seen` counts occurrences across the whole run, so a
    // finding's suffix depends on how many identical-looking lines preceded it
    // — which is only well defined in file order. Computing per file in
    // parallel and numbering afterwards keeps both properties.
    let per_file = analyze_files(files, args.dialect, |file, dialect, tree, input| {
        let pass = run_lint_pass(
            file,
            dialect,
            tree,
            &input.text,
            LintPassRequest {
                active,
                settings: Some(settings),
                measure: false,
            },
        )?;
        let mut fixes = fix_map(pass.fixes);
        let findings = merge_custom(pass.findings, custom, file, tree, &input.text);
        custom_fixes(custom, file, tree, &input.text, &mut fixes);
        let findings = retain_unsuppressed(findings, &input.text, tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let findings: Vec<LintFinding> = findings
            .into_iter()
            .filter(|finding| active.contains(&finding.rule) || custom.is_rule(finding.rule))
            .collect();
        let ids = assign_finding_ids(&findings, &input.text);
        Ok((findings, ids, fixes, input.text.clone()))
    })?;

    for (findings, ids, fixes, text) in per_file {
        for (index, finding) in findings.into_iter().enumerate() {
            let start = finding.span.start().get();
            let end = finding.span.end().get();
            let (start_line, start_column) = line_and_column(&text, start);
            let fingerprint =
                line_fingerprint(finding.rule, &text, start_line, &mut fingerprint_seen);
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
                finding_id: ids.get(index).cloned().unwrap_or_default(),
                fix,
            });
        }
    }

    let finding_rules: Vec<&'static str> = results.iter().map(|result| result.rule).collect();
    print_lint_sarif(&results, meta)?;

    if let Some(message) = gate_message(&finding_rules, args, meta.overrides()) {
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
    settings: &RuleSettings,
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let pass = run_lint_pass(
            file,
            dialect,
            &tree,
            &input.text,
            LintPassRequest {
                active,
                settings: Some(settings),
                measure: false,
            },
        )?;
        let fixes = fix_map(pass.fixes);
        let findings = retain_unsuppressed(pass.findings, &input.text, &tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let findings: Vec<LintFinding> = findings
            .into_iter()
            .filter(|finding| active.contains(&finding.rule))
            .collect();
        let ids = assign_finding_ids(&findings, &input.text);
        for (index, finding) in findings.into_iter().enumerate() {
            let start = finding.span.start().get();
            let end = finding.span.end().get();
            // A fix is keyed by (rule, form-span); a finding without one is a
            // report-only rule and simply contributes no plan entry.
            let Some(fix) = fixes.get(&(finding.rule, start, end)).cloned() else {
                continue;
            };
            entries.push(LintFixPlanEntry {
                rule: finding.rule,
                path: finding.path.display().to_string(),
                byte_offset: start,
                byte_length: end.saturating_sub(start),
                finding_id: ids.get(index).cloned().unwrap_or_default(),
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
/// A rule's position in the registry, used as the last tie-break when two
/// rules offer a fix for the exact same span.
///
/// Without it the winner came out of a `HashMap` iteration, so two runs over
/// the same file could rewrite it differently — a determinism hole in the one
/// command that writes to disk.
fn registration_rank(rule: &str) -> usize {
    RULES
        .iter()
        .position(|name| *name == rule)
        .unwrap_or(RULES.len())
}

fn lint_report_fix(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    custom: &CustomRules,
    settings: &RuleSettings,
) -> Result<()> {
    let mut file_fixes = Vec::new();
    let mut fixes_applied = 0;
    let mut fix_conflicts = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let mut text = input.text.clone();
        let mut tree = tree;
        let mut per_rule: std::collections::BTreeMap<&'static str, usize> =
            std::collections::BTreeMap::new();
        let mut applied = 0;
        // How often two fixes wanted overlapping regions in one pass. The
        // shadowed one is not lost — the next pass re-offers it once the outer
        // form has been rewritten — so this measures contention, not backlog.
        let mut conflicts = 0;

        for _ in 0..MAX_FIX_PASSES {
            let mut fixes = collect_lint_fixes(file, dialect, &tree, &text, active, settings)?;
            custom_fixes(custom, file, &tree, &text, &mut fixes);
            // Re-parse suppressions each pass: line numbers shift as edits land,
            // but the directive comment and its form move together.
            let suppressions = LintSuppressions::parse_in_tree(&text, &tree);
            if !suppressions.is_empty() {
                fixes.retain(|(rule, start, _end), _| {
                    let (line, _) = line_and_column(&text, *start);
                    !suppressions.is_suppressed(rule, line)
                });
            }
            if args.no_destructive_fixes {
                fixes.retain(|(rule, _, _), _| !rule_tags(rule).contains(RuleTag::Destructive));
            }
            if fixes.is_empty() {
                break;
            }

            // Choose a non-overlapping subset, preferring the *outermost* fix on
            // any overlap; nested fixes it shadows are caught on the next pass
            // once the outer form has been rewritten. Each candidate occupies
            // its finding span [start, end); its edits (one, or several for a
            // multi-region fix) all fall within it.
            //
            // The sort key is total and derived only from the data, so the
            // choice is the same on every run: earliest start first, then the
            // widest span (the enclosing form), then registry order for two
            // rules that report the identical span.
            let mut candidates: Vec<(&'static str, usize, usize, Vec<LintReplacement>)> = fixes
                .into_iter()
                .map(|((rule, start, end), fix)| (rule, start, end, fix.replacements))
                .collect();
            candidates.sort_by_key(|(rule, start, end, _)| {
                (
                    *start,
                    std::cmp::Reverse(*end),
                    registration_rank(rule),
                    *rule,
                )
            });

            let mut edits = Vec::new();
            let mut chosen_rules = Vec::new();
            let mut last_end = 0;
            for (rule, start, end, replacements) in candidates {
                if start < last_end {
                    conflicts += 1;
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
        fix_conflicts += conflicts;

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

    print_lint_fix_report(&file_fixes, fixes_applied, fix_conflicts, args.output)
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
        let findings = retain_unsuppressed(
            collect_lint_findings(file, dialect, &tree)?,
            &input.text,
            &tree,
        );
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
    meta: &RuleMetaResolver<'_>,
    custom: &CustomRules,
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut by_rule: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut files_with_findings = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings = merge_custom(
            collect_lint_findings(file, dialect, &tree)?,
            custom,
            file,
            &tree,
            &input.text,
        );
        let findings = retain_unsuppressed(findings, &input.text, &tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let mut file_had_finding = false;
        for finding in findings {
            if !active.contains(&finding.rule) && !custom.is_rule(finding.rule) {
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
        match meta.severity(rule) {
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
                .filter(|(rule, _)| meta.category(rule) == Some(*category))
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
        let present = findings_by_line(file, dialect, &tree, &input.text)?;
        let suppressions = LintSuppressions::parse_in_tree(&input.text, &tree);
        for unused in suppressions.unused_directives(&present, args.require_suppression_reason) {
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
    meta: &RuleMetaResolver<'_>,
    custom: &CustomRules,
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut finding_rules: Vec<&'static str> = Vec::new();

    // Analysed in parallel, *printed* serially: an annotation stream that
    // interleaved by thread would be unreadable and, worse, unstable between
    // runs. The split is the general shape for adopting `analyze_files` in a
    // command that emits as it goes — compute per file, emit in file order.
    let annotated = analyze_files(files, args.dialect, |file, dialect, tree, input| {
        let findings = merge_custom(
            collect_lint_findings(file, dialect, tree)?,
            custom,
            file,
            tree,
            &input.text,
        );
        let findings = retain_unsuppressed(findings, &input.text, tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        Ok(findings
            .into_iter()
            .filter(|finding| active.contains(&finding.rule) || custom.is_rule(finding.rule))
            .map(|finding| {
                let (line, column) = line_and_column(&input.text, finding.span.start().get());
                (finding, line, column)
            })
            .collect::<Vec<_>>())
    })?;

    for (finding, line, column) in annotated.into_iter().flatten() {
        finding_rules.push(finding.rule);
        print_lint_github_annotation(
            &finding.path.display().to_string(),
            line,
            column,
            finding.rule,
            &finding.message,
            meta.severity(finding.rule),
        );
    }

    if let Some(message) = gate_message(&finding_rules, args, meta.overrides()) {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {message}"
        )));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Custom rules, merged into the shipped suite's findings.
//
// The two passes stay separate (see `custom.rs` for why the engine cannot hold
// a rule read at startup), and they meet here: one function that appends the
// custom findings to a file's list, and one that adds their fixes to the map
// `--fix` and `--fix-plan` index. Everything downstream — suppressions, the
// baseline, ids, the gate, every output format — then treats both alike.
// ---------------------------------------------------------------------------

/// Appends the custom rules' findings for one file.
///
/// Appended rather than interleaved: the shipped suite's order is a published
/// contract, and putting a project's rules after it means adding one cannot
/// move a shipped finding.
fn merge_custom(
    mut findings: Vec<LintFinding>,
    custom: &CustomRules,
    file: &std::path::Path,
    tree: &SyntaxTree,
    text: &str,
) -> Vec<LintFinding> {
    if custom.is_empty() {
        return findings;
    }
    for (rule, finding) in custom.findings(tree, text) {
        findings.push(LintFinding {
            rule,
            path: file.to_path_buf(),
            span: finding.span,
            message: finding.message,
        });
    }
    findings
}

/// Adds the custom rules' rewrites to a file's fix map.
fn custom_fixes(
    custom: &CustomRules,
    _file: &std::path::Path,
    tree: &SyntaxTree,
    text: &str,
    fixes: &mut FixMap,
) {
    if custom.is_empty() {
        return;
    }
    for (rule, finding) in custom.findings(tree, text) {
        let Some(replacement) = finding.fix else {
            continue;
        };
        let (start, end) = (finding.span.start().get(), finding.span.end().get());
        fixes.insert(
            (rule, start, end),
            LintFix {
                description: format!("Apply the custom rule {rule}"),
                replacements: vec![LintReplacement {
                    byte_offset: start,
                    byte_length: end.saturating_sub(start),
                    text: replacement,
                }],
            },
        );
    }
}

/// The active rule list widened by the custom rules.
///
/// `summarize_lint_findings` keeps only findings whose rule is in this list and
/// builds the per-rule checklist from it, so a custom rule that is not here
/// would be silently dropped after having been computed.
fn active_with_custom(active: &[&'static str], custom: &CustomRules) -> Vec<&'static str> {
    let mut widened = active.to_vec();
    widened.extend(custom.names());
    widened
}

/// Runs the `deftest` clauses in the custom rule files.
///
/// A separate mode rather than part of a scan: a rule file is code, and code
/// nobody can check goes wrong quietly. Exits 3 on any failure so CI can keep
/// a project's own rules correct the same way it keeps its own tests correct.
fn lint_report_test_rules(custom: &CustomRules) -> Result<()> {
    let failures = custom.test();
    let rule_count = custom.ruleset().rules.len();
    let test_count = custom.ruleset().tests.len();

    println!("custom_rule_count\t{rule_count}");
    println!("custom_test_count\t{test_count}");
    println!("failure_count\t{}", failures.len());
    for failure in &failures {
        println!("failure\t{failure}");
    }

    if !failures.is_empty() {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "{} custom rule test(s) failed",
            failures.len()
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
    ///
    /// Two fixtures, because a rule declares the dialects it applies to and the
    /// dispatcher skips one whose scope excludes the file's: an Emacs Lisp rule
    /// can only be triggered by an Emacs Lisp file, and asking a Common Lisp
    /// fixture to cover one would fail for a reason that has nothing to do with
    /// the fix engine.
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
            "(length (copy-list ucx))\n", // unnecessary-copy
            "(nreverse (copy-list cbd))\n", // copy-before-destructive
            "(code-char 65)\n",        // ascii-code-char
            "(if a b c d)\n",          // if-arity — NOT fixable
        );
        // The Emacs Lisp half. Its rules declare `Dialect::EmacsLisp` only, so
        // they never see the fixture above.
        let elisp_source = ";;; -*- lexical-binding: t -*-\n(mapcar '(lambda (x) x) xs)\n";

        let active: Vec<&str> = RULES.to_vec();
        let mut produced: BTreeSet<&str> = BTreeSet::new();
        for (text, dialect, name) in [
            (source, Dialect::CommonLisp, "fixture.lisp"),
            (elisp_source, Dialect::EmacsLisp, "fixture.el"),
        ] {
            let tree = crate::domain::sexpr::SyntaxTree::parse_with_dialect(text, dialect)
                .expect("parse fixture");
            let fixes = collect_lint_fixes(
                &PathBuf::from(name),
                dialect,
                &tree,
                text,
                &active,
                &RuleSettings::new(),
            )
            .expect("collect fixes");
            produced.extend(fixes.keys().map(|(rule, _, _)| *rule));
        }

        let declared: BTreeSet<&str> = FIXABLE_RULES.iter().copied().collect();
        assert_eq!(
            produced, declared,
            "collect_lint_fixes must produce a fix for exactly the FIXABLE_RULES"
        );
    }
}
