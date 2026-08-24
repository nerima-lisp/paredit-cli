use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, total_file_failure, write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::generate_defsystem::cli::args::GenerateDefsystemArgs;
use crate::generate_defsystem::cli::render::print_defsystem_plan;
use crate::generate_defsystem::usecase::plan_defsystem;

/// One file's outcome: either it parsed as Common Lisp, or it was skipped for
/// being another dialect.
///
/// A file that fails to *read or parse* is not a third variant here — see the
/// hard-fail check below for why.
enum FileOutcome {
    Parsed(PathBuf, SyntaxTree),
    SkippedDialect(PathBuf),
}

pub fn generate_defsystem(args: GenerateDefsystemArgs) -> CliResult<()> {
    let system_name = args
        .name
        .clone()
        .unwrap_or_else(|| default_system_name(&args.directory));

    let files = expand_input_files(std::slice::from_ref(&args.directory), args.dialect)?;
    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _input| {
        CliResult::Ok(if dialect == Dialect::CommonLisp {
            FileOutcome::Parsed(file.clone(), tree.clone())
        } else {
            FileOutcome::SkippedDialect(file.clone())
        })
    });
    // A generated defsystem is meant to cover every file in the directory;
    // silently excluding one that failed to read or parse (the tolerant
    // `analyze_files` convention used by `query`/`report` commands) would
    // produce a manifest missing a file with no visible sign why. Failing
    // hard here preserves this command's original behavior, where the
    // per-file read/parse `?` aborted the whole run on the first bad file.
    if let Some(failure) = analysis.failed.into_iter().next() {
        return Err(total_file_failure(vec![failure]).into());
    }

    let mut parsed = Vec::new();
    let mut skipped_dialect = Vec::new();
    for outcome in analysis.succeeded {
        match outcome {
            FileOutcome::Parsed(file, tree) => parsed.push((file, tree)),
            FileOutcome::SkippedDialect(file) => skipped_dialect.push(file),
        }
    }

    let plan = plan_defsystem(&system_name, &parsed);
    SyntaxTree::parse(&plan.generated).map_err(|source| {
        crate::error::GeneratedOutputWouldNotParse {
            summary: "the generated defsystem would not be parseable",
            source,
        }
    })?;

    let mut written = false;
    if args.write {
        let target = args.directory.join(format!("{system_name}.asd"));
        if target.exists() && !args.force {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,
                format!("refusing to overwrite {} without --force", target.display()),
            )
            .into());
        }
        write_file_with_rollback(target, plan.generated.clone())?;
        written = true;
    }

    print_defsystem_plan(&plan, &skipped_dialect, written, args.output)
}

fn default_system_name(directory: &std::path::Path) -> String {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace('_', "-").to_ascii_lowercase())
        .unwrap_or_else(|| "app".to_owned())
}
