//! `paredit inspect change --before <FILE> --after <FILE>`.

use std::path::PathBuf;

use clap::Args;
use paredit_core_cli::CommandResult;
use serde_json::{Value, json};

use paredit_core_cli::args::{DialectArg, OutputFormat};
use paredit_core_cli::gate::gate_failure;
use paredit_core_cli::shared::{read_input_and_dialect, terminal_safe};

use super::domain::{Change, Definition, summarise};
use super::prose;

#[derive(Debug, Args)]
pub struct ChangeSummaryArgs {
    /// The earlier version.
    #[arg(long, value_name = "FILE")]
    pub before: PathBuf,
    /// The later version. Defaults to --before's own path, which is the shape
    /// of "compare my working copy against a snapshot I took".
    #[arg(long, value_name = "FILE")]
    pub after: Option<PathBuf>,
    /// Override extension-based dialect detection for both versions.
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    /// Exit with failure when any definition changed. Formatting-only edits
    /// do not trip it, which is the point: it gates on substance.
    #[arg(long)]
    pub fail_on_change: bool,
    /// Output format for agent consumption. `text` is the pull request draft.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output: OutputFormat,
}

pub fn change_summary(args: ChangeSummaryArgs) -> CommandResult {
    let after_path = args.after.clone().unwrap_or_else(|| args.before.clone());

    let (before_input, dialect) = read_input_and_dialect(Some(args.before.clone()), args.dialect)?;
    let (after_input, _) = read_input_and_dialect(Some(after_path.clone()), args.dialect)?;

    let summary = summarise(&before_input.text, &after_input.text, dialect).ok_or_else(|| {
        crate::error::DocumentsNotComparable {
            before: args.before.display().to_string(),
            after: after_path.display().to_string(),
        }
    })?;

    match args.output {
        // Not escaped here: `prose` escapes each interpolated name, and
        // escaping the finished draft would turn its own line breaks into
        // `\u{a}` and leave a pull request description on one line.
        OutputFormat::Text => print!("{}", prose::draft(&summary)),
        OutputFormat::Json => {
            let report = json!({
                "schema_version": 1,
                "report": "change",
                "before": args.before.display().to_string(),
                "after": after_path.display().to_string(),
                "dialect": dialect.label(),
                "identical": summary.identical,
                "formatting_only": summary.formatting_only,
                "definition_count": {
                    "before": summary.before_definitions,
                    "after": summary.after_definitions,
                },
                "counts": {
                    "added": summary.count("added"),
                    "removed": summary.count("removed"),
                    "renamed": summary.count("renamed"),
                    "modified": summary.count("modified"),
                    "moved": summary.count("moved"),
                },
                // Both the sentence and the facts it was rendered from, so a
                // caller can paste one or compute with the other.
                "headline": prose::headline(&summary),
                "draft": prose::draft(&summary),
                "changes": summary.changes.iter().map(change_json).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    if args.fail_on_change && !summary.changes.is_empty() {
        return Err(gate_failure(format!(
            "--fail-on-change: {} definition change(s)",
            summary.changes.len()
        )));
    }
    Ok(())
}

fn change_json(change: &Change) -> Value {
    let (before, after) = match change {
        Change::Added(definition) => (None, Some(definition)),
        Change::Removed(definition) => (Some(definition), None),
        Change::Modified { before, after }
        | Change::Renamed { before, after }
        | Change::Moved { before, after } => (Some(before), Some(after)),
    };

    json!({
        "kind": change.kind(),
        "label": terminal_safe(change.anchor().label()).to_string(),
        "sentence": prose::describe(change),
        "before": before.map(definition_json),
        "after": after.map(definition_json),
    })
}

fn definition_json(definition: &Definition) -> Value {
    json!({
        "head": definition.head,
        "name": definition.name,
        "path": definition.path,
        "line": definition.line,
    })
}

#[cfg(test)]
mod tests {
    use super::super::domain::ChangeSummary;
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn summary(before: &str, after: &str) -> ChangeSummary {
        summarise(before, after, Dialect::CommonLisp).expect("parses")
    }

    #[test]
    fn a_change_carries_both_the_sentence_and_the_facts() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) (list x))\n");
        let json = change_json(&summary.changes[0]);

        assert_eq!(json["kind"], "modified");
        assert_eq!(json["before"]["name"], "f");
        assert_eq!(json["after"]["line"], 1);
        assert!(
            json["sentence"]
                .as_str()
                .expect("sentence")
                .starts_with("Changed the body")
        );
    }

    #[test]
    fn an_addition_has_no_before_side() {
        let summary = summary("(defun f (x) x)\n", "(defun f (x) x)\n(defun g (y) y)\n");
        let json = change_json(&summary.changes[0]);
        assert!(json["before"].is_null());
        assert_eq!(json["after"]["name"], "g");
    }

    /// A definition name comes from source and reaches a terminal.
    #[test]
    fn a_hostile_definition_name_is_escaped() {
        let summary = summary("(defun a (x) x)\n", "(defun a\u{202e}b (x) x)\n");
        let json = change_json(&summary.changes[0]);
        let label = json["label"].as_str().expect("label");
        assert!(!label.contains('\u{1b}'), "{label}");
    }
}
