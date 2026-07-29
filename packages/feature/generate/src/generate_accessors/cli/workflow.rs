use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_cli::shared::{
    apply_byte_span_edits, read_input_dialect_and_tree, resolve_compact_target, unified_diff,
    write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::generate_accessors::cli::args::GenerateAccessorsArgs;
use crate::generate_accessors::cli::render::print_accessors_plan;
use crate::generate_accessors::usecase::{AccessorsOutcome, plan_accessors};

pub fn generate_accessors(args: GenerateAccessorsArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    if dialect != Dialect::CommonLisp {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputDialectUnsupported,
            format!(
                "generate accessors supports only Common Lisp, found {}",
                dialect.label()
            ),
        )
        .into());
    }
    let target = resolve_compact_target(&tree, dialect, &args.selector, "generate accessors")?;
    let selection = tree.select_path(&target.path)?;
    let selected = selection.view();

    let outcome = plan_accessors(&input.text, &selected);
    let rewritten = match &outcome {
        AccessorsOutcome::Ready { edits, .. } => {
            let byte_edits = edits
                .iter()
                .map(|edit| (edit.span, edit.replacement.clone()))
                .collect::<Vec<_>>();
            let rewritten = apply_byte_span_edits(&input.text, byte_edits)?;
            SyntaxTree::parse_with_dialect(&rewritten, dialect).map_err(|source| {
                crate::error::GeneratedOutputWouldNotParse {
                    summary: "the generated accessors would leave the file unparseable",
                    source,
                }
            })?;
            rewritten
        }
        AccessorsOutcome::Nothing { .. } => input.text.clone(),
        AccessorsOutcome::Unsupported { reason } => {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,
                format!("generate accessors cannot use the selected form: {reason}"),
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

    print_accessors_plan(&outcome, written, args.output)
}
