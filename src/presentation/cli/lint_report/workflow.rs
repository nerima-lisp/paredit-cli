use paredit_core_cli::{CliError, CliResult, CommandResult};
use paredit_core_lint_engine::suppression::{Date, LintSuppressions};

use crate::lint::report::{
    CATEGORIES, FindingId, LintFinding, LintPassRequest, LintPolicyOptions, RULES, RuleFilter,
    RuleFixFor, RulePreset, RuleSettings, RuleTag, RuleTimings, Severity, SeverityOverrides,
    apply_severity_override, collect_lint_findings, evaluate_lint_policy, lint_gate_violations,
    resolve_active_rules, rule_setting, rule_tags, rule_timing_report, run_lint_pass,
    summarize_lint_findings,
};
use crate::presentation::cli::lint_report::args::{EmitFormat, LintReportArgs};
use crate::presentation::cli::lint_report::baseline::{BaselineEntry, LintBaseline};
use crate::presentation::cli::lint_report::custom::{self, CustomRules, RuleMetaResolver};
use crate::presentation::cli::lint_report::next_commands::{
    fix_apply_next_commands, fix_plan_next_commands,
};
use crate::presentation::cli::lint_report::render::{
    CustomLintTiming, DensityBucket, FindingIds, LintFileFix, LintFix, LintFixGroupResult,
    LintFixPlanEntry, LintReplacement, LintSarifResult, LintStats, LintSuppressionRemoval,
    LintTiming, SeverityDensitySuggestion, print_custom_lint_explanation, print_lint_docs,
    print_lint_expired_suppressions, print_lint_explanation, print_lint_fix_plan,
    print_lint_fix_report, print_lint_github_annotation, print_lint_presets, print_lint_report,
    print_lint_rule_catalog, print_lint_sarif, print_lint_stats, print_lint_suggest_severity,
    print_lint_suppression_inventory, print_lint_suppression_removal, print_lint_tags,
    print_lint_timings, print_lint_unused_suppressions,
};
use paredit_core_cli::report::FindingSeverity;
use paredit_core_cli::report::interop::{self, Flattened, Row};
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use paredit_feature_change_summary::change_summary::domain::{ChangeSummary, summarise};
use paredit_feature_change_summary::change_summary::prose;
use paredit_feature_project_analysis::impact_report::usecase::collect_impact_definitions;

use crate::presentation::cli::shared::{
    FileFailure, analyze_files, apply_byte_span_edits, expand_input_files,
    note_partial_file_failures, read_input_dialect_and_tree, stable_text_hash, total_file_failure,
    unified_diff, write_file_with_rollback, write_files_with_rollback,
};

/// The group key for a file with no `in-package` of its own.
///
/// Same spelling as `refactor apply --group-by-impact-area`'s
/// `NO_PACKAGE_GROUP`: both commands group by declared package, and a file
/// with none should read the same way from either report.
const NO_PACKAGE_GROUP: &str = "<no-package>";

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

/// Every file `analyze_files` could not analyze failed the same way: none of
/// them did. There is no partial report to produce, so this is the one case
/// where a lint run still fails outright — naming the first failure by input
/// order, the same file a fully serial run would have stopped on first.
fn total_failure(failures: Vec<FileFailure>) -> paredit_core_cli::error::FeatureRefusal {
    let first = failures
        .into_iter()
        .next()
        .expect("total failure has at least one failure");
    paredit_core_cli::error::FeatureRefusal::message(
        paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
        format!(
            "failed to analyze {}: {}",
            first.file.display(),
            first.message
        ),
    )
}

/// Notes, on stderr, any files `analyze_files` could not produce a result
/// for — without failing the command over them. The files that did succeed
/// still have a report worth producing; mirrors the cache-statistics note
/// already printed for the same reason (an aside about the run, not part of
/// the report body).
fn note_partial_failures(failures: &[FileFailure]) {
    if failures.is_empty() {
        return;
    }
    eprintln!(
        "warning: {} of the requested files could not be analyzed and are excluded from this report:",
        failures.len()
    );
    for failure in failures {
        eprintln!("  {}: {}", failure.file.display(), failure.message);
    }
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
fn load_baseline(args: &LintReportArgs) -> CliResult<Option<LintBaseline>> {
    match &args.baseline {
        None => Ok(None),
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(CliError::io(format!(
                "reading baseline file {}",
                path.display()
            )))?;
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
/// [`crate::lint::report::collect_lint_fixes`]); this only reshapes the
/// domain's list into the map the fixpoint loop, SARIF writer, and fix plan
/// all index by. Later entries overwrite earlier ones on an identical key,
/// which is what a rule reporting twice on one span has always resolved to.
fn collect_lint_fixes(
    file: &std::path::Path,
    dialect: paredit_core_syntax::dialect::Dialect,
    tree: &paredit_core_syntax::sexpr::SyntaxTree,
    text: &str,
    active: &[&str],
    settings: &RuleSettings,
) -> CliResult<FixMap> {
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
/// Resolved after the run's custom rules have loaded (FR-E16), so a
/// `lint.deny`/`lint.warn` (or `--deny`/`--warn`) naming one of them resolves
/// against the loaded ruleset rather than only the shipped catalogue. A typo'd
/// rule or category name — one that matches neither — still fails the run
/// rather than quietly changing nothing: the whole point of the flag is to
/// change what fails CI.
///
/// Every custom rule's own declared severity is seeded first, before any
/// selector is applied. Without it, a custom finding's severity for the CI
/// gate would fall through `SeverityOverrides::severity_of` to the shipped
/// catalogue's "unknown name" default of `Error` — silently gating an
/// `--fail-on error` run on a custom rule the project itself shipped at
/// `warning`. An explicit `--deny`/`--warn` naming that rule is applied
/// afterward and still wins, since [`SeverityOverrides::apply`] always
/// inserts over whatever [`SeverityOverrides::seed`] left behind.
fn resolve_severity_overrides(
    args: &LintReportArgs,
    custom: &CustomRules,
) -> CliResult<SeverityOverrides> {
    let mut overrides = SeverityOverrides::new();
    for (name, _category, _description, severity, _fixable) in custom.catalog() {
        overrides.seed(name, severity);
    }
    let custom_rules: Vec<&'static str> = custom.names().collect();
    for selector in &args.warn {
        apply_severity_override(&mut overrides, selector, Severity::Warning, &custom_rules)?;
    }
    // `--deny` is applied second so it wins a same-selector tie, matching the
    // reading that the stricter flag is the deliberate one.
    for selector in &args.deny {
        apply_severity_override(&mut overrides, selector, Severity::Error, &custom_rules)?;
    }
    Ok(overrides)
}

/// Parses every `--rule-arg <rule>.<key>=<value>` against the rules' declared
/// knobs.
///
/// Each part is checked: the rule must exist, it must declare that key, and the
/// value must be an integer. A silently ignored override would leave a CI gate
/// running with a threshold nobody set.
fn resolve_rule_settings(args: &LintReportArgs) -> CliResult<RuleSettings> {
    let mut settings = RuleSettings::new();
    for argument in &args.rule_args {
        let (target, value) = argument.split_once('=').ok_or_else(|| {
            paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
                format!("malformed --rule-arg {argument:?}; expected <rule>.<key>=<value>"),
            )
        })?;
        let (rule, key) = target.rsplit_once('.').ok_or_else(|| {
            paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
                format!("malformed --rule-arg {argument:?}; expected <rule>.<key>=<value>"),
            )
        })?;
        if !RULES.contains(&rule) {
            let suggestion =
                paredit_core_lint_engine::error::did_you_mean(RULES.iter().copied(), rule);
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
                format!("unknown lint rule {rule:?} in --rule-arg {argument:?}{suggestion}"),
            )
            .into());
        }
        let Some(declared) = rule_setting(rule, key) else {
            let valid: Vec<&str> = crate::lint::report::rule_settings(rule)
                .iter()
                .map(|setting| setting.key())
                .collect();
            let suggestion =
                paredit_core_lint_engine::error::did_you_mean(valid.iter().copied(), key);
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
                format!(
                    "lint rule {rule:?} has no setting {key:?}; valid settings: {}{suggestion}",
                    if valid.is_empty() {
                        "(none)".to_owned()
                    } else {
                        valid.join(", ")
                    }
                ),
            )
            .into());
        };
        let parsed: i64 =
            value
                .parse()
                .map_err(|_| paredit_core_cli::ArgumentError::FlagCombination {
                    message: format!(
                        "--rule-arg {argument:?} needs an integer value (the default is {})",
                        declared.default()
                    ),
                })?;
        settings.set(rule, key, parsed);
    }
    Ok(settings)
}

pub(in crate::presentation::cli) fn lint_report(args: LintReportArgs) -> CommandResult {
    // Loaded first — and before any file is read — so a rule file that does
    // not parse fails the run rather than contributing nothing to a green
    // one, and so the rule selectors resolved next (FR-E16) see the loaded
    // ruleset's names alongside the shipped catalogue's.
    let custom = custom::load(args.custom_rules.as_deref())?;
    let custom_names: Vec<&'static str> = custom.names().collect();

    // Resolve the selected rules first so the catalogue-only modes honor the
    // same `--rule`/`--exclude`/`--category`/`--tag`/`--preset` selectors as a
    // scan. Every name is validated here, before any file is read, against
    // both the shipped catalogue and the custom rules just loaded.
    let filter = RuleFilter {
        only: &args.rules,
        exclude: &args.exclude,
        categories: &args.categories,
        tags: &args.tags,
        preset: args.preset.into(),
        experimental: args.experimental,
    };
    let active = resolve_active_rules(&filter, &custom_names)?;
    let overrides = resolve_severity_overrides(&args, &custom)?;
    let settings = resolve_rule_settings(&args)?;
    let meta = RuleMetaResolver::new(&overrides, &custom);

    if args.test_rules {
        return lint_report_test_rules(&args, &custom);
    }

    if let Some(rule) = &args.explain {
        // A custom rule loaded successfully above and already shows up in
        // `--list-rules`/`--docs` through the same catalogue; refusing to
        // explain it here would be the one place it is not indistinguishable
        // from a shipped rule. Checked first since a custom rule's name can
        // never collide with a shipped one (`Ruleset::validate` rejects that
        // at load time), so the order cannot change which branch answers.
        if let Some(found) = custom.rule(rule) {
            return Ok(print_custom_lint_explanation(found, args.output)?);
        }
        if !RULES.contains(&rule.as_str()) {
            let suggestion =
                paredit_core_lint_engine::error::did_you_mean(RULES.iter().copied(), rule);
            return Err(paredit_core_cli::error::FeatureRefusal::message(
    paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
    format!("unknown lint rule {rule:?}; run `inspect lint --list-rules` for the catalogue{suggestion}"),
)
.into());
        }
        return Ok(print_lint_explanation(rule, args.output)?);
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
                resolve_active_rules(&scoped, &[]).map(|rules| (preset, rules.len()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return Ok(print_lint_presets(&counts, args.output)?);
    }

    if args.list_tags {
        return Ok(print_lint_tags(args.output)?);
    }

    if args.docs {
        return Ok(print_lint_docs(&active, &custom)?);
    }

    if args.list_rules {
        return Ok(print_lint_rule_catalog(
            &active,
            &custom,
            args.fixable,
            args.output,
        )?);
    }

    let files = expand_input_files(&args.files, args.dialect)?;
    let files = filter_suppressed_paths(files, &args.suppress_paths);

    if args.timings {
        return Ok(lint_report_timings(
            &args, &files, &active, &settings, &custom,
        )?);
    }

    if args.sarif || args.emit == Some(EmitFormat::Sarif) {
        return lint_report_sarif(&args, &files, &active, &meta, &custom, &settings);
    }

    if args.github || args.emit == Some(EmitFormat::Github) {
        return lint_report_github(&args, &files, &active, &meta, &custom);
    }

    if let Some(format) = args.emit {
        return lint_report_interop(format, &args, &files, &active, &meta, &custom);
    }

    if args.stats {
        return Ok(lint_report_stats(&args, &files, &active, &meta, &custom)?);
    }

    if args.suggest_severity {
        return Ok(lint_report_suggest_severity(
            &args, &files, &active, &meta, &custom,
        )?);
    }

    if args.remove_unused_suppressions {
        return Ok(lint_report_remove_unused_suppressions(&args, &files)?);
    }

    if args.report_unused_suppressions {
        return lint_report_unused_suppressions(&args, &files);
    }

    if args.report_expired_suppressions {
        return lint_report_expired_suppressions(&args, &files);
    }

    if args.report_suppressions {
        return Ok(lint_report_suppressions(&args, &files)?);
    }

    if let Some(out_path) = args.write_baseline.clone() {
        return Ok(lint_report_write_baseline(
            &args, &files, &active, &custom, &out_path,
        )?);
    }

    if args.fix_plan {
        return Ok(lint_report_fix_plan(&args, &files, &active, &settings)?);
    }

    if args.fix {
        return lint_report_fix(&args, &files, &active, &custom, &settings);
    }

    let baseline = load_baseline(&args)?;
    let mut findings = Vec::new();
    let mut ids: FindingIds = Vec::new();

    let cache = open_lint_cache(&args)?;
    // Everything about this request, other than the file's own bytes, that can
    // change what the rules report. Getting this wrong is the one way a
    // content-addressed cache can be wrong, so it is derived from the resolved
    // values rather than from the flags the caller typed.
    let discriminator = lint_cache_discriminator(&active, &custom, &settings);
    let statistics = std::sync::Mutex::new(paredit_core_safety::cache::CacheStatistics::default());
    // FR-E12: how many loaded custom rules a file's dialect put out of scope,
    // summed across the run. `:dialects` is a guard, not a hint — a rule it
    // excludes never even attempts to match — so this is worth reporting
    // rather than leaving a project to notice only that a rule never fires.
    let dialect_skips = std::sync::Mutex::new(0usize);

    // The 170-rule pass over each file is the heaviest per-file work in this
    // tool and has no dependency between files, so it runs on every core.
    // `analyze_files` returns results in input order, which is what keeps the
    // report byte-identical however the workers were scheduled.
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, input| {
        if !custom.is_empty() {
            let skipped = custom.dialect_skip_count(dialect);
            if skipped > 0 {
                if let Ok(mut total) = dialect_skips.lock() {
                    *total += skipped;
                }
            }
        }
        // The cached value is the *pre-baseline* finding set: a baseline is a
        // filter over the answer, not part of the question, so changing one
        // must not throw the analysis away.
        let key = cache
            .as_ref()
            .map(|cache| cache.key("lint", &discriminator, &input.text));

        let unfiltered = match key
            .as_deref()
            .and_then(|key| cache.as_ref().and_then(|cache| cache.get(key)))
            .as_ref()
            .and_then(|cached| decode_cached_findings(cached, file))
        {
            Some(cached) => {
                if let Ok(mut statistics) = statistics.lock() {
                    statistics.hits += 1;
                }
                cached
            }
            None => {
                let computed = run_lint_pass(
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
                let computed = merge_custom(computed, &custom, file, dialect, tree);
                let computed = retain_unsuppressed(computed, &input.text, tree);
                if let (Some(cache), Some(key)) = (cache.as_ref(), key.as_deref()) {
                    let written = cache.put(key, &encode_cached_findings(&computed));
                    if let Ok(mut statistics) = statistics.lock() {
                        statistics.misses += 1;
                        if !written {
                            statistics.write_failures += 1;
                        }
                    }
                } else if let Ok(mut statistics) = statistics.lock() {
                    statistics.misses += 1;
                }
                computed
            }
        };

        let file_findings = retain_unbaselined(unfiltered, &input.text, baseline.as_ref());
        // Ids are assigned per file, against that file's source, and the
        // summary keeps the findings in the order they arrive — so the two
        // lists stay aligned as long as `summarize_lint_findings` only ever
        // filters, which it does.
        let kept: Vec<LintFinding> = file_findings
            .into_iter()
            .filter(|finding| active.contains(&finding.rule))
            .collect();
        let file_ids = assign_finding_ids(&kept, &input.text);
        CliResult::Ok((kept, file_ids))
    });
    if analysis.is_total_failure() {
        return Err(total_failure(analysis.failed).into());
    }
    note_partial_failures(&analysis.failed);
    let file_failures = analysis.failed;

    for (kept, file_ids) in analysis.succeeded {
        ids.extend(file_ids);
        findings.extend(kept);
    }

    // Reported on stderr rather than in the JSON: the cache is an execution
    // detail, and putting it in the report would make the report depend on
    // whether a cache was configured — which is exactly what a cache must
    // never change.
    if cache.is_some() {
        if let Ok(statistics) = statistics.lock() {
            eprintln!(
                "cache: {} hit(s), {} miss(es){}",
                statistics.hits,
                statistics.misses,
                match statistics.write_failures {
                    0 => String::new(),
                    failures => format!(", {failures} unwritable"),
                }
            );
        }
    }
    if let Ok(skipped) = dialect_skips.lock() {
        if *skipped > 0 {
            eprintln!("custom rules: {skipped} rule application(s) skipped by :dialects scope");
        }
    }

    // `active` already carries the eligible custom rule names alongside the
    // shipped ones (FR-E16's widened `resolve_active_rules`), so a rule
    // `lint.disable` excluded is excluded from the checklist here too.
    let summary = summarize_lint_findings(findings, &active);
    let policy = evaluate_lint_policy(
        &overrides,
        LintPolicyOptions::new(args.fail_on_finding, args.fail_on.map(Severity::from)),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_lint_report(&summary, &policy, &ids, &file_failures, &meta, args.output)?;

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
///
/// The loaded custom rules (FR-E5) are measured the same way, serially, for
/// the identical reason — see [`paredit_feature_lint_custom::timed_run`] —
/// and reported as their own section, since they cannot join
/// [`RuleTimings`]: that table is indexed by a rule's compile-time
/// registration position, which a rule read from a file at startup does not
/// have.
fn lint_report_timings(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    settings: &RuleSettings,
    custom: &CustomRules,
) -> CliResult<()> {
    let mut total: Option<RuleTimings> = None;
    let mut custom_total: std::collections::BTreeMap<String, (std::time::Duration, u64)> =
        std::collections::BTreeMap::new();

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
        if let Some(measured) = result.timings {
            match &mut total {
                Some(accumulated) => accumulated.merge(&measured),
                None => total = Some(measured),
            }
        }
        if !custom.is_empty() {
            for (rule, elapsed) in custom.timed_findings(&tree, dialect) {
                let entry = custom_total
                    .entry(rule)
                    .or_insert((std::time::Duration::ZERO, 0));
                entry.0 += elapsed;
                entry.1 += 1;
            }
        }
    }

    let custom_total_micros: u128 = custom_total
        .values()
        .map(|(elapsed, _)| elapsed.as_micros())
        .sum();
    let mut custom_rows: Vec<CustomLintTiming> = custom_total
        .into_iter()
        .filter(|(rule, _)| active.contains(&rule.as_str()))
        .map(|(rule, (elapsed, invocations))| {
            let micros = elapsed.as_micros();
            CustomLintTiming {
                rule,
                micros,
                invocations,
                #[allow(clippy::cast_precision_loss)]
                share: if custom_total_micros == 0 {
                    0.0
                } else {
                    (micros as f64 / custom_total_micros as f64) * 100.0
                },
            }
        })
        .collect();
    custom_rows.sort_by(|a, b| b.micros.cmp(&a.micros).then(a.rule.cmp(&b.rule)));

    let Some(total) = total else {
        return print_lint_timings(
            &[],
            0,
            files.len(),
            &custom_rows,
            custom_total_micros,
            args.output,
        );
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

    print_lint_timings(
        &rows,
        total_micros,
        files.len(),
        &custom_rows,
        custom_total_micros,
        args.output,
    )
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
) -> CliResult<()> {
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
        SyntaxTree::parse_with_dialect(&text, dialect).map_err(|_| {
            paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
                "refusing to edit: source with suppressions removed does not reparse",
            )
        })?;

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
    dialect: paredit_core_syntax::dialect::Dialect,
    tree: &SyntaxTree,
    text: &str,
) -> CliResult<std::collections::HashMap<usize, std::collections::HashSet<&'static str>>> {
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
) -> CommandResult {
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
    let analysis = analyze_files(files, args.dialect, |file, dialect, tree, input| {
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
        let findings = merge_custom(pass.findings, custom, file, dialect, tree);
        custom_fixes(custom, file, dialect, tree, active, &mut fixes);
        let findings = retain_unsuppressed(findings, &input.text, tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let findings: Vec<LintFinding> = findings
            .into_iter()
            .filter(|finding| active.contains(&finding.rule))
            .collect();
        let ids = assign_finding_ids(&findings, &input.text);
        CliResult::Ok((findings, ids, fixes, input.text.clone()))
    });
    if analysis.is_total_failure() {
        return Err(total_failure(analysis.failed).into());
    }
    note_partial_failures(&analysis.failed);

    for (findings, ids, fixes, text) in analysis.succeeded {
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
) -> CliResult<()> {
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

    let next_commands = fix_plan_next_commands(entries.len(), files);
    print_lint_fix_plan(&entries, &next_commands, args.output)?;
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
) -> CommandResult {
    let mut file_fixes = Vec::new();
    let mut fixes_applied = 0;
    let mut fix_conflicts = 0;
    // How many destructive-tagged fixes `--no-destructive-fixes` left on the
    // table, across every file — a snapshot of the *converged* text's own
    // remaining fixes, not a per-pass tally, so it names exactly what this
    // run deliberately did not apply.
    let mut fix_skipped_destructive = 0;
    // Fed straight from the before/after text this loop already reads and
    // rewrites — never a second diff, mirroring `refactor apply`'s headline.
    // A file whose text does not compare (rare: `summarise` re-parses both
    // sides, and the rewrite already had to reparse to be adopted at all) is
    // silently left out of the headline rather than guessed at.
    let mut change_summaries: Vec<ChangeSummary> = Vec::new();
    // Deferred writes for `--group-by-impact-area`: every file this run would
    // otherwise have written immediately, paired with its declared-package
    // group key. Empty, and never consulted, on every other run.
    let mut deferred_writes: Vec<(std::path::PathBuf, String, String)> = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let mut text = input.text.clone();
        let mut tree = tree;
        // The group key is the file's own declared package, read from its
        // content before any fix touches it — the same "declared package as
        // impact-area" policy `refactor apply --group-by-impact-area`
        // established, so the two commands group files the same way.
        let group_key = if args.group_by_impact_area {
            collect_impact_definitions(&tree, dialect)
                .ok()
                .and_then(|(package, _definitions)| package)
                .unwrap_or_else(|| NO_PACKAGE_GROUP.to_owned())
        } else {
            String::new()
        };
        let mut per_rule: std::collections::BTreeMap<&'static str, usize> =
            std::collections::BTreeMap::new();
        let mut applied = 0;
        // How often two fixes wanted overlapping regions in one pass. The
        // shadowed one is not lost — the next pass re-offers it once the outer
        // form has been rewritten — so this measures contention, not backlog.
        let mut conflicts = 0;
        // How many destructive-tagged fixes were still on the table on the
        // *last* pass this loop ran. Overwritten every pass rather than
        // accumulated, so it reflects the converged text's own remaining
        // fixes and not a sum across every pass that ever saw one.
        let mut skipped_destructive = 0;

        for _ in 0..MAX_FIX_PASSES {
            let mut fixes = collect_lint_fixes(file, dialect, &tree, &text, active, settings)?;
            custom_fixes(custom, file, dialect, &tree, active, &mut fixes);
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
                let before = fixes.len();
                fixes.retain(|(rule, _, _), _| !rule_tags(rule).contains(RuleTag::Destructive));
                skipped_destructive = before - fixes.len();
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
            tree = SyntaxTree::parse_with_dialect(&rewritten, dialect).map_err(|_| {
                paredit_core_cli::error::FeatureRefusal::message(
                    paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,
                    "refusing to fix: rewritten source does not reparse",
                )
            })?;
            text = rewritten;
            for rule in chosen_rules {
                *per_rule.entry(rule).or_insert(0) += 1;
                applied += 1;
            }
        }
        fix_conflicts += conflicts;
        fix_skipped_destructive += skipped_destructive;

        if applied > 0 && text != input.text {
            if args.diff {
                // Preview only: the unified diff is the payload (stdout, so it
                // pipes to a file/pager), and nothing is written.
                print!(
                    "{}",
                    paredit_core_cli::color::colorize_diff(
                        paredit_core_cli::color::Painter::stdout(),
                        &unified_diff(file, &input.text, &text)
                    )
                );
            } else if !args.check {
                if let Some(summary) = summarise(&input.text, &text, dialect) {
                    change_summaries.push(summary);
                }
                if args.group_by_impact_area {
                    deferred_writes.push((file.clone(), text.clone(), group_key));
                } else {
                    write_file_with_rollback(file.clone(), text)?;
                }
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

    // Only ever non-empty with `--group-by-impact-area`; every other run
    // reports no groups, mirroring `refactor apply`. Reached only when
    // neither `--check` nor `--diff` returned early above, so every deferred
    // write here really is one this run means to make.
    let mut impact_area_groups: Vec<LintFixGroupResult> = Vec::new();
    if args.group_by_impact_area {
        let mut file_failures: Vec<FileFailure> = Vec::new();
        let mut any_written = false;
        for (group, members) in group_writes_by_key(deferred_writes) {
            let file_count = members.len();
            // Each group is its own all-or-nothing transaction — the same
            // atomicity `write_files_with_rollback` already gives a single
            // `--fix` run without grouping. Grouping changes how many
            // transactions there are, not what one transaction does: a group
            // that fails is left exactly as it was before this run, and the
            // groups that already wrote stay written rather than being
            // rolled back over a *later* group's failure, since undoing a
            // successful group to punish an unrelated one is the
            // all-or-nothing behavior this flag exists to opt out of.
            match write_files_with_rollback(members.clone()) {
                Ok(()) => {
                    any_written = true;
                    impact_area_groups.push(LintFixGroupResult {
                        group,
                        file_count,
                        written: true,
                        failure: None,
                    });
                }
                Err(error) => {
                    let message = paredit_core_cli::error::error_chain(&error);
                    for (path, _content) in &members {
                        file_failures.push(FileFailure {
                            file: path.clone(),
                            message: message.clone(),
                        });
                    }
                    impact_area_groups.push(LintFixGroupResult {
                        group,
                        file_count,
                        written: false,
                        failure: Some(message),
                    });
                }
            }
        }

        // The same total-failure threshold `migrate run` and `refactor apply
        // --group-by-impact-area` use: only when nothing at all survived does
        // this refuse outright, since a run with even one written group has a
        // partial result worth reporting.
        if !any_written && !file_failures.is_empty() {
            return Err(total_file_failure(file_failures).into());
        }
        note_partial_file_failures(&file_failures);
    }

    let headline = build_fix_headline(&change_summaries, file_fixes.len());
    let next_commands = fix_apply_next_commands(fix_skipped_destructive, files);

    Ok(print_lint_fix_report(
        &file_fixes,
        fixes_applied,
        fix_conflicts,
        &headline,
        &impact_area_groups,
        &next_commands,
        args.compact,
        args.output,
    )?)
}

/// Partitions `writes` by their own group key, in first-seen group order.
///
/// First-seen rather than sorted: `files` lists the run's inputs in an order
/// the caller chose (or `expand_input_files` discovered them in), and
/// preserving it is what makes "continues to the next group if one fails"
/// mean something predictable rather than an alphabetical accident. Mirrors
/// `refactor apply --group-by-impact-area`'s `group_indexes_by_key`.
fn group_writes_by_key(
    writes: Vec<(std::path::PathBuf, String, String)>,
) -> Vec<(String, Vec<(std::path::PathBuf, String)>)> {
    let mut groups: Vec<(String, Vec<(std::path::PathBuf, String)>)> = Vec::new();
    for (path, content, key) in writes {
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, members)) => members.push((path, content)),
            None => groups.push((key, vec![(path, content)])),
        }
    }
    groups
}

/// The one-line "N added, M renamed ... definitions." for every file this fix
/// run changed, combined.
///
/// Mirrors `refactor apply`'s `build_apply_headline`: never a second
/// summarizer, only a concatenation of what `summarise` (the same comparison
/// `inspect change` uses) already found per file, handed to `prose::headline`
/// unmodified.
fn build_fix_headline(change_summaries: &[ChangeSummary], changed_file_count: usize) -> String {
    if changed_file_count == 0 {
        return prose::headline(&ChangeSummary {
            changes: Vec::new(),
            identical: true,
            formatting_only: false,
            before_definitions: 0,
            after_definitions: 0,
        });
    }
    if change_summaries.is_empty() {
        // Every changed file's before/after text failed to compare (rare: the
        // rewrite already had to reparse to be adopted at all, so this is the
        // before side). Silence here would read as "nothing changed", which
        // is false.
        return format!(
            "{changed_file_count} file(s) changed; no per-definition summary was available."
        );
    }
    let merged = ChangeSummary {
        changes: change_summaries
            .iter()
            .flat_map(|summary| summary.changes.clone())
            .collect(),
        identical: false,
        formatting_only: change_summaries
            .iter()
            .all(|summary| summary.changes.is_empty()),
        before_definitions: change_summaries
            .iter()
            .map(|summary| summary.before_definitions)
            .sum(),
        after_definitions: change_summaries
            .iter()
            .map(|summary| summary.after_definitions)
            .sum(),
    };
    prose::headline(&merged)
}

#[cfg(test)]
mod fix_group_tests {
    use super::*;

    #[test]
    fn writes_are_partitioned_by_key_in_first_seen_group_order() {
        let writes = vec![
            (
                std::path::PathBuf::from("a.lisp"),
                "(a)".to_owned(),
                "app".to_owned(),
            ),
            (
                std::path::PathBuf::from("u.lisp"),
                "(u)".to_owned(),
                "util".to_owned(),
            ),
            (
                std::path::PathBuf::from("b.lisp"),
                "(b)".to_owned(),
                "app".to_owned(),
            ),
        ];
        let groups = group_writes_by_key(writes);
        assert_eq!(
            groups
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            vec!["app".to_owned(), "util".to_owned()]
        );
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn an_empty_write_list_produces_no_groups() {
        assert!(group_writes_by_key(Vec::new()).is_empty());
    }
}

#[cfg(test)]
mod fix_headline_tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    #[test]
    fn no_changed_files_reports_no_change() {
        assert_eq!(
            build_fix_headline(&[], 0),
            "No change: the two versions are identical."
        );
    }

    #[test]
    fn changed_files_with_no_comparable_summary_says_so_rather_than_nothing() {
        let headline = build_fix_headline(&[], 2);
        assert!(headline.contains('2'), "{headline}");
        assert!(!headline.contains("identical"), "{headline}");
    }

    #[test]
    fn one_file_s_summary_passes_straight_through() {
        let summary = summarise(
            "(defun old-name (x) x)\n",
            "(defun new-name (x) x)\n",
            Dialect::CommonLisp,
        )
        .expect("parses");
        let headline = build_fix_headline(&[summary], 1);
        assert_eq!(headline, "1 renamed definition.");
    }

    #[test]
    fn several_files_combine_into_one_headline() {
        let a = summarise(
            "(defun f (x) x)\n",
            "(defun f (x) x)\n(defun g (y) y)\n",
            Dialect::CommonLisp,
        )
        .expect("parses");
        let b = summarise("(defun h (z) z)\n", "", Dialect::CommonLisp).expect("parses");
        let headline = build_fix_headline(&[a, b], 2);
        assert!(headline.contains("added"), "{headline}");
        assert!(headline.contains("removed"), "{headline}");
    }
}

/// Writes the current findings (for the active rules plus any custom rules,
/// after suppression) to a baseline file, so a later `--baseline` run can gate
/// only on new findings.
/// Prints a one-line summary and exits 0.
///
/// Custom findings are merged in exactly as [`lint_report_stats`] and the main
/// scan do: a rule that is "indistinguishable from a shipped one" everywhere
/// downstream (see `custom.rs`'s module doc) must also be something
/// `--write-baseline` can capture, or a later `--baseline` run has no entry to
/// suppress it with.
fn lint_report_write_baseline(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    custom: &CustomRules,
    out_path: &std::path::Path,
) -> CliResult<()> {
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings = merge_custom(
            collect_lint_findings(file, dialect, &tree)?,
            custom,
            file,
            dialect,
            &tree,
        );
        let findings = retain_unsuppressed(findings, &input.text, &tree);
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
    std::fs::write(out_path, baseline.to_json()?).map_err(paredit_core_cli::CliError::io(
        format!("writing baseline file {}", out_path.display()),
    ))?;

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
) -> CliResult<()> {
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
            dialect,
            &tree,
        );
        let findings = retain_unsuppressed(findings, &input.text, &tree);
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

/// The findings-per-file cutoffs a rule's density is sorted into, in findings
/// per file scanned. Round order-of-magnitude numbers rather than a tuned
/// model: `--suggest-severity` is advisory, and a threshold a reader cannot
/// sanity-check in their head would undercut that.
const VERY_HIGH_DENSITY: f64 = 1.0;
const HIGH_DENSITY: f64 = 0.1;
const MODERATE_DENSITY: f64 = 0.01;
const LOW_DENSITY: f64 = 0.001;

/// Sorts a positive density into its [`DensityBucket`]. Only called with
/// `finding_count > 0`; a rule with no findings is [`DensityBucket::Never`],
/// decided by the caller before density is even computed (dividing by
/// `files_scanned` would still work, but "never fired" is the more honest
/// thing to print than "very low density").
fn density_bucket(density: f64) -> DensityBucket {
    if density >= VERY_HIGH_DENSITY {
        DensityBucket::VeryHigh
    } else if density >= HIGH_DENSITY {
        DensityBucket::High
    } else if density >= MODERATE_DENSITY {
        DensityBucket::Moderate
    } else if density >= LOW_DENSITY {
        DensityBucket::Low
    } else {
        DensityBucket::VeryLow
    }
}

/// The severity a rule's firing rate suggests instead of its current one, or
/// `None` when the two agree (the common case — most rules need no
/// suggestion).
///
/// Only two directions are ever suggested, deliberately:
/// - An `Error` rule firing at `High` or `VeryHigh` density is too noisy to be
///   gating a build on likely/certain bugs; consider `Warning`.
/// - A `Warning` rule that never fired across the whole scanned workspace is
///   either dead weight or, if it ever does fire, rare enough that missing it
///   would be worth failing over; consider `Error`.
///
/// A `Warning` firing constantly, or an `Error` that never fires, is each
/// rule's expected shape and gets no suggestion.
const fn suggested_severity(
    current: Severity,
    finding_count: usize,
    bucket: DensityBucket,
) -> Option<Severity> {
    match (current, finding_count, bucket) {
        (Severity::Error, count, DensityBucket::VeryHigh | DensityBucket::High) if count > 0 => {
            Some(Severity::Warning)
        }
        (Severity::Warning, 0, DensityBucket::Never) => Some(Severity::Error),
        _ => None,
    }
}

/// Runs a normal lint scan and, for every rule that fired at least once,
/// computes its findings-per-file density across the scanned workspace (see
/// [`SeverityDensitySuggestion::density`]); for every rule that never fired,
/// notes that too. Reports only the rules whose density disagrees with their
/// current severity — see [`suggested_severity`] for the two directions this
/// ever recommends.
///
/// **Advisory only.** This reuses the same finding-collection machinery
/// (`collect_lint_findings`, suppressions, `--baseline`) as `--stats`, on data
/// already computed for a scan; it never re-derives severities, never writes
/// `paredit.toml`, and always returns `Ok` regardless of what it finds — a
/// `--suggest-severity` run cannot fail a build the way `--fail-on`/`--fix
/// --check` can, because a suggestion is not a policy.
fn lint_report_suggest_severity(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&'static str],
    meta: &RuleMetaResolver<'_>,
    custom: &CustomRules,
) -> CliResult<()> {
    let baseline = load_baseline(args)?;
    let mut finding_counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut files_with_finding: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings = merge_custom(
            collect_lint_findings(file, dialect, &tree)?,
            custom,
            file,
            dialect,
            &tree,
        );
        let findings = retain_unsuppressed(findings, &input.text, &tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let mut fired_in_file: std::collections::HashSet<&'static str> =
            std::collections::HashSet::new();
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            *finding_counts.entry(finding.rule).or_insert(0) += 1;
            fired_in_file.insert(finding.rule);
        }
        for rule in fired_in_file {
            *files_with_finding.entry(rule).or_insert(0) += 1;
        }
    }

    let files_scanned = files.len();
    #[allow(clippy::cast_precision_loss)]
    let denominator = files_scanned.max(1) as f64;

    let mut suggestions = Vec::new();
    for rule in active.iter().copied() {
        let finding_count = finding_counts.get(rule).copied().unwrap_or(0);
        let current_severity = meta.severity(rule);
        let (bucket, density) = if finding_count == 0 {
            (DensityBucket::Never, 0.0)
        } else {
            #[allow(clippy::cast_precision_loss)]
            let density = finding_count as f64 / denominator;
            (density_bucket(density), density)
        };
        let Some(suggested) = suggested_severity(current_severity, finding_count, bucket) else {
            continue;
        };
        suggestions.push(SeverityDensitySuggestion {
            rule,
            category: meta.category(rule),
            current_severity,
            suggested_severity: suggested,
            finding_count,
            files_with_finding: files_with_finding.get(rule).copied().unwrap_or(0),
            files_scanned,
            density,
            bucket,
        });
    }
    // Deterministic regardless of finding order: alphabetical by rule.
    suggestions.sort_by_key(|entry| entry.rule);

    print_lint_suggest_severity(&suggestions, files_scanned, args.output)
}

/// Reports every inline `; paredit:ignore` directive that silences no finding
/// (a stale ignore or a typo'd rule name). Detection runs against *all* rules —
/// independent of `--rule`/`--exclude` — so an ignore is "unused" only when it
/// matches no finding the file actually has, and the run exits 3 if any are
/// found so CI can keep the ignore list clean.
fn lint_report_unused_suppressions(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
) -> CommandResult {
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

/// Reports every inline suppression directive whose `-until` date has passed,
/// whether or not it currently silences anything. Unlike
/// [`lint_report_unused_suppressions`], use is irrelevant here: a directive
/// still actively hiding a finding past its own deadline is the one that most
/// needs a human decision. Exits 3 if any are found.
fn lint_report_expired_suppressions(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
) -> CommandResult {
    let today = Date::today();
    let mut entries = Vec::new();

    for file in files {
        let (input, _dialect, tree) =
            read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let suppressions = LintSuppressions::parse_in_tree(&input.text, &tree);
        for expired in suppressions.expired_directives(today) {
            entries.push((file.display().to_string(), expired));
        }
    }

    let expired_count = entries.len();
    print_lint_expired_suppressions(&entries, args.output)?;

    if expired_count > 0 {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {expired_count} expired suppression(s)"
        )));
    }

    Ok(())
}

/// Lists every inline suppression directive across the scanned files, used or
/// not — the full survey `--report-suppressions` provides, one step past
/// `--report-unused-suppressions`'s stale-only view. Always exits 0: a survey,
/// not a gate.
fn lint_report_suppressions(args: &LintReportArgs, files: &[std::path::PathBuf]) -> CliResult<()> {
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let present = findings_by_line(file, dialect, &tree, &input.text)?;
        let suppressions = LintSuppressions::parse_in_tree(&input.text, &tree);
        for entry in suppressions.inventory(&present) {
            entries.push((file.display().to_string(), entry));
        }
    }

    print_lint_suppression_inventory(&entries, args.output)
}

/// Drops files under any `--suppress-path` prefix, so lint findings never
/// surface for generated code or vendored dependencies a project cannot edit
/// to carry an inline `paredit:ignore`. Scoped to `inspect lint` alone; other
/// commands still see these files — `paths.exclude` is the flag for hiding a
/// path everywhere.
fn filter_suppressed_paths(
    files: Vec<std::path::PathBuf>,
    prefixes: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    if prefixes.is_empty() {
        return files;
    }
    let normalized: Vec<std::path::PathBuf> = prefixes.iter().map(|p| normalize(p)).collect();
    files
        .into_iter()
        .filter(|file| {
            let candidate = normalize(file);
            !normalized
                .iter()
                .any(|prefix| candidate.starts_with(prefix))
        })
        .collect()
}

/// The absolute, canonical form of `path`, for a prefix comparison that a
/// relative invocation directory or a `..` cannot fool. Falls back to a plain
/// `current_dir`-joined path when `path` does not exist (a `--suppress-path`
/// is allowed to name a directory with nothing under it yet).
fn normalize(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(path))
}

/// Emits findings in one of the interchange formats the report envelope
/// already knows how to write.
///
/// Lint findings are not `FileFindings`, so this builds the flattened rows the
/// envelope's emitters consume directly rather than routing through a trait
/// impl. That is the whole adapter: a lint finding is already a rule, a path, a
/// span, and a message, which is exactly a row.
///
/// SARIF and GitHub annotations are deliberately *not* handled here. Both have
/// a richer lint-specific rendering — SARIF advertises the whole rule catalogue
/// and carries the auto-fixes, GitHub carries a column — and reaching them
/// through the generic path would be a downgrade.
fn lint_report_interop(
    format: EmitFormat,
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    meta: &RuleMetaResolver<'_>,
    custom: &CustomRules,
) -> CommandResult {
    let baseline = load_baseline(args)?;
    let mut rows = Vec::new();
    let mut finding_rules: Vec<&'static str> = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings = merge_custom(
            collect_lint_findings(file, dialect, &tree)?,
            custom,
            file,
            dialect,
            &tree,
        );
        let findings = retain_unsuppressed(findings, &input.text, &tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            finding_rules.push(finding.rule);
            let start = finding.span.start().get();
            let (line, _) = line_and_column(&input.text, start);
            rows.push(Row {
                path: finding.path.display().to_string(),
                dialect: dialect.label(),
                kind: finding.rule,
                severity: match meta.severity(finding.rule) {
                    Severity::Error => FindingSeverity::Error,
                    Severity::Warning => FindingSeverity::Warning,
                },
                line,
                span_start: start,
                span_end: finding.span.end().get(),
                message: finding.message,
                fields: vec![("category", serde_json::json!(meta.category(finding.rule)))],
            });
        }
    }

    let gate = gate_message(&finding_rules, args, meta.overrides());
    let flat = Flattened {
        command: "inspect lint",
        rows,
        // Every dialect this build parses has at least one applicable rule, so
        // lint has no "not examined" class to report.
        skipped: Vec::new(),
        file_count: files.len(),
        gate: gate_flag(args),
        gate_passed: gate.is_none(),
        violations: gate.iter().cloned().collect(),
    };

    match format {
        EmitFormat::Junit => print!("{}", interop::junit(&flat)),
        EmitFormat::CodeClimate => println!(
            "{}",
            serde_json::to_string_pretty(&interop::code_climate(&flat))?
        ),
        EmitFormat::Csv => print!("{}", interop::delimited(&flat, true)),
        EmitFormat::Tsv => print!("{}", interop::delimited(&flat, false)),
        EmitFormat::Html => print!("{}", interop::html(&flat)),
        EmitFormat::Markdown => print!("{}", interop::markdown(&flat)),
        // Routed to their richer lint-specific renderers before this call.
        EmitFormat::Sarif | EmitFormat::Github => unreachable!(),
    }

    if let Some(message) = gate {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {message}"
        )));
    }

    Ok(())
}

/// Which flag armed the gate, for the formats that report it.
const fn gate_flag(args: &LintReportArgs) -> Option<&'static str> {
    if args.fail_on.is_some() {
        Some("--fail-on")
    } else if args.fail_on_finding {
        Some("--fail-on-finding")
    } else {
        None
    }
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
) -> CommandResult {
    let baseline = load_baseline(args)?;
    let mut finding_rules: Vec<&'static str> = Vec::new();

    // Analysed in parallel, *printed* serially: an annotation stream that
    // interleaved by thread would be unreadable and, worse, unstable between
    // runs. The split is the general shape for adopting `analyze_files` in a
    // command that emits as it goes — compute per file, emit in file order.
    let analysis = analyze_files(files, args.dialect, |file, dialect, tree, input| {
        let findings = merge_custom(
            collect_lint_findings(file, dialect, tree)?,
            custom,
            file,
            dialect,
            tree,
        );
        let findings = retain_unsuppressed(findings, &input.text, tree);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        CliResult::Ok(
            findings
                .into_iter()
                .filter(|finding| active.contains(&finding.rule))
                .map(|finding| {
                    let (line, column) = line_and_column(&input.text, finding.span.start().get());
                    (finding, line, column)
                })
                .collect::<Vec<_>>(),
        )
    });
    if analysis.is_total_failure() {
        return Err(total_failure(analysis.failed).into());
    }
    note_partial_failures(&analysis.failed);

    for (finding, line, column) in analysis.succeeded.into_iter().flatten() {
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
// The incremental cache.
//
// A lint pass runs 170 rules over every file, every time; on a re-run almost
// all of that is spent rediscovering that nothing changed. The cache is
// content-addressed, so a hit means "this exact question was answered", never
// "a file with this name was seen". See `paredit_core_safety::cache`.
// ---------------------------------------------------------------------------

/// Opens the cache the caller asked for, or `None`.
///
/// A cache directory that cannot be created is an error rather than a silent
/// fallback: the caller asked for a cache, and a run that quietly does not use
/// one looks identical to a run that does, only slower — which is the hardest
/// kind of configuration mistake to notice.
fn open_lint_cache(
    args: &LintReportArgs,
) -> CliResult<Option<paredit_core_safety::cache::AnalysisCache>> {
    args.cache_dir
        .as_deref()
        .map(|root| {
            paredit_core_safety::cache::AnalysisCache::open(root, env!("CARGO_PKG_VERSION"))
                .map_err(paredit_core_cli::CliError::io(format!(
                    "opening lint cache {}",
                    root.display()
                )))
        })
        .transpose()
}

/// Everything other than the file's bytes that changes what the rules report.
///
/// The active rule *names* rather than the flags that selected them: `--preset
/// recommended` and an explicit `--rule` list that happens to name the same
/// rules are the same question and should share an entry. The custom rules are
/// keyed by their source, since a project can edit a rule without renaming it.
fn lint_cache_discriminator(
    active: &[&str],
    custom: &CustomRules,
    settings: &RuleSettings,
) -> String {
    let mut parts = vec![format!("rules={}", active.join(","))];
    parts.push(format!(
        "settings={}",
        settings
            .entries()
            .map(|(rule, key, value)| format!("{rule}.{key}={value}"))
            .collect::<Vec<_>>()
            .join(",")
    ));
    // Custom rules are keyed by name *and* by what each one reports on a
    // canonical probe, so editing a rule's pattern without renaming it changes
    // the key. Name alone would let an edited rule serve stale findings.
    parts.push(format!(
        "custom={}",
        stable_text_hash(&custom.test().join("\n"))
    ));
    parts.join("|")
}

/// A file's findings, in the cache's on-disk form.
///
/// **The path is deliberately not stored.** The key is the file's *content*,
/// so two files with identical bytes share one entry — which is correct and
/// is the reason a renamed or copied file hits. Storing the path in the value
/// would then serve whichever duplicate wrote the entry first, and every other
/// copy would be reported under the wrong name. A 1065-file corpus containing
/// 135 duplicates surfaced exactly that, as findings whose path belonged to a
/// different file.
///
/// The path is context, not result. It is re-attached on decode from the file
/// actually being reported.
///
/// `rule` is a `&'static str` in memory and a string on disk, so decoding has
/// to map it back onto the registry — a cached name that no longer exists
/// makes the whole entry a miss rather than a finding with a dangling rule.
fn encode_cached_findings(findings: &[LintFinding]) -> serde_json::Value {
    serde_json::json!({
        "findings": findings
            .iter()
            .map(|finding| serde_json::json!({
                "rule": finding.rule,
                "start": finding.span.start().get(),
                "end": finding.span.end().get(),
                "message": finding.message,
            }))
            .collect::<Vec<_>>(),
    })
}

/// Reads a cached entry back, or `None` when anything about it is unusable.
fn decode_cached_findings(
    value: &serde_json::Value,
    path: &std::path::Path,
) -> Option<Vec<LintFinding>> {
    value
        .get("findings")?
        .as_array()?
        .iter()
        .map(|entry| {
            let name = entry.get("rule")?.as_str()?;
            // The `&'static str` the rest of the pipeline expects, and the
            // check that this build still has the rule. A cached name that no
            // longer exists makes the whole entry a miss, which is the right
            // answer: the analysis that produced it is not this one.
            let rule = RULES.iter().copied().find(|candidate| *candidate == name)?;
            let start = usize::try_from(entry.get("start")?.as_u64()?).ok()?;
            let end = usize::try_from(entry.get("end")?.as_u64()?).ok()?;
            Some(LintFinding {
                rule,
                path: path.to_path_buf(),
                span: ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end)),
                message: entry.get("message")?.as_str()?.to_owned(),
            })
        })
        .collect()
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
    dialect: paredit_core_syntax::dialect::Dialect,
    tree: &SyntaxTree,
) -> Vec<LintFinding> {
    if custom.is_empty() {
        return findings;
    }
    for (rule, finding) in custom.findings(tree, dialect) {
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
///
/// Only for a rule in `active`: `lint.disable`/`--exclude` (FR-E16) excludes a
/// custom rule's fixes the same way it excludes a shipped rule's — offering a
/// fix for a rule the run just excluded would be the one output mode where
/// exclusion did not hold.
fn custom_fixes(
    custom: &CustomRules,
    _file: &std::path::Path,
    dialect: paredit_core_syntax::dialect::Dialect,
    tree: &SyntaxTree,
    active: &[&str],
    fixes: &mut FixMap,
) {
    if custom.is_empty() {
        return;
    }
    for (rule, finding) in custom.findings(tree, dialect) {
        if !active.contains(&rule) {
            continue;
        }
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

/// Runs the `deftest` clauses in the custom rule files.
///
/// A separate mode rather than part of a scan: a rule file is code, and code
/// nobody can check goes wrong quietly. Exits 3 on any failure so CI can keep
/// a project's own rules correct the same way it keeps its own tests correct.
///
/// Honors `--output`: text keeps the original tab-separated lines, and JSON
/// emits one object per [`paredit_feature_lint_custom::TestFailure`] (`rule`,
/// `clause`, `input`, `expected`, `actual`) instead of flattening it into a
/// single string, so a caller parsing the output does not have to re-split
/// the text-mode line.
fn lint_report_test_rules(args: &LintReportArgs, custom: &CustomRules) -> CommandResult {
    let failures = custom.test_failures();
    let rule_count = custom.ruleset().rules.len();
    let test_count = custom.ruleset().tests.len();

    match args.output {
        crate::presentation::cli::OutputFormat::Text => {
            println!("custom_rule_count\t{rule_count}");
            println!("custom_test_count\t{test_count}");
            println!("failure_count\t{}", failures.len());
            for failure in &failures {
                println!(
                    "failure\t{}\t{}\t{}\texpected {}\tgot {}",
                    failure.rule, failure.clause, failure.input, failure.expected, failure.actual
                );
            }
        }
        crate::presentation::cli::OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "custom_rule_count": rule_count,
                    "custom_test_count": test_count,
                    "failure_count": failures.len(),
                    "failures": failures
                        .iter()
                        .map(|failure| serde_json::json!({
                            "rule": failure.rule,
                            "clause": failure.clause,
                            "input": failure.input,
                            "expected": failure.expected,
                            "actual": failure.actual,
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
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
    use crate::lint::report::{FIXABLE_RULES, RULES};
    use paredit_core_syntax::dialect::Dialect;
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
            "(princ lpd)\n",           // leftover-print-debug
            "(trace ltc)\n",           // leftover-trace-call
            "(break)\n",               // leftover-break-call
            "(inspect lic)\n",         // leftover-inspect-call
            "(time (ltbc))\n",         // leftover-time-benchmark-call
            "(step (lsc))\n",          // leftover-step-call
            "(format t \"DEBUG lfdm\")\n", // leftover-format-debug-marker
            "(if a b c d)\n",          // if-arity — NOT fixable
        );
        // The Emacs Lisp half. Its rules declare `Dialect::EmacsLisp` only, so
        // they never see the fixture above.
        let elisp_source = ";;; -*- lexical-binding: t -*-\n(mapcar '(lambda (x) x) xs)\n";
        // The Scheme half, for the same reason: `lint-scheme-idiom`'s four
        // rules declare `[Scheme, Racket]` (or Scheme alone), so neither
        // fixture above reaches them. They are also the only fixable rules
        // outside Common Lisp and Emacs Lisp, which is what made this test the
        // one that noticed them — a `Fixability::Fixable` the fix engine never
        // exercises is a `--fix` that silently does nothing.
        let scheme_source = concat!(
            "(begin (sbs-body))\n",                       // scheme-begin-single-form
            "(let* ((slsa 1) (slsb 2)) (+ slsa slsb))\n", // scheme-let-star-independent-bindings
            "(memq 101 smk)\n",                           // scheme-memq-assq-literal-key
            "(let sln ((slni 0)) (* slni 2))\n",          // scheme-named-let-never-recurs
        );
        // The Carp half, added for the same reason the Scheme half was: its one
        // fixable rule is `Dialect::Carp` only, so no fixture above reaches it.
        // Note `=>` must not be shadowed by a local `->` definition in this
        // file — the rule withholds its fix when it is, which would make this
        // test fail for a reason that has nothing to do with the fix engine.
        let carp_source = "(=> cdtm (f) (g))\n"; // carp-deprecated-thread-macro
        // The Racket half, added for the third time this test has caught the
        // same class. `lint-racket-depth`'s two fixable rules declare
        // `[Dialect::Racket]` *alone*, where `lint-scheme-idiom`'s declare
        // `[Scheme, Racket]` — so the Scheme fixture above parses as
        // `Dialect::Scheme` and reaches neither of them. Without this fixture
        // both are `Fixability::Fixable` rules the fix engine never exercises,
        // which is a `--fix` that silently does nothing.
        let racket_source = concat!(
            "(begin0 (rb0-body))\n",                 // racket-begin0-single-form
            "(case-lambda [(rcl-x) (* rcl-x 2)])\n", // racket-case-lambda-single-clause
        );

        let active: Vec<&str> = RULES.to_vec();
        let mut produced: BTreeSet<&str> = BTreeSet::new();
        for (text, dialect, name) in [
            (source, Dialect::CommonLisp, "fixture.lisp"),
            (elisp_source, Dialect::EmacsLisp, "fixture.el"),
            (scheme_source, Dialect::Scheme, "fixture.scm"),
            (carp_source, Dialect::Carp, "fixture.carp"),
            (racket_source, Dialect::Racket, "fixture.rkt"),
        ] {
            let tree = paredit_core_syntax::sexpr::SyntaxTree::parse_with_dialect(text, dialect)
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
