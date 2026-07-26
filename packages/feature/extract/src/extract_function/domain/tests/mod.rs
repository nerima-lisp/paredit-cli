use super::*;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

mod inference;
mod pbt;
mod planning;

fn infer_at(input: &str, path: &[usize], explicit: &[&str]) -> Vec<String> {
    infer_at_dialect(Dialect::CommonLisp, input, path, explicit)
}

fn infer_at_dialect(
    dialect: Dialect,
    input: &str,
    path: &[usize],
    explicit: &[&str],
) -> Vec<String> {
    let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse fixture");
    let selection = tree
        .select_path(&Path::from_indexes(path.to_vec()))
        .expect("select fixture");
    let explicit = explicit
        .iter()
        .map(|param| (*param).to_owned())
        .collect::<Vec<_>>();

    infer_extract_function_params(dialect, &selection.view(), &explicit)
}

fn plan_at(
    input: &str,
    path: &[usize],
    name: &str,
    explicit: &[&str],
    infer_params: bool,
) -> ExtractFunctionPlan {
    plan_at_dialect(
        Dialect::CommonLisp,
        input,
        path,
        name,
        explicit,
        infer_params,
    )
}

fn plan_at_dialect(
    dialect: Dialect,
    input: &str,
    path: &[usize],
    name: &str,
    explicit: &[&str],
    infer_params: bool,
) -> ExtractFunctionPlan {
    let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse fixture");
    let selection = tree
        .select_path(&Path::from_indexes(path.to_vec()))
        .expect("select fixture");
    let explicit_params = explicit
        .iter()
        .map(|param| (*param).to_owned())
        .collect::<Vec<_>>();

    plan_extract_function(ExtractFunctionRequest {
        input,
        selection,
        path: Some(Path::from_indexes(path.to_vec())),
        dialect,
        name: SymbolName::new(name).expect("symbol fixture"),
        explicit_params,
        infer_params,
        insert: ExtractFunctionInsert::Append,
        anchor_path: None,
    })
    .expect("plan extract function")
}
