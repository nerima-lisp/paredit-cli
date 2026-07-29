use std::path::PathBuf;

use paredit_core_cli::CliResult;

use paredit_core_cli::shared::{
    apply_byte_span_edits, read_input_dialect_and_tree, unified_diff, write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};

use crate::generate_defgeneric::cli::args::GenerateDefgenericArgs;
use crate::generate_defgeneric::cli::render::print_defgeneric_plan;
use crate::generate_defgeneric::usecase::{Candidate, find_by_name, find_undeclared_generics};

pub fn generate_defgeneric(args: GenerateDefgenericArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    if dialect != Dialect::CommonLisp {
        return Err(paredit_core_cli::error::FeatureRefusal::message(
            paredit_core_cli::diagnosis::ErrorCode::InputDialectUnsupported,
            format!(
                "generate defgeneric supports only Common Lisp, found {}",
                dialect.label()
            ),
        )
        .into());
    }

    let candidates = match &args.name {
        Some(name) => find_by_name(&tree, name)
            .map(|candidate| vec![candidate])
            .ok_or_else(|| {
                paredit_core_cli::error::FeatureRefusal::message(
                    paredit_core_cli::diagnosis::ErrorCode::SelectionNoMatch,
                    format!("no defmethod named {name} without a defgeneric"),
                )
            })?,
        None => find_undeclared_generics(&tree),
    };

    let mut ready = candidates
        .iter()
        .filter_map(|candidate| match candidate {
            Candidate::Ready(generic) => Some(generic),
            Candidate::Unready(_) => None,
        })
        .collect::<Vec<_>>();
    ready.sort_by_key(|generic| generic.insertion_offset);

    let edits = ready
        .iter()
        .map(|generic| {
            let offset = ByteOffset::new(generic.insertion_offset);
            (ByteSpan::new(offset, offset), generic.generated.clone())
        })
        .collect::<Vec<_>>();

    let rewritten = apply_byte_span_edits(&input.text, edits)?;
    if rewritten != input.text {
        SyntaxTree::parse_with_dialect(&rewritten, dialect).map_err(|source| {
            crate::error::GeneratedOutputWouldNotParse {
                summary: "the generated defgeneric form(s) would leave the file unparseable",
                source,
            }
        })?;
    }

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

    print_defgeneric_plan(&candidates, written, args.output)
}
