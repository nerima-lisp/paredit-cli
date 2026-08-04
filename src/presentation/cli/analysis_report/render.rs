use std::collections::BTreeMap;

use paredit_core_cli::CliResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use serde_json::json;

use paredit_core_cli::report::budget::Budget;
use paredit_core_cli::report::next_command::{self, NextCommand};
use paredit_core_cli::runtime::Verbosity;
use paredit_core_cli::shared::stable_text_hash;

use crate::presentation::cli::args::OutputFormat;

pub(super) fn print_dialect(dialect: Dialect, output: OutputFormat) -> CliResult<()> {
    match output {
        OutputFormat::Text => println!("{dialect}"),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "family": dialect.family(),
            }))?
        ),
    }
    Ok(())
}

pub(super) fn print_stats(
    tree: &SyntaxTree,
    dialect: Dialect,
    output: OutputFormat,
) -> CliResult<()> {
    let atoms = tree.atom_occurrences();
    let outline = tree.outline(|head| dialect.is_definition_head(head));
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", dialect.label());
            println!("top_level_forms\t{}", tree.root_children().len());
            println!("outline_entries\t{}", outline.len());
            println!("atom_occurrences\t{}", atoms.len());
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": dialect.label(),
                "topLevelForms": tree.root_children().len(),
                "outlineEntries": outline.len(),
                "atomOccurrences": atoms.len(),
            }))?
        ),
    }
    Ok(())
}

/// Everything the agent report needs beyond the tree itself.
#[derive(Debug)]
pub(super) struct AgentReportOptions<'a> {
    pub verbosity: Verbosity,
    pub budget: Budget,
    /// The previous report to compare against, already read and parsed.
    pub previous: Option<&'a serde_json::Value>,
    /// The file this was read from, for the follow-up commands.
    pub file: Option<&'a std::path::Path>,
}

pub(super) fn print_agent_report(
    tree: &SyntaxTree,
    dialect: Dialect,
    output: OutputFormat,
    options: &AgentReportOptions<'_>,
) -> CliResult<()> {
    let atoms = tree.atom_occurrences();
    let outline = tree.outline(|head| dialect.is_definition_head(head));
    let definition_count = outline.iter().filter(|entry| entry.definition_like).count();

    // `quiet` drops the two lists and keeps every count, which is the
    // distinction that makes it useful: you still learn the shape of the file
    // and can decide whether the detail is worth asking for.
    let include_lists = options.verbosity != Verbosity::Quiet;

    let outline_json = outline
        .iter()
        .map(|entry| {
            json!({
                "path": entry.path.to_string(),
                "span": {
                    "start": entry.span.start().get(),
                    "end": entry.span.end().get(),
                },
                "head": entry.head,
                // The name a definition binds — `f` in `(defun f ...)`. Added
                // because `head` alone is `defun` for every function in the
                // file, which cannot identify one across an edit.
                "name": definition_name(tree, &entry.path),
                "definitionLike": entry.definition_like,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "schema_version": 1,
        "verbosity": options.verbosity.label(),
        "dialect": {
            "label": dialect.label(),
            "family": dialect.family(),
        },
        "metrics": {
            "topLevelForms": tree.root_children().len(),
            "outlineEntries": outline.len(),
            "definitionLikeForms": definition_count,
            "atomOccurrences": atoms.len(),
        },
        "outline": if include_lists { outline_json.clone() } else { Vec::new() },
        "atoms": if include_lists {
            atoms
                .iter()
                .map(|occurrence| {
                    json!({
                        "path": occurrence.path.to_string(),
                        "span": {
                            "start": occurrence.span.start().get(),
                            "end": occurrence.span.end().get(),
                        },
                        "text": occurrence.text,
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        },
    });

    if options.verbosity == Verbosity::Detailed {
        // The digest is what makes `--since` cheap on the next run: an
        // unchanged digest settles the question without comparing anything.
        payload["digest"] = json!(stable_text_hash(tree.source()));
        payload["distinctAtoms"] = json!(distinct_atom_count(&atoms));
    }

    if let Some(previous) = options.previous {
        payload["delta"] = delta_json(previous, &outline_json, atoms.len());
    }

    if let Some(commands) = next_command::to_json(&follow_up_commands(
        dialect,
        options.file,
        definition_count,
        atoms.len(),
    )) {
        payload["next_commands"] = commands;
    }

    // Applied last, so the budget accounts for everything the report carries.
    // Atoms go first: they are the least information-dense thing here, and a
    // caller that needs one can ask `inspect find-symbol` for it directly.
    if let Some(truncation) = options.budget.apply(&mut payload, &["atoms", "outline"]) {
        payload["truncation"] = truncation.to_json();
    }

    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", dialect.label());
            println!("top_level_forms\t{}", tree.root_children().len());
            println!("definition_like_forms\t{definition_count}");
            println!("atom_occurrences\t{}", atoms.len());
            println!("use --output json for full outline and atom spans");
        }
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&payload)?),
    }
    Ok(())
}

fn distinct_atom_count(atoms: &[paredit_core_syntax::sexpr::AtomOccurrence]) -> usize {
    atoms
        .iter()
        .map(|occurrence| occurrence.text.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

/// The name a top-level form binds: `f` in `(defun f (x) ...)`.
///
/// The second child, which is where every `def*` form in every dialect this
/// tool parses puts it. `None` for a form that has no second atom — a bare
/// list, or one whose name is itself a list, as in `(defun (setf foo) ...)`.
fn definition_name(tree: &SyntaxTree, path: &paredit_core_syntax::sexpr::Path) -> Option<String> {
    let selection = tree.select_path(path).ok()?;
    let view = selection.view();
    view.children
        .get(1)
        .filter(|child| child.kind == paredit_core_syntax::sexpr::ExpressionKind::Atom)
        .and_then(|child| child.text.clone())
}

/// What changed since a previous report.
///
/// Keyed on `head` plus `name` — `defun f` — rather than on the path. A path
/// shifts whenever anything above it grows, so a path-keyed delta reports the
/// whole rest of the file as changed every time a definition is inserted,
/// which is the same as reporting nothing. Keyed on identity, an insertion
/// reports one addition, and a definition whose path shifted appears under
/// `moved` — which is what a caller holding stored `--path` selectors needs.
fn delta_json(
    previous: &serde_json::Value,
    outline: &[serde_json::Value],
    atom_count: usize,
) -> serde_json::Value {
    let previous_outline = previous["outline"].as_array().cloned().unwrap_or_default();

    let identity = |entry: &serde_json::Value| {
        format!(
            "{} {}",
            entry["head"].as_str().unwrap_or("<unknown>"),
            entry["name"].as_str().unwrap_or("<anonymous>"),
        )
    };
    let path_of = |entry: &serde_json::Value| entry["path"].as_str().unwrap_or_default().to_owned();

    let before: BTreeMap<String, String> = previous_outline
        .iter()
        .map(|entry| (identity(entry), path_of(entry)))
        .collect();
    let after: BTreeMap<String, String> = outline
        .iter()
        .map(|entry| (identity(entry), path_of(entry)))
        .collect();

    let added: Vec<&serde_json::Value> = outline
        .iter()
        .filter(|entry| !before.contains_key(&identity(entry)))
        .collect();
    let removed: Vec<&serde_json::Value> = previous_outline
        .iter()
        .filter(|entry| !after.contains_key(&identity(entry)))
        .collect();
    let moved: Vec<serde_json::Value> = after
        .iter()
        .filter_map(|(name, path)| {
            let was = before.get(name)?;
            (was != path).then(|| json!({ "name": name, "from": was, "to": path }))
        })
        .collect();

    // A previous report written at `--verbosity quiet` has an empty outline
    // and a full metrics block, so the delta would claim every definition was
    // removed. Saying the comparison is not meaningful beats saying that.
    let comparable = previous_outline.len()
        == previous["metrics"]["outlineEntries"]
            .as_u64()
            .map_or(previous_outline.len(), |count| {
                usize::try_from(count).unwrap_or(usize::MAX)
            });

    json!({
        "compared_against": previous["digest"],
        "comparable": comparable,
        "unchanged": comparable && added.is_empty() && removed.is_empty() && moved.is_empty(),
        "outline": {
            "added": added,
            "removed": removed,
            "moved": moved,
        },
        "atom_occurrences": {
            "previous": previous["metrics"]["atomOccurrences"].as_u64(),
            "current": atom_count,
        },
    })
}

/// The commands this report's own contents justify.
///
/// Nothing generic: a suggestion that would be emitted whatever the report
/// found is an advertisement, and an agent that learns to ignore one learns to
/// ignore all of them.
fn follow_up_commands(
    dialect: Dialect,
    file: Option<&std::path::Path>,
    definition_count: usize,
    atom_count: usize,
) -> Vec<NextCommand> {
    let mut commands = Vec::new();
    let target = file.map(|path| next_command::shell_path(&path.display().to_string()));

    if dialect == Dialect::Unknown {
        commands.push(NextCommand::new(
            "paredit config schema",
            "the dialect could not be determined from the extension; \
             [dialect] in paredit.toml can settle it for every command",
        ));
    }

    let Some(target) = target else {
        // Reading from stdin, so there is no path to put in a command line.
        // A suggestion the caller has to rewrite is not a suggestion.
        return commands;
    };

    if definition_count > 0 {
        commands.push(NextCommand::new(
            format!("paredit inspect lint --output json {target}"),
            format!(
                "{definition_count} definition-like {} present; lint reports logic bugs in {}",
                if definition_count == 1 {
                    "form is"
                } else {
                    "forms are"
                },
                if definition_count == 1 { "it" } else { "them" },
            ),
        ));
        commands.push(NextCommand::new(
            format!("paredit inspect complexity --output json {target}"),
            "ranks those definitions by nesting depth and size, for refactor triage",
        ));
    } else if atom_count > 0 {
        commands.push(NextCommand::new(
            format!("paredit inspect outline --output json --file {target}"),
            "no definition-like forms were recognised; the outline shows what is actually there",
        ));
    }

    commands
}

pub(super) fn print_outline(
    tree: &SyntaxTree,
    dialect: Dialect,
    output: OutputFormat,
) -> CliResult<()> {
    let entries = tree.outline(|head| dialect.is_definition_head(head));
    match output {
        OutputFormat::Text => {
            for entry in entries {
                println!(
                    "{}\t{}..{}\t{}\t{}",
                    safe_text!(entry.path),
                    entry.span.start().get(),
                    entry.span.end().get(),
                    entry.head.as_deref().unwrap_or("<unknown>"),
                    entry.definition_like
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(
                &entries
                    .into_iter()
                    .map(|entry| {
                        json!({
                            "path": entry.path.to_string(),
                            "span": {
                                "start": entry.span.start().get(),
                                "end": entry.span.end().get(),
                            },
                            "head": entry.head,
                            "definitionLike": entry.definition_like,
                        })
                    })
                    .collect::<Vec<_>>()
            )?
        ),
    }
    Ok(())
}
