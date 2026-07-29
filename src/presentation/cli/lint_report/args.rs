use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::lint::report::{RulePreset, Severity};
use crate::presentation::cli::{DialectArg, OutputFormat};

/// The minimum severity that trips the `--fail-on` gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::presentation::cli) enum FailSeverity {
    /// Fail only on error-severity findings (likely/certain bugs).
    Error,
    /// Fail on any finding (warnings and errors).
    Warning,
}

impl From<FailSeverity> for Severity {
    fn from(value: FailSeverity) -> Self {
        match value {
            FailSeverity::Error => Severity::Error,
            FailSeverity::Warning => Severity::Warning,
        }
    }
}

/// How wide a net `--preset` casts.
///
/// A `clap` mirror of [`RulePreset`] rather than a `ValueEnum` derived on the
/// domain type, so the domain package keeps no dependency on the argument
/// parser. The `From` below is the only place the two spellings meet, and it is
/// exhaustive, so a preset added to one and not the other fails to compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub(in crate::presentation::cli) enum PresetArg {
    /// Error-severity rules only: likely and certain bugs.
    Minimal,
    /// Every stable rule that is not opinionated. The default.
    #[default]
    Recommended,
    /// Adds naming, documentation, and other convention rules.
    Pedantic,
    /// Every registered rule, including experimental ones.
    All,
}

impl From<PresetArg> for RulePreset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::Minimal => Self::Minimal,
            PresetArg::Recommended => Self::Recommended,
            PresetArg::Pedantic => Self::Pedantic,
            PresetArg::All => Self::All,
        }
    }
}

/// The interoperability formats `inspect lint` can render its findings in.
///
/// A separate option from `--output` rather than more variants on it, because
/// `--output` also governs a dozen catalogue-only modes — `--explain`,
/// `--list-rules`, `--stats`, `--timings` — none of which produce a finding
/// list and none of which have anything to put in a JUnit testcase. `--emit`
/// applies only to a scan.
///
/// `sarif` and `github` are the same output the older `--sarif` and `--github`
/// flags produce; those stay, since scripts and the shipped GitHub Action pass
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(in crate::presentation::cli) enum EmitFormat {
    /// SARIF 2.1.0, with the full rule catalogue and the available auto-fixes.
    Sarif,
    /// GitHub Actions workflow commands, rendered inline on the pull request.
    Github,
    /// JUnit XML, for the test-report panel of a CI system.
    Junit,
    /// Code Climate issue JSON, which GitLab Code Quality consumes.
    CodeClimate,
    /// RFC 4180 comma-separated values, for a spreadsheet.
    Csv,
    /// Tab-separated values with a header row, for `cut`/`awk`.
    Tsv,
    /// A standalone HTML page, shareable without the tool.
    Html,
    /// A Markdown table, for a pull request comment or an issue.
    Markdown,
}

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct LintReportArgs {
    /// Files to scan. Not required with the catalogue-only modes
    /// (--list-rules, --list-presets, --list-tags, --explain, --docs).
    #[arg(required_unless_present_any = [
        "list_rules", "list_presets", "list_tags", "explain", "docs", "test_rules",
    ])]
    pub(in crate::presentation::cli::lint_report) files: Vec<PathBuf>,
    /// List every available lint rule with its description, then exit without scanning.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) list_rules: bool,
    /// With --list-rules, list only the rules that carry an auto-fix.
    ///
    /// The full catalogue reports fixability as a column, which answers
    /// "is this rule fixable" and not "what would a fix run do". A caller
    /// about to run the fixer is asking the second one; `paredit fix list` is
    /// this flag under a name that says so.
    #[arg(long, requires = "list_rules")]
    pub(in crate::presentation::cli::lint_report) fixable: bool,
    /// Print the long-form explanation of one rule — why it fires, a
    /// before/after example, what it deliberately leaves alone — then exit.
    #[arg(long, value_name = "RULE")]
    pub(in crate::presentation::cli::lint_report) explain: Option<String>,
    /// List the --preset rungs with what each admits, then exit.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) list_presets: bool,
    /// List the --tag values with the rules carrying each, then exit.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) list_tags: bool,
    /// Emit the whole rule reference as Markdown (one section per rule), then
    /// exit. Ignores --output; the payload is the document.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) docs: bool,
    /// Render findings in an interchange format instead of --output's
    /// (sarif, github, junit, code-climate, csv, tsv, html, markdown).
    #[arg(long, value_enum, value_name = "FORMAT", conflicts_with_all = [
        "list_rules", "list_presets", "list_tags", "explain", "docs", "sarif",
        "github", "fix", "fix_plan", "stats", "timings", "test_rules",
        "report_unused_suppressions", "write_baseline", "remove_unused_suppressions",
        "report_expired_suppressions", "report_suppressions",
    ])]
    pub(in crate::presentation::cli::lint_report) emit: Option<EmitFormat>,
    /// Emit findings as SARIF 2.1.0 for CI code scanning (ignores --output).
    /// Equivalent to `--emit sarif`.
    #[arg(long, conflicts_with = "list_rules")]
    pub(in crate::presentation::cli::lint_report) sarif: bool,
    /// Emit findings as GitHub Actions annotations (::error::) for inline PR
    /// review. Equivalent to `--emit github`.
    #[arg(long, conflicts_with_all = ["list_rules", "sarif"])]
    pub(in crate::presentation::cli::lint_report) github: bool,
    /// Apply available auto-fixes in place and report what changed (see
    /// --list-rules for which rules are fixable).
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fail_on_finding"])]
    pub(in crate::presentation::cli::lint_report) fix: bool,
    /// With --fix, print a unified diff of what would change and write nothing
    /// (a safe preview of the fixes).
    #[arg(long, requires = "fix")]
    pub(in crate::presentation::cli::lint_report) diff: bool,
    /// With --fix, write nothing and exit 3 if any auto-fix is still pending
    /// (a CI gate that fails when fixable lint has not been cleaned up).
    #[arg(long, requires = "fix")]
    pub(in crate::presentation::cli::lint_report) check: bool,
    /// Instead of applying fixes, emit the machine-readable fix plan: every
    /// fixable finding's exact byte-region replacements, without writing (for
    /// editors/agents to preview or apply one fix at a time).
    #[arg(long, conflicts_with_all = [
        "list_rules", "sarif", "github", "fix", "stats", "report_unused_suppressions",
        "write_baseline", "report_expired_suppressions", "report_suppressions",
    ])]
    pub(in crate::presentation::cli::lint_report) fix_plan: bool,
    /// Instead of listing findings, print a rollup of finding counts by
    /// severity, category, and rule (a lint-debt dashboard).
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix"])]
    pub(in crate::presentation::cli::lint_report) stats: bool,
    /// Instead of scanning, report inline `; paredit:ignore` directives that
    /// silence no finding (stale ignores or typo'd rule names); exit 3 if any.
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix", "stats"])]
    pub(in crate::presentation::cli::lint_report) report_unused_suppressions: bool,
    /// Instead of scanning, report inline suppression directives whose
    /// `-until <date>` has passed, whether or not they still silence
    /// anything; exit 3 if any are found. The write-once counterpart to
    /// `--report-unused-suppressions`: an expiry is a prompt to renew or
    /// delete a directive, not evidence it did nothing.
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix", "stats"])]
    pub(in crate::presentation::cli::lint_report) report_expired_suppressions: bool,
    /// Instead of scanning, list every inline suppression directive — used or
    /// not, with its scope, rules, reason, and expiry — for auditing how much
    /// of the file is silenced. One step past
    /// --report-unused-suppressions, which lists only the stale ones. Exits 0.
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix", "stats"])]
    pub(in crate::presentation::cli::lint_report) report_suppressions: bool,
    /// Suppress every lint finding under this path, as if the whole file
    /// carried `paredit:ignore-file` (repeatable). For generated code and
    /// vendored dependencies that cannot carry an inline suppression comment
    /// because they are regenerated; other commands still see these files.
    #[arg(long = "suppress-path", value_name = "PATH")]
    pub(in crate::presentation::cli::lint_report) suppress_paths: Vec<PathBuf>,
    /// Suppress findings recorded in this baseline file, so only new findings
    /// are reported/gated (works with the default, --sarif, and --github output).
    #[arg(long, value_name = "FILE", conflicts_with_all = ["list_rules", "fix"])]
    pub(in crate::presentation::cli::lint_report) baseline: Option<PathBuf>,
    /// Instead of scanning, write the current findings to this file as a
    /// baseline of known findings (for later use with --baseline); exit 0.
    #[arg(long, value_name = "FILE", conflicts_with_all = [
        "list_rules", "sarif", "github", "fix", "baseline", "report_unused_suppressions",
        "report_expired_suppressions", "report_suppressions",
    ])]
    pub(in crate::presentation::cli::lint_report) write_baseline: Option<PathBuf>,
    /// Reuse results for files whose content has not changed, cached here.
    ///
    /// Content-addressed: a hit means this exact question — same rules, same
    /// settings, same bytes, same build — was answered before. Safe to share
    /// between branches and to keep across upgrades; a new build simply cannot
    /// read the old entries.
    #[arg(long, value_name = "DIR", conflicts_with_all = ["list_rules", "fix"])]
    pub(in crate::presentation::cli::lint_report) cache_dir: Option<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) dialect: Option<DialectArg>,
    /// Run only these rules (repeatable). Mutually exclusive with --exclude.
    #[arg(long = "rule", value_name = "RULE")]
    pub(in crate::presentation::cli::lint_report) rules: Vec<String>,
    /// Run only rules in these categories (repeatable). Mutually exclusive with --rule.
    #[arg(long = "category", value_name = "CATEGORY", conflicts_with = "rules")]
    pub(in crate::presentation::cli::lint_report) categories: Vec<String>,
    /// Skip these rules (repeatable). Mutually exclusive with --rule.
    #[arg(long = "exclude", value_name = "RULE", conflicts_with = "rules")]
    pub(in crate::presentation::cli::lint_report) exclude: Vec<String>,
    /// Run only rules carrying every one of these tags (repeatable).
    /// See --list-tags for the values.
    #[arg(long = "tag", value_name = "TAG")]
    pub(in crate::presentation::cli::lint_report) tags: Vec<String>,
    /// How wide a net to cast. `recommended` (the default) is every stable,
    /// non-opinionated rule; see --list-presets.
    #[arg(long, value_enum, value_name = "PRESET", default_value_t = PresetArg::Recommended)]
    pub(in crate::presentation::cli::lint_report) preset: PresetArg,
    /// Also run the experimental rules, whichever preset is in force.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) experimental: bool,
    /// Report these rules (or every rule in these categories) at error
    /// severity, whatever they ship as (repeatable).
    #[arg(long = "deny", value_name = "RULE|CATEGORY")]
    pub(in crate::presentation::cli::lint_report) deny: Vec<String>,
    /// Report these rules (or every rule in these categories) at warning
    /// severity, whatever they ship as (repeatable). Use --exclude to silence
    /// a rule entirely.
    #[arg(long = "warn", value_name = "RULE|CATEGORY")]
    pub(in crate::presentation::cli::lint_report) warn: Vec<String>,
    /// Override one rule's threshold, as `<rule>.<key>=<value>` (repeatable).
    /// See `--explain <rule>` for the knobs a rule declares.
    #[arg(long = "rule-arg", value_name = "RULE.KEY=VALUE")]
    pub(in crate::presentation::cli::lint_report) rule_args: Vec<String>,
    /// Report how long each rule took and how often it ran, instead of the
    /// findings. Measurement is not free, so it is opt-in.
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix", "fix_plan"])]
    pub(in crate::presentation::cli::lint_report) timings: bool,
    /// With --fix, skip the fixes tagged `destructive` — the few whose rewrite
    /// can change runtime behaviour rather than only spelling.
    #[arg(long, requires = "fix")]
    pub(in crate::presentation::cli::lint_report) no_destructive_fixes: bool,
    /// Load the project's own pattern rules from this directory instead of
    /// the default `.paredit/rules`.
    #[arg(long, value_name = "DIR")]
    pub(in crate::presentation::cli::lint_report) custom_rules: Option<PathBuf>,
    /// Instead of scanning, run the `deftest` clauses in the custom rule files
    /// and exit 3 if any fail.
    #[arg(long, conflicts_with_all = ["sarif", "github", "fix", "fix_plan", "stats"])]
    pub(in crate::presentation::cli::lint_report) test_rules: bool,
    /// Treat a `paredit:ignore` directive with no `-- reason` as an unused
    /// suppression, so CI can require every silenced finding to say why.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) require_suppression_reason: bool,
    /// Delete the `paredit:ignore` directives that silence nothing, in place,
    /// and report what was removed. The write side of
    /// --report-unused-suppressions.
    #[arg(long, conflicts_with_all = [
        "list_rules", "sarif", "github", "fix", "fix_plan", "stats",
        "report_unused_suppressions", "write_baseline",
        "report_expired_suppressions", "report_suppressions",
    ])]
    pub(in crate::presentation::cli::lint_report) remove_unused_suppressions: bool,
    /// Exit with failure when any lint finding is reported.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) fail_on_finding: bool,
    /// Exit with failure only when a finding at or above this severity is
    /// reported (error = likely/certain bugs; warning = any finding).
    #[arg(long, value_enum, value_name = "SEVERITY")]
    pub(in crate::presentation::cli::lint_report) fail_on: Option<FailSeverity>,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli::lint_report) output: OutputFormat,
}

/// Which of `inspect lint`'s write-side modes the `fix` namespace is asking
/// for.
///
/// `fix` exists because the auto-fixer was reachable only as
/// `inspect lint --fix`, which is three wrong words for it: it is not an
/// inspection, its subject is not the lint report, and the capability is a
/// flag on a command with forty other flags. The mode is what the flag used to
/// be, moved into the command name where a reader can find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presentation::cli) enum FixMode {
    /// Apply the fixes.
    Apply,
    /// Apply nothing; exit 3 if any fix is still pending.
    Check,
    /// Emit the machine-readable fix plan without writing.
    Plan,
    /// List the fixable rules and exit.
    List,
}

/// The subset of lint's flags the `fix` namespace exposes.
///
/// A subset rather than a re-declaration of all forty. Everything omitted is
/// omitted for a reason a caller can state: `--baseline` and `--emit` shape a
/// *report*, and a fix run does not produce one; `--fail-on` gates on
/// findings, and `fix check` already gates on pending fixes; `--stats` and
/// `--timings` measure a scan.
#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct FixSelectionArgs {
    /// Files to fix. Not required by `fix list`.
    pub(in crate::presentation::cli) files: Vec<PathBuf>,
    /// Override extension-based dialect detection for every file.
    #[arg(long)]
    pub(in crate::presentation::cli) dialect: Option<DialectArg>,
    /// Fix only these rules (repeatable). Mutually exclusive with --exclude.
    #[arg(long = "rule", value_name = "RULE")]
    pub(in crate::presentation::cli) rules: Vec<String>,
    /// Fix only rules in these categories (repeatable).
    #[arg(long = "category", value_name = "CATEGORY", conflicts_with = "rules")]
    pub(in crate::presentation::cli) categories: Vec<String>,
    /// Skip these rules (repeatable). Mutually exclusive with --rule.
    #[arg(long = "exclude", value_name = "RULE", conflicts_with = "rules")]
    pub(in crate::presentation::cli) exclude: Vec<String>,
    /// Fix only rules carrying every one of these tags (repeatable).
    #[arg(long = "tag", value_name = "TAG")]
    pub(in crate::presentation::cli) tags: Vec<String>,
    /// How wide a net to cast. See `paredit inspect lint --list-presets`.
    #[arg(long, value_enum, value_name = "PRESET", default_value_t = PresetArg::Recommended)]
    pub(in crate::presentation::cli) preset: PresetArg,
    /// Also apply the experimental rules' fixes, whichever preset is in force.
    #[arg(long)]
    pub(in crate::presentation::cli) experimental: bool,
    /// Skip the fixes tagged `destructive` — the few whose rewrite can change
    /// runtime behaviour rather than only spelling.
    #[arg(long)]
    pub(in crate::presentation::cli) no_destructive_fixes: bool,
    /// Load the project's own pattern rules from this directory instead of
    /// the default `.paredit/rules`.
    #[arg(long, value_name = "DIR")]
    pub(in crate::presentation::cli) custom_rules: Option<PathBuf>,
    /// Print a unified diff of what would change and write nothing.
    #[arg(long)]
    pub(in crate::presentation::cli) diff: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub(in crate::presentation::cli) output: OutputFormat,
}

impl LintReportArgs {
    /// Builds the lint arguments a `fix` invocation means.
    ///
    /// Written as an explicit field-by-field literal rather than a `Default`
    /// plus overrides, and deliberately: a flag added to `LintReportArgs`
    /// stops this compiling until somebody decides whether `fix` should
    /// expose it. A `..Default::default()` would silently answer "no" to that
    /// question forever.
    pub(in crate::presentation::cli) fn for_fix(
        selection: FixSelectionArgs,
        mode: FixMode,
    ) -> Self {
        Self {
            files: selection.files,
            list_rules: mode == FixMode::List,
            fixable: mode == FixMode::List,
            explain: None,
            list_presets: false,
            list_tags: false,
            docs: false,
            emit: None,
            sarif: false,
            github: false,
            fix: matches!(mode, FixMode::Apply | FixMode::Check),
            diff: selection.diff && matches!(mode, FixMode::Apply | FixMode::Check),
            check: mode == FixMode::Check,
            fix_plan: mode == FixMode::Plan,
            stats: false,
            report_unused_suppressions: false,
            report_expired_suppressions: false,
            report_suppressions: false,
            suppress_paths: Vec::new(),
            baseline: None,
            write_baseline: None,
            cache_dir: None,
            dialect: selection.dialect,
            rules: selection.rules,
            categories: selection.categories,
            exclude: selection.exclude,
            tags: selection.tags,
            preset: selection.preset,
            experimental: selection.experimental,
            deny: Vec::new(),
            warn: Vec::new(),
            rule_args: Vec::new(),
            timings: false,
            no_destructive_fixes: selection.no_destructive_fixes,
            custom_rules: selection.custom_rules,
            test_rules: false,
            require_suppression_reason: false,
            remove_unused_suppressions: false,
            fail_on_finding: false,
            fail_on: None,
            output: selection.output,
        }
    }
}
