use anyhow::{Context, Result};

use paredit_core_cli::shared::{
    expand_input_files, read_input_dialect_and_tree, write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_feature_project_inventory::test_map_report::usecase::build_test_map_report;

use crate::generate_tests::cli::args::GenerateTestsArgs;
use crate::generate_tests::cli::render::print_test_stub_plan;
use crate::generate_tests::usecase::build_test_stubs;

pub fn generate_tests(args: GenerateTestsArgs) -> Result<()> {
    let files = expand_input_files(&args.files, args.dialect)?;

    let mut stubs = Vec::new();
    let mut skipped_dialect = Vec::new();
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        if dialect != Dialect::CommonLisp {
            skipped_dialect.push(file.clone());
            continue;
        }
        let report = build_test_map_report(file, dialect, &tree);
        stubs.extend(build_test_stubs(&report.findings));
    }

    let mut written = false;
    if args.write {
        let into = args.into.as_ref().context("--write requires --into")?;
        if !stubs.is_empty() {
            let mut content = std::fs::read_to_string(into).unwrap_or_default();
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            for stub in &stubs {
                content.push_str(&stub.generated);
            }
            SyntaxTree::parse(&content)
                .context("the appended test skeleton(s) would leave the file unparseable")?;
            write_file_with_rollback(into.clone(), content)?;
            written = true;
        }
    }

    print_test_stub_plan(&stubs, &skipped_dialect, written, args.output)
}
