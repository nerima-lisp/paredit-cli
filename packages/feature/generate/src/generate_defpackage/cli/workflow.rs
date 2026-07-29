use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_cli::shared::{
    apply_byte_span_edits, read_input_dialect_and_tree, unified_diff, write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};

use crate::generate_defpackage::cli::args::GenerateDefpackageArgs;
use crate::generate_defpackage::cli::render::print_defpackage_plan;
use crate::generate_defpackage::usecase::plan_defpackage;

pub fn generate_defpackage(args: GenerateDefpackageArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    if dialect != Dialect::CommonLisp {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputDialectUnsupported,
            format!(
                "generate defpackage supports only Common Lisp, found {}",
                dialect.label()
            ),
        )
        .into());
    }

    let package_name = args
        .package_name
        .clone()
        .unwrap_or_else(|| default_package_name(input.file.as_deref()));
    let plan = plan_defpackage(&package_name, &tree);

    let edit = match plan.replaces {
        Some(span) => (span, plan.generated.clone()),
        None => {
            let start = ByteOffset::new(0);
            (ByteSpan::new(start, start), plan.generated.clone())
        }
    };
    let rewritten = apply_byte_span_edits(&input.text, vec![edit])?;
    SyntaxTree::parse_with_dialect(&rewritten, dialect).map_err(|source| {
        crate::error::GeneratedOutputWouldNotParse {
            summary: "the generated defpackage would leave the file unparseable",
            source,
        }
    })?;

    if args.diff {
        let path = input
            .file
            .clone()
            .unwrap_or_else(|| PathBuf::from("<stdin>"));
        print!("{}", unified_diff(&path, &input.text, &rewritten));
        return Ok(());
    }

    let mut written = false;
    if args.write && rewritten != input.text {
        let file = input
            .file
            .as_ref()
            .ok_or(paredit_core_cli::ArgumentError::WriteRequiresFile)?;
        write_file_with_rollback(file.clone(), rewritten.clone())?;
        written = true;
    }

    print_defpackage_plan(&plan, written, args.output)
}

fn default_package_name(file: Option<&std::path::Path>) -> String {
    file.and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.replace('_', "-").to_ascii_lowercase())
        .unwrap_or_else(|| "app".to_owned())
}
