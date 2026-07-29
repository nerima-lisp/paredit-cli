use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::structural_diff::usecase::{Change, DiffPolicy};

/// The caveat printed with every run.
///
/// Stated rather than documented-elsewhere because the omission is exactly the
/// kind a reader will not notice: an empty structural diff of a change that
/// rewrote every comment in the file is a correct answer to the question this
/// command asks, and the wrong answer to the question a reviewer is asking.
const BLIND_SPOT: &str =
    "comments and whitespace are not compared; an empty diff does not mean the files are identical";

pub fn print_structural_diff(
    old: &str,
    new: &str,
    changes: &[Change],
    policy: &DiffPolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("old\t{}", safe_text!(old));
            println!("new\t{}", safe_text!(new));
            println!("change_count\t{}", changes.len());
            for (kind, count) in counts(changes) {
                println!("{kind}\t{count}");
            }
            if policy.fail_on_change {
                println!("policy\t--fail-on-change\tpassed={}", policy.passed);
            }
            println!("note\t{BLIND_SPOT}");
            for change in changes {
                println!(
                    "{}\t{}\tdepth={}\thead={}\tbefore={}\tafter={}",
                    change.kind.label(),
                    safe_text!(if change.path.is_empty() {
                        "<root>"
                    } else {
                        change.path.as_str()
                    }),
                    change.depth,
                    safe_text!(change.head().unwrap_or_default()),
                    safe_text!(one_line(
                        change.before.as_ref().map(|side| side.text.as_str())
                    )),
                    safe_text!(one_line(
                        change.after.as_ref().map(|side| side.text.as_str())
                    )),
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "inspect diff",
                "old": old,
                "new": new,
                "change_count": changes.len(),
                "by_kind": counts(changes)
                    .into_iter()
                    .map(|(kind, count)| json!({ "kind": kind, "count": count }))
                    .collect::<Vec<_>>(),
                "compares": "structure",
                "note": BLIND_SPOT,
                "policy": {
                    "fail_on_change": policy.fail_on_change,
                    "passed": policy.passed,
                },
                "changes": changes
                    .iter()
                    .map(|change| json!({
                        "kind": change.kind.label(),
                        "path": change.path,
                        "depth": change.depth,
                        "head": change.head(),
                        "before": change.before.as_ref().map(|side| json!({
                            "text": side.text,
                            "span": {
                                "start": side.span.start().get(),
                                "end": side.span.end().get(),
                            },
                        })),
                        "after": change.after.as_ref().map(|side| json!({
                            "text": side.text,
                            "span": {
                                "start": side.span.start().get(),
                                "end": side.span.end().get(),
                            },
                        })),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

/// Change counts by kind, in a fixed order so the header does not reshuffle
/// when a run happens to have no deletions.
fn counts(changes: &[Change]) -> Vec<(&'static str, usize)> {
    ["inserted", "deleted", "replaced"]
        .into_iter()
        .map(|kind| {
            (
                kind,
                changes
                    .iter()
                    .filter(|change| change.kind.label() == kind)
                    .count(),
            )
        })
        .collect()
}

/// Collapses a form onto one line for the tab-separated row.
///
/// The text output is one record per line by contract, and a multi-line form
/// would break that. JSON keeps the form verbatim.
fn one_line(text: Option<&str>) -> String {
    let Some(text) = text else {
        return String::new();
    };
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 80 {
        let head: String = collapsed.chars().take(77).collect();
        format!("{head}...")
    } else {
        collapsed
    }
}
