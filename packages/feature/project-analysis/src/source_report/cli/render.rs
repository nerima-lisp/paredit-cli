use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_cli::workspace_args::ResolvedWorkspaceInput;
use paredit_core_workspace::workspace::{CacheOutcome, ManifestSource, skip_counter_names};

use super::args::SourceReportArgs;
use super::workflow::SourceScan;

/// The shape both a fresh scan and a cache hit reduce to.
///
/// Rendering against this rather than against `WorkspaceDiscovery` is what lets
/// a cache entry be a plain file list: the report never needs the directory
/// capabilities a live discovery carries, because it never reads a file.
struct ScanView<'a> {
    files: &'a [PathBuf],
    repositories: BTreeMap<&'a Path, usize>,
    files_outside_repositories: usize,
    ignore_files_read: &'a [PathBuf],
    skipped: [usize; 9],
    visited_entry_count: usize,
}

impl SourceScan {
    fn view(&self) -> ScanView<'_> {
        match self {
            Self::Fresh(discovery) => ScanView {
                files: discovery.files(),
                repositories: discovery
                    .repositories()
                    .iter()
                    .map(|(repository, files)| (repository.as_path(), files.len()))
                    .collect(),
                files_outside_repositories: discovery.files_outside_repositories().len(),
                ignore_files_read: discovery.ignore_files_read(),
                skipped: discovery.skip_counters(),
                visited_entry_count: discovery.visited_entry_count(),
            },
            Self::Cached(cached) => ScanView {
                files: &cached.files,
                repositories: cached
                    .repositories
                    .iter()
                    .map(|(repository, files)| (repository.as_path(), files.len()))
                    .collect(),
                files_outside_repositories: cached.files_outside_repositories.len(),
                ignore_files_read: &cached.ignore_files_read,
                skipped: cached.skipped,
                visited_entry_count: cached.visited_entry_count,
            },
        }
    }
}

pub fn print_source_report(
    args: &SourceReportArgs,
    resolved: &ResolvedWorkspaceInput,
    scan: &SourceScan,
    cache_outcome: Option<CacheOutcome>,
    output: OutputFormat,
) -> Result<()> {
    let view = scan.view();
    match output {
        OutputFormat::Text => print_text(args, resolved, &view, cache_outcome),
        OutputFormat::Json => print_json(args, resolved, &view, cache_outcome)?,
    }
    Ok(())
}

fn print_text(
    args: &SourceReportArgs,
    resolved: &ResolvedWorkspaceInput,
    view: &ScanView<'_>,
    cache_outcome: Option<CacheOutcome>,
) {
    println!("selector\t{}", resolved.selector.as_str());
    println!("selected\t{}", resolved.selected_path_count);
    println!("files\t{}", view.files.len());
    println!("entries_visited\t{}", view.visited_entry_count);
    if let Some(outcome) = cache_outcome {
        println!("cache\t{}", outcome.as_str());
    }
    for (name, count) in skip_counter_names().into_iter().zip(view.skipped) {
        println!("skipped_{name}\t{count}");
    }
    for ignore_file in view.ignore_files_read {
        println!("ignore_file\t{}", safe_text!(ignore_file.display()));
    }
    for (repository, count) in &view.repositories {
        println!("repository\t{}\t{count}", safe_text!(repository.display()));
    }
    if view.files_outside_repositories > 0 {
        println!("outside_repositories\t{}", view.files_outside_repositories);
    }
    for manifest in &resolved.manifests {
        println!(
            "manifest\t{}\t{}\tfiles={}\tsource_paths={}\tdependencies={}\tmissing={}",
            manifest.kind.as_str(),
            safe_text!(manifest.path.display()),
            manifest.files.len(),
            manifest.source_paths.len(),
            manifest.dependencies.len(),
            manifest.missing.len()
        );
    }
    if args.list_files {
        for file in view.files {
            println!("file\t{}", safe_text!(file.display()));
        }
    }
}

fn print_json(
    args: &SourceReportArgs,
    resolved: &ResolvedWorkspaceInput,
    view: &ScanView<'_>,
    cache_outcome: Option<CacheOutcome>,
) -> Result<()> {
    let mut skipped = serde_json::Map::new();
    for (name, count) in skip_counter_names().into_iter().zip(view.skipped) {
        skipped.insert(name.to_owned(), json!(count));
    }

    let mut report = json!({
        "schema_version": 1,
        "roots": args.roots.iter().map(|path| display(path)).collect::<Vec<_>>(),
        "selector": resolved.selector.as_str(),
        "selected_path_count": resolved.selected_path_count,
        "file_count": view.files.len(),
        "entries_visited": view.visited_entry_count,
        "cache": cache_outcome.map(CacheOutcome::as_str),
        "ignore": {
            "gitignore": resolved.options.ignore.respect_gitignore,
            "pareditignore": resolved.options.ignore.respect_pareditignore,
            "files_read": view.ignore_files_read.iter().map(|path| display(path)).collect::<Vec<_>>(),
        },
        "symlinks": resolved.options.symlinks.as_str(),
        "skipped": Value::Object(skipped),
        "repositories": view
            .repositories
            .iter()
            .map(|(repository, count)| json!({
                "path": repository.display().to_string(),
                "file_count": count,
            }))
            .collect::<Vec<_>>(),
        "files_outside_repositories": view.files_outside_repositories,
        "manifests": resolved.manifests.iter().map(manifest_report).collect::<Vec<_>>(),
    });
    if args.list_files {
        report["files"] = Value::Array(
            view.files
                .iter()
                .map(|file| Value::String(file.display().to_string()))
                .collect(),
        );
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn manifest_report(manifest: &ManifestSource) -> Value {
    json!({
        "kind": manifest.kind.as_str(),
        "path": manifest.path.display().to_string(),
        "name": manifest.name.as_deref(),
        "files": manifest.files.iter().map(|path| display(path)).collect::<Vec<_>>(),
        "source_paths": manifest
            .source_paths
            .iter()
            .map(|entry| json!({
                "path": entry.path.display().to_string(),
                "role": entry.role.as_str(),
            }))
            .collect::<Vec<_>>(),
        "dependencies": manifest
            .dependencies
            .iter()
            .map(|dependency| json!({
                "name": dependency.name,
                "version": dependency.version.as_deref(),
            }))
            .collect::<Vec<_>>(),
        "missing": manifest.missing.iter().map(|path| display(path)).collect::<Vec<_>>(),
    })
}

fn display(path: &Path) -> String {
    path.display().to_string()
}
