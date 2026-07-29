use std::io::BufRead;

use anyhow::{Context, Result};

use paredit_core_cli::color::{Painter, colorize_diff};
use paredit_core_cli::shared::{
    apply_byte_span_edits, read_text_file_with_limit, stable_text_hash, unified_diff,
    write_file_with_rollback,
};
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::refactor::cli::manifest::io::read_refactor_manifest_file;
use crate::refactor::cli::manifest::parse::parse_refactor_apply_manifest;
use crate::refactor_step::cli::args::RefactorStepArgs;
use crate::refactor_step::cli::render::print_steps;
use crate::refactor_step::domain::{Selection, Step, parse_selector, selection_is_coherent, steps};

/// The same ceiling `refactor apply` reads a source under.
const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;

pub fn refactor_step(args: RefactorStepArgs) -> Result<()> {
    let loaded = read_refactor_manifest_file(&args.manifest, args.expect_manifest_hash.as_deref())?;
    let manifest = parse_refactor_apply_manifest(&loaded.value)?;

    // Only the files the manifest says it changes. An unchanged file has no
    // steps, and listing it would make the numbering depend on files nothing
    // happens to.
    let changed: Vec<_> = manifest
        .files
        .iter()
        .filter(|file| file.changed && !file.edits.is_empty())
        .collect();

    let mut sources = Vec::with_capacity(changed.len());
    for file in &changed {
        let text = read_text_file_with_limit(&file.path, MAX_SOURCE_BYTES)
            .with_context(|| format!("failed to read {}", file.path.display()))?;
        // The guard `refactor apply` applies, applied before anything is even
        // listed: a manifest describes edits at byte offsets into the file as
        // it was, and against a file that has moved on those offsets name
        // different text.
        let hash = stable_text_hash(&text);
        if hash != file.input_hash {
            anyhow::bail!(
                "{} has changed since the manifest was written (expected {}, found {}); \
                 re-run `refactor preview` before stepping",
                file.path.display(),
                file.input_hash,
                hash,
            );
        }
        sources.push(text);
    }

    let described: Vec<(String, Vec<_>)> = changed
        .iter()
        .map(|file| {
            (
                file.path.display().to_string(),
                file.edits
                    .iter()
                    .map(|edit| (edit.span, edit.replacement.clone()))
                    .collect(),
            )
        })
        .collect();
    let steps = steps(&described, &sources);

    let selection = select(&args, &steps)?;
    let coherent = selection_is_coherent(&steps, &selection);

    if args.diff {
        let painter = Painter::stdout();
        for (index, file) in changed.iter().enumerate() {
            let rewritten = rewrite(&sources[index], &steps, &selection, index)?;
            if rewritten != sources[index] {
                print!(
                    "{}",
                    colorize_diff(
                        painter,
                        &unified_diff(&file.path, &sources[index], &rewritten)
                    )
                );
            }
        }
        return gate(&args, coherent, selection.count(), steps.len());
    }

    let mut written = Vec::new();
    if args.write {
        for (index, file) in changed.iter().enumerate() {
            let rewritten = rewrite(&sources[index], &steps, &selection, index)?;
            if rewritten == sources[index] {
                continue;
            }
            // A subset of a rewrite is not guaranteed to be balanced source —
            // that is precisely what taking part of a change risks — so the
            // result is parsed before it is written, not after.
            SyntaxTree::parse_with_dialect(&rewritten, file.dialect).with_context(|| {
                format!(
                    "refusing to write {}: the selected steps do not leave a parseable document",
                    file.path.display(),
                )
            })?;
            write_file_with_rollback(file.path.clone(), rewritten)?;
            written.push(file.path.display().to_string());
        }
    }

    print_steps(&steps, &selection, &written, args.write, args.output)?;
    gate(&args, coherent, selection.count(), steps.len())
}

/// Applies the accepted steps belonging to one file.
fn rewrite(
    source: &str,
    steps: &[Step],
    selection: &Selection,
    file_index: usize,
) -> Result<String> {
    let edits = steps
        .iter()
        .filter(|step| step.file_index == file_index && selection.contains(step.number))
        .map(|step| (step.span, step.after.clone()))
        .collect::<Vec<_>>();
    Ok(apply_byte_span_edits(source, edits)?)
}

fn select(args: &RefactorStepArgs, steps: &[Step]) -> Result<Selection> {
    if args.interactive {
        return interactive(steps);
    }

    let mut selection = match &args.accept {
        Some(spec) => {
            let accepted = parse_selector(spec, steps.len()).map_err(anyhow::Error::msg)?;
            let mut selection = Selection::none();
            for number in accepted {
                selection.accept(number);
            }
            selection
        }
        // `--skip` alone means "everything but these", which is the only
        // reading that makes it useful on its own.
        None if args.skip.is_some() => Selection::all(steps.len()),
        // Neither flag: listing, not choosing. Everything is shown as selected
        // so `--write` with no selector behaves like `refactor apply`.
        None => Selection::all(steps.len()),
    };

    if let Some(spec) = &args.skip {
        for number in parse_selector(spec, steps.len()).map_err(anyhow::Error::msg)? {
            selection.skip(number);
        }
    }
    Ok(selection)
}

/// Reads one decision per step from standard input.
///
/// The `git add -p` vocabulary, because a reviewer stepping a refactor already
/// knows it. Prompts go to stderr so a caller can pipe the JSON result
/// somewhere while still driving the prompts.
fn interactive(steps: &[Step]) -> Result<Selection> {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut selection = Selection::none();
    let mut take_the_rest = false;

    for step in steps {
        if take_the_rest {
            selection.accept(step.number);
            continue;
        }
        eprintln!(
            "step {}/{}  {}:{}  {} -> {}",
            step.number,
            steps.len(),
            step.path,
            step.line,
            step.before,
            step.after,
        );
        eprint!("  {}\n  take it? [y/n/a/q] ", step.context);
        let Some(line) = lines.next() else {
            // Input ended. Taking the remaining steps because the caller
            // stopped answering would be deciding for them.
            break;
        };
        match line?.trim().chars().next() {
            Some('y' | 'Y') => selection.accept(step.number),
            Some('a' | 'A') => {
                selection.accept(step.number);
                take_the_rest = true;
            }
            Some('q' | 'Q') => break,
            _ => {}
        }
    }
    Ok(selection)
}

fn gate(args: &RefactorStepArgs, coherent: bool, accepted: usize, total: usize) -> Result<()> {
    if !args.fail_on_partial || coherent {
        return Ok(());
    }
    Err(paredit_core_cli::gate::gate_failure(format!(
        "refactor step policy failed: {accepted} of {total} step(s) selected, and a manifest's \
         edits are one change"
    )))
}
