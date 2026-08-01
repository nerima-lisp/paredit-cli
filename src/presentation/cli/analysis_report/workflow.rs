use std::path::{Path, PathBuf};

use paredit_core_cli::{CliError, CliResult};
use serde_json::json;

use paredit_core_cli::report::budget::Budget;

use crate::presentation::cli::args::{AnalyzeArgs, OutputFormat};
use crate::presentation::cli::shared::{
    check_deadline_for_read, read_input_and_dialect, read_input_dialect_and_tree,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use super::args::{AgentReportArgs, CheckArgs};
use super::render::{
    AgentReportOptions, print_agent_report, print_dialect, print_outline, print_stats,
};

/// Where `defrule`'s own rules live by default.
///
/// Must keep matching `src/presentation/cli/lint_report/custom.rs`'s own
/// `DEFAULT_RULE_DIRECTORY`, which an ordinary lint run reads from; this is a
/// second, read-only look at the same directory, not a second source of
/// truth for where it is.
const DEFAULT_PAREDIT_RULE_DIRECTORY: &str = ".paredit/rules";

/// One `.paredit/rules/*.lisp` file's `--paredit-config` report.
struct PareditConfigFileReport {
    path: String,
    /// Parse failures, from the same dialect-aware path `check` itself uses.
    syntax_errors: Vec<String>,
    /// Everything else worth a project's attention: a `defrule` clause
    /// `parse_ruleset` refused, or one that parsed but still uses the
    /// pattern grammar's pre-unification spelling for a variable-length
    /// middle of a list.
    issues: Vec<String>,
}

/// Validates `.paredit/rules/*.lisp` — a directory ordinary workspace
/// discovery never sees, since `.paredit` is hidden and every discovery walk
/// skips dot-directories by default — and flags a `defrule` clause still
/// written in the pattern grammar's pre-unification style.
///
/// Never itself a reason to fail: this is a migration aid a project runs on
/// purpose, not a gate on every other command, so every finding here is
/// reported and none of them turns into an `Err`. A rule file that is
/// genuinely broken already fails the run that loads it by default (see
/// `custom::load`); what this adds is a way to see that *before* running a
/// command that loads rules, and a nudge for a clause that still works but
/// could be clearer.
fn check_paredit_config() -> Vec<PareditConfigFileReport> {
    let directory = Path::new(DEFAULT_PAREDIT_RULE_DIRECTORY);
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "lisp")
        })
        .collect();
    paths.sort();

    let mut reports = Vec::new();
    let mut merged = paredit_feature_lint_custom::Ruleset::default();
    for path in &paths {
        let display = path.display().to_string();
        let Ok(text) = std::fs::read_to_string(path) else {
            reports.push(PareditConfigFileReport {
                path: display,
                syntax_errors: vec!["could not be read".to_owned()],
                issues: Vec::new(),
            });
            continue;
        };

        // The dialect-aware path, not `SyntaxTree::parse`: a rule file is
        // read as Common Lisp everywhere else in this feature
        // (`parse_ruleset`), and using the generic parser here would miss
        // whatever the CLI path's dialect handling changes underneath it.
        let syntax_errors: Vec<String> = SyntaxTree::find_parse_errors(&text, Dialect::CommonLisp)
            .iter()
            .map(ToString::to_string)
            .collect();
        if !syntax_errors.is_empty() {
            reports.push(PareditConfigFileReport {
                path: display,
                syntax_errors,
                issues: Vec::new(),
            });
            continue;
        }

        match paredit_feature_lint_custom::parse_ruleset(&display, &text) {
            Err(error) => reports.push(PareditConfigFileReport {
                path: display,
                syntax_errors: Vec::new(),
                issues: vec![error.to_string()],
            }),
            Ok(ruleset) => {
                let issues = ruleset
                    .rules
                    .iter()
                    .filter(|rule| {
                        paredit_feature_lint_custom::pattern::uses_non_trailing_ellipsis(
                            &rule.pattern,
                        )
                    })
                    .map(|rule| {
                        format!(
                            "custom rule {:?}'s :pattern uses `...` before the end of \
                             a list; it still matches exactly what it always matched, \
                             but the clearer, capturing spelling for a variable-length \
                             middle is now `?name...`",
                            rule.name
                        )
                    })
                    .collect();
                merged.extend(ruleset);
                reports.push(PareditConfigFileReport {
                    path: display,
                    syntax_errors: Vec::new(),
                    issues,
                });
            }
        }
    }

    // Checked once across every file, the same way a real load does: a
    // collision or a dangling `deftest` is a property of the whole set, and a
    // per-file check would miss a `deftest` in one file naming a rule defined
    // in another.
    if let Err(error) = merged.validate(&crate::lint::report::RULES) {
        reports.push(PareditConfigFileReport {
            path: DEFAULT_PAREDIT_RULE_DIRECTORY.to_owned(),
            syntax_errors: Vec::new(),
            issues: vec![error.to_string()],
        });
    }

    reports
}

/// Validates that input is a balanced S-expression document.
///
/// Reports every syntax problem `SyntaxTree::find_parse_errors` finds, not
/// only the first — a file with three unrelated syntax errors used to cost
/// three round trips: fix one, re-run, hit the next. The singular `error`
/// field is unchanged, still the first problem found, so an existing
/// consumer reading only that field sees exactly what it always has; `errors`
/// is the new, additive way to see all of them at once.
pub(in crate::presentation::cli) fn check(args: CheckArgs) -> CliResult<()> {
    let CheckArgs {
        analyze: args,
        paredit_config,
    } = args;
    check_deadline_for_read(args.file.as_deref())?;
    let (input, dialect) = read_input_and_dialect(args.file, args.dialect)?;
    let mut errors = SyntaxTree::find_parse_errors(&input.text, dialect);

    // Computed either way `--output` is asked for, but never a reason this
    // function returns `Err`: see `check_paredit_config`'s own documentation
    // for why it is advice rather than a gate.
    let paredit_config_reports = if paredit_config {
        check_paredit_config()
    } else {
        Vec::new()
    };

    match args.output {
        OutputFormat::Text => {
            for report in &paredit_config_reports {
                for error in &report.syntax_errors {
                    println!("paredit-config: {}: error: {error}", report.path);
                }
                for issue in &report.issues {
                    println!("paredit-config: {}: {issue}", report.path);
                }
            }
            if errors.is_empty() {
                println!("ok");
                return Ok(());
            }
            for error in &errors {
                println!("error: {error}");
            }
            Err(CliError::Parse {
                origin: "input".to_owned(),
                location: "input is not a balanced S-expression document".to_owned(),
                source: errors.remove(0),
            })
        }
        OutputFormat::Json => {
            let file = input.file.as_deref().map(|path| path.display().to_string());
            let errors_json: Vec<_> = errors
                .iter()
                .map(|error| {
                    json!({
                        "message": error.to_string(),
                        "offset": error.position(),
                    })
                })
                .collect();
            let paredit_config_json = paredit_config.then(|| {
                json!({
                    "directory": DEFAULT_PAREDIT_RULE_DIRECTORY,
                    "files": paredit_config_reports.iter().map(|report| json!({
                        "path": report.path,
                        "syntax_errors": report.syntax_errors,
                        "issues": report.issues,
                    })).collect::<Vec<_>>(),
                })
            });
            let report = json!({
                "schema_version": 1,
                "status": if errors.is_empty() { "ok" } else { "error" },
                "file": file,
                "dialect": dialect.label(),
                "error": errors.first().map(ToString::to_string),
                "errors": errors_json,
                "paredit_config": paredit_config_json,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            if errors.is_empty() {
                Ok(())
            } else {
                Err(CliError::Parse {
                    origin: "input".to_owned(),
                    location: "input is not a balanced S-expression document".to_owned(),
                    source: errors.remove(0),
                })
            }
        }
    }
}

pub(in crate::presentation::cli) fn dialect(args: AnalyzeArgs) -> CliResult<()> {
    let (_, dialect, _) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_dialect(dialect, args.output)
}

pub(in crate::presentation::cli) fn stats(args: AnalyzeArgs) -> CliResult<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_stats(&tree, dialect, args.output)
}

pub(in crate::presentation::cli) fn agent_report(args: AgentReportArgs) -> CliResult<()> {
    let runtime = paredit_core_cli::runtime::current();
    let analyze = args.analyze;

    // A flag beats the configuration, which beats the built-in default. The
    // same order every other setting follows.
    let verbosity = args.verbosity.map_or(runtime.verbosity, Into::into);
    let budget = Budget(args.max_tokens.unwrap_or(runtime.max_tokens));

    let previous = args
        .since
        .as_deref()
        .map(read_previous_report)
        .transpose()?;

    let (input, dialect, tree) = read_input_dialect_and_tree(analyze.file, analyze.dialect)?;
    print_agent_report(
        &tree,
        dialect,
        analyze.output,
        &AgentReportOptions {
            verbosity,
            budget,
            previous: previous.as_ref(),
            file: input.file.as_deref(),
        },
    )
}

/// Reads a previous `agent-report --output json`.
///
/// Refused rather than ignored when it is not one: comparing against a file
/// that happens to be JSON but is not this report would produce a delta that
/// says everything changed, which is indistinguishable from a real answer.
fn read_previous_report(path: &Path) -> CliResult<serde_json::Value> {
    let text = std::fs::read_to_string(path).map_err(CliError::io(format!(
        "failed to read --since {}",
        path.display()
    )))?;
    let report: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| CliError::Json {
            context: format!("--since {} is not JSON", path.display()),
            source,
        })?;

    if report.get("outline").is_none() || report.get("metrics").is_none() {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
            format!(
                "--since {} is not an `inspect agent-report --output json` report \
                 (no `outline` or `metrics`)",
                path.display()
            ),
        )
        .into());
    }
    Ok(report)
}

pub(in crate::presentation::cli) fn outline(args: AnalyzeArgs) -> CliResult<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    print_outline(&tree, dialect, args.output)
}
