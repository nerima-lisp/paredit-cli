//! `inspect sources` — what this run would analyse, and why.
//!
//! Every other workspace command answers a question *about* the files it
//! selected. None of them answers "which files did you select, and which rule
//! dropped the rest" — and with `.gitignore`, `.pareditignore`, `--include`,
//! `--exclude-glob`, `--since`, `--from-git`, `--from-manifest` and
//! `--paths-from` all able to change the answer, that question stopped being
//! trivial.
//!
//! It exists mostly for the two moments when input selection is the bug: a CI
//! run that reports nothing because a pattern was wider than intended, and an
//! agent that needs the file list before deciding what to run over it. Both are
//! answered without parsing a single form, so this is also the cheapest command
//! in the tool.
//!
//! It is also where `--cache-dir` lives, and deliberately only here. A cached
//! entry is a *file list*, not the capability-scoped directory handles the
//! reading commands need, so restoring one for a command that then reads those
//! files would mean rebuilding the very checks that make reads safe. Piping
//! instead composes without weakening anything:
//!
//! ```sh
//! paredit inspect sources --cache-dir ~/.cache/paredit --list-files --output text . \
//!   | sed -n 's/^file\t//p' \
//!   | paredit inspect lint --paths-from - .
//! ```

use anyhow::Result;

use paredit_core_cli::workspace_args::ResolvedWorkspaceInput;
use paredit_core_workspace::workspace::{
    CacheOutcome, CachedDiscovery, DiscoveryCache, WorkspaceDiscovery, discover_workspace_files,
    discover_workspace_files_from_list,
};

use super::args::SourceReportArgs;
use super::render::print_source_report;

/// A scan result, however it was obtained.
#[derive(Debug)]
pub enum SourceScan {
    /// Produced by walking or by resolving a file list.
    Fresh(Box<WorkspaceDiscovery>),
    /// Restored from a cache entry that was still valid.
    Cached(Box<CachedDiscovery>),
}

pub fn source_report(args: SourceReportArgs) -> Result<()> {
    let resolved = args.input.resolve(&args.roots)?;
    let cache = args.cache_dir.clone().map(DiscoveryCache::new);
    if let Some(cache) = cache.as_ref() {
        if args.clear_cache {
            cache.clear()?;
        }
    }

    let (scan, outcome) = scan(&resolved, cache.as_ref())?;
    print_source_report(&args, &resolved, &scan, outcome, args.output)
}

fn scan(
    resolved: &ResolvedWorkspaceInput,
    cache: Option<&DiscoveryCache>,
) -> Result<(SourceScan, Option<CacheOutcome>)> {
    if let Some(cache) = cache {
        let (outcome, cached) = cache.lookup(&resolved.options);
        if let Some(cached) = cached {
            return Ok((SourceScan::Cached(Box::new(cached)), Some(outcome)));
        }
        let discovery = discover(resolved)?;
        // A store failure is reported rather than swallowed: a cache directory
        // that cannot be written is a configuration mistake, and silently
        // rescanning forever is how it stays unnoticed.
        cache.store(&resolved.options, &discovery)?;
        return Ok((SourceScan::Fresh(Box::new(discovery)), Some(outcome)));
    }
    Ok((SourceScan::Fresh(Box::new(discover(resolved)?)), None))
}

/// Runs discovery, taking the relaxed root bound when the roots are a file list.
///
/// A `--since` over a large merge names thousands of files, and every one of
/// them becomes a root. That is well past the bound meant for a hand-written
/// command line, and the file limit still applies to the result.
fn discover(resolved: &ResolvedWorkspaceInput) -> Result<WorkspaceDiscovery> {
    let discovery = if resolved.from_list {
        discover_workspace_files_from_list(&resolved.options)?
    } else {
        discover_workspace_files(&resolved.options)?
    };
    Ok(discovery)
}
