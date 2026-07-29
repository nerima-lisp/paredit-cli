//! Printing what the configuration is, and what is wrong with it.

use std::path::Path;

use anyhow::Result;
use serde_json::{Value as Json, json};

use paredit_core_cli::args::OutputFormat;
use paredit_core_config::error::{Diagnostic, Severity};
use paredit_core_config::load::{Loaded, Source};
use paredit_core_config::schema::{self, KeySchema, ValueKind};
use paredit_core_config::settings::Resolved;

use crate::presentation::cli::config_bridge::Injected;
use crate::presentation::cli::terminal_safe;

pub fn print_check(loaded: &Loaded, output: OutputFormat) -> Result<()> {
    let errors = count(loaded, Severity::Error);
    let warnings = count(loaded, Severity::Warning);

    match output {
        OutputFormat::Json => {
            let report = json!({
                "schema_version": 1,
                "report": "config check",
                "status": if errors == 0 { "ok" } else { "error" },
                "error_count": errors,
                "warning_count": warnings,
                "repository_root": display(loaded.repository_root.as_deref()),
                "source_count": loaded.sources.len(),
                "sources": loaded.sources.iter().map(source_json).collect::<Vec<_>>(),
                "diagnostics": loaded
                    .diagnostics
                    .iter()
                    .map(diagnostic_json)
                    .collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            println!("status\t{}", if errors == 0 { "ok" } else { "error" });
            println!("sources\t{}", loaded.sources.len());
            println!("errors\t{errors}");
            println!("warnings\t{warnings}");
            for source in &loaded.sources {
                println!(
                    "source\t{}\t{}\t{} keys",
                    source.layer.label(),
                    terminal_safe(source.path.display()),
                    source.entry_count
                );
            }
            for diagnostic in &loaded.diagnostics {
                println!("{}", terminal_safe(diagnostic));
            }
            // With nothing to read, "ok" alone reads as "your file is fine"
            // when there was no file. Say which happened.
            if loaded.sources.is_empty() {
                println!("note\tno configuration file was found; built-in defaults are in force");
            }
        }
    }
    Ok(())
}

pub fn print_show(
    loaded: &Loaded,
    key: Option<&str>,
    changed_only: bool,
    injections: Option<&[Injected]>,
    output: OutputFormat,
) -> Result<()> {
    let rows: Vec<(&'static KeySchema, Option<&Resolved>)> = loaded
        .settings
        .entries()
        .into_iter()
        .filter(|(entry, _)| key.is_none_or(|wanted| entry.key == wanted))
        .filter(|(entry, _)| !changed_only || loaded.settings.is_customised(entry.key))
        .collect();

    match output {
        OutputFormat::Json => {
            let report = json!({
                "schema_version": 1,
                "report": "config show",
                "repository_root": display(loaded.repository_root.as_deref()),
                "source_count": loaded.sources.len(),
                "sources": loaded.sources.iter().map(source_json).collect::<Vec<_>>(),
                "diagnostics": loaded
                    .diagnostics
                    .iter()
                    .map(diagnostic_json)
                    .collect::<Vec<_>>(),
                "settings": rows
                    .iter()
                    .map(|(entry, resolved)| setting_json(entry, *resolved))
                    .collect::<Vec<_>>(),
                "injections": injections.map(|injections| {
                    injections.iter().map(injection_json).collect::<Vec<_>>()
                }),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            for source in &loaded.sources {
                println!(
                    "source\t{}\t{}",
                    source.layer.label(),
                    terminal_safe(source.path.display())
                );
            }
            // key, value, layer, and where it came from: the four columns the
            // question "why is this set to that?" actually needs.
            for (entry, resolved) in &rows {
                match resolved {
                    Some(resolved) => println!(
                        "{}\t{}\t{}\t{}",
                        entry.key,
                        terminal_safe(&resolved.value),
                        resolved.origin.layer.label(),
                        terminal_safe(resolved.origin.describe())
                    ),
                    None => println!("{}\t<unset>\t-\t-", entry.key),
                }
            }
            for injection in injections.unwrap_or_default() {
                println!(
                    "injects\t{}\t--{}\t{}",
                    injection.key,
                    injection.flag,
                    if injection.values.is_empty() {
                        "(no value)".to_owned()
                    } else {
                        injection.values.join(" ")
                    }
                );
            }
            for diagnostic in &loaded.diagnostics {
                println!("{}", terminal_safe(diagnostic));
            }
        }
    }
    Ok(())
}

pub fn print_schema(output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            let report = json!({
                "schema_version": 1,
                "report": "config schema",
                "key_count": schema::KEY_COUNT,
                "file_names": paredit_core_config::load::CONFIG_FILE_NAMES,
                "loader_variables": paredit_core_config::load::LOADER_VARS
                    .iter()
                    .map(|(name, summary)| json!({ "name": name, "summary": summary }))
                    .collect::<Vec<_>>(),
                "layers": ["default", "user", "repository", "directory", "explicit", "environment", "flag"],
                "keys": schema::SCHEMA.iter().map(key_json).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            for entry in &schema::SCHEMA {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    entry.key,
                    entry.kind.label(),
                    entry.default.display().unwrap_or_else(|| "-".to_owned()),
                    entry.env_var(),
                    entry.summary
                );
            }
        }
    }
    Ok(())
}

pub fn print_init(path: &Path, contents: &str, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            let report = json!({
                "schema_version": 1,
                "report": "config init",
                "path": path.display().to_string(),
                "bytes": contents.len(),
                "key_count": schema::KEY_COUNT,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            println!(
                "wrote\t{}\t{} bytes",
                terminal_safe(path.display()),
                contents.len()
            );
        }
    }
    Ok(())
}

fn count(loaded: &Loaded, severity: Severity) -> usize {
    loaded
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == severity)
        .count()
}

fn display(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

fn source_json(source: &Source) -> Json {
    json!({
        "layer": source.layer.label(),
        "path": source.path.display().to_string(),
        "extended_by": display(source.extended_by.as_deref()),
        "entry_count": source.entry_count,
    })
}

fn diagnostic_json(diagnostic: &Diagnostic) -> Json {
    json!({
        "code": diagnostic.code.label(),
        "severity": diagnostic.severity().label(),
        "key": diagnostic.key,
        "path": display(diagnostic.path.as_deref()),
        "line": diagnostic.line,
        "origin": diagnostic.origin(),
        "message": diagnostic.message,
        "suggestion": diagnostic.suggestion,
    })
}

fn setting_json(entry: &KeySchema, resolved: Option<&Resolved>) -> Json {
    json!({
        "key": entry.key,
        "type": entry.kind.label(),
        "summary": entry.summary,
        "env": entry.env_var(),
        "default": entry.default.display(),
        "set": resolved.is_some(),
        "value": resolved.map(|resolved| resolved.value.to_json()),
        "value_display": resolved.map(|resolved| resolved.value.to_string()),
        "origin": resolved.map(|resolved| resolved.origin.to_json()),
    })
}

fn injection_json(injection: &Injected) -> Json {
    json!({
        "key": injection.key,
        "flag": format!("--{}", injection.flag),
        "values": injection.values,
    })
}

fn key_json(entry: &KeySchema) -> Json {
    let (minimum, maximum) = match entry.kind {
        ValueKind::Integer { min, max } => (Some(min), Some(max)),
        _ => (None, None),
    };
    json!({
        "key": entry.key,
        "type": entry.kind.label(),
        "choices": entry.kind.choices(),
        "min": minimum,
        "max": maximum,
        "default": entry.default.display(),
        "env": entry.env_var(),
        "path_relative": schema::PATH_KEYS.contains(&entry.key),
        "summary": entry.summary,
    })
}
