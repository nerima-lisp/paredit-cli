use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::application::usecase::lint_report::{RulePreset, Severity};
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

#[derive(Debug, Args)]
pub(in crate::presentation::cli) struct LintReportArgs {
    /// Files to scan. Not required with the catalogue-only modes
    /// (--list-rules, --list-presets, --list-tags, --explain, --docs).
    #[arg(required_unless_present_any = [
        "list_rules", "list_presets", "list_tags", "explain", "docs",
    ])]
    pub(in crate::presentation::cli::lint_report) files: Vec<PathBuf>,
    /// List every available lint rule with its description, then exit without scanning.
    #[arg(long)]
    pub(in crate::presentation::cli::lint_report) list_rules: bool,
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
    /// Emit findings as SARIF 2.1.0 for CI code scanning (ignores --output).
    #[arg(long, conflicts_with = "list_rules")]
    pub(in crate::presentation::cli::lint_report) sarif: bool,
    /// Emit findings as GitHub Actions annotations (::error::) for inline PR review.
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
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix", "stats", "report_unused_suppressions", "write_baseline"])]
    pub(in crate::presentation::cli::lint_report) fix_plan: bool,
    /// Instead of listing findings, print a rollup of finding counts by
    /// severity, category, and rule (a lint-debt dashboard).
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix"])]
    pub(in crate::presentation::cli::lint_report) stats: bool,
    /// Instead of scanning, report inline `; paredit:ignore` directives that
    /// silence no finding (stale ignores or typo'd rule names); exit 3 if any.
    #[arg(long, conflicts_with_all = ["list_rules", "sarif", "github", "fix", "stats"])]
    pub(in crate::presentation::cli::lint_report) report_unused_suppressions: bool,
    /// Suppress findings recorded in this baseline file, so only new findings
    /// are reported/gated (works with the default, --sarif, and --github output).
    #[arg(long, value_name = "FILE", conflicts_with_all = ["list_rules", "fix"])]
    pub(in crate::presentation::cli::lint_report) baseline: Option<PathBuf>,
    /// Instead of scanning, write the current findings to this file as a
    /// baseline of known findings (for later use with --baseline); exit 0.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["list_rules", "sarif", "github", "fix", "baseline", "report_unused_suppressions"])]
    pub(in crate::presentation::cli::lint_report) write_baseline: Option<PathBuf>,
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
