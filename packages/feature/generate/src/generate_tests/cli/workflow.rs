use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_cli::shared::{
    analyze_files, expand_input_files, total_file_failure, write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_feature_project_inventory::test_map_report::usecase::build_test_map_report;

use crate::generate_tests::cli::args::GenerateTestsArgs;
use crate::generate_tests::cli::render::print_test_stub_plan;
use crate::generate_tests::usecase::{TestStub, build_test_stubs};

/// One file's outcome: either it parsed as Common Lisp and yielded zero or
/// more stubs, or it was skipped for being another dialect.
///
/// A file that fails to *read or parse* is not a third variant here — see the
/// hard-fail check below for why.
enum FileOutcome {
    Stubs(Vec<TestStub>),
    SkippedDialect(PathBuf),
}

pub fn generate_tests(args: GenerateTestsArgs) -> CliResult<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let analysis = analyze_files(&files, args.dialect, |file, dialect, tree, _input| {
        CliResult::Ok(if dialect == Dialect::CommonLisp {
            let report = build_test_map_report(file, dialect, tree);
            FileOutcome::Stubs(build_test_stubs(&report.findings))
        } else {
            FileOutcome::SkippedDialect(file.clone())
        })
    });
    // Generated test stubs are meant to cover every file passed in; silently
    // excluding one that failed to read or parse (the tolerant `analyze_files`
    // convention used by `query`/`report` commands) would produce a stub set
    // missing a file with no visible sign why. Failing hard here preserves
    // this command's original behavior, where the per-file read/parse `?`
    // aborted the whole run on the first bad file.
    if let Some(failure) = analysis.failed.into_iter().next() {
        return Err(total_file_failure(vec![failure]).into());
    }

    let mut stubs = Vec::new();
    let mut skipped_dialect = Vec::new();
    for outcome in analysis.succeeded {
        match outcome {
            FileOutcome::Stubs(file_stubs) => stubs.extend(file_stubs),
            FileOutcome::SkippedDialect(file) => skipped_dialect.push(file),
        }
    }

    let mut written = false;
    if args.write {
        let into =
            args.into
                .as_ref()
                .ok_or_else(|| paredit_core_cli::ArgumentError::FlagCombination {
                    message: "--write requires --into".to_owned(),
                })?;
        if !stubs.is_empty() {
            let mut content = std::fs::read_to_string(into).unwrap_or_default();
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            for stub in &stubs {
                content.push_str(&stub.generated);
            }
            SyntaxTree::parse(&content).map_err(|source| {
                crate::error::GeneratedOutputWouldNotParse {
                    summary: "the appended test skeleton(s) would leave the file unparseable",
                    source,
                }
            })?;
            write_file_with_rollback(into.clone(), content)?;
            written = true;
        }
    }

    print_test_stub_plan(&stubs, &skipped_dialect, written, args.output)
}
