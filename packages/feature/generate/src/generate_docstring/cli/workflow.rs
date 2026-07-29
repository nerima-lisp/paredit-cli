use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_cli::shared::{
    apply_byte_span_edits, read_input_dialect_and_tree, resolve_compact_target, unified_diff,
    write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};

use crate::generate_docstring::cli::args::GenerateDocstringArgs;
use crate::generate_docstring::cli::render::print_docstring_plan;
use crate::generate_docstring::usecase::{DocstringOutcome, plan_docstring};

pub fn generate_docstring(args: GenerateDocstringArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    if dialect != Dialect::CommonLisp {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputDialectUnsupported,
            format!(
                "generate docstring supports only Common Lisp, found {}",
                dialect.label()
            ),
        )
        .into());
    }
    let target = resolve_compact_target(&tree, dialect, &args.selector, "generate docstring")?;
    let selection = tree.select_path(&target.path)?;
    let selected = selection.view();

    let outcome = plan_docstring(&selected);
    let rewritten = match &outcome {
        DocstringOutcome::Ready(plan) => {
            let offset = ByteOffset::new(plan.insertion_offset);
            let rewritten = apply_byte_span_edits(
                &input.text,
                vec![(ByteSpan::new(offset, offset), plan.insertion_text.clone())],
            )?;
            SyntaxTree::parse_with_dialect(&rewritten, dialect).map_err(|source| {
                crate::error::GeneratedOutputWouldNotParse {
                    summary: "the generated docstring would leave the file unparseable",
                    source,
                }
            })?;
            rewritten
        }
        DocstringOutcome::AlreadyDocumented => input.text.clone(),
        DocstringOutcome::Unsupported { reason } => {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
                format!("generate docstring cannot document the selected form: {reason}"),
            )
            .into());
        }
    };

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

    print_docstring_plan(&outcome, written, args.output)
}
