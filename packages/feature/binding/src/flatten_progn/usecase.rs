//! Application safety policy for the Domain `progn` transformation.

use anyhow::{Result, bail};

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::progn as domain;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{Path, SyntaxTree};

pub use domain::{FlattenPrognPlan, FlattenPrognRequest};

pub fn plan_flatten_progn(request: FlattenPrognRequest<'_>) -> Result<FlattenPrognPlan> {
    domain::require_supported(request.dialect, "flatten-progn")?;
    if request.path.indexes().len() < 2 {
        bail!("flatten-progn refuses to rewrite a top-level progn");
    }
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    reject_unsafe_context(&tree, &request.path)?;
    domain::plan_flatten_progn(request)
}

fn reject_unsafe_context(tree: &SyntaxTree, path: &Path) -> Result<()> {
    if path.indexes().last().is_some_and(|index| index.get() == 0) {
        bail!("flatten-progn refuses to rewrite an operator position");
    }
    let mut ancestor = path.parent();
    while let Some(ancestor_path) = ancestor {
        if ancestor_path.indexes().is_empty() {
            break;
        }
        let view = tree.select_path(&ancestor_path)?.view();
        if !view.reader_prefixes.is_empty() {
            bail!("flatten-progn refuses to rewrite inside a reader template");
        }
        if view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, "declare"))
        {
            bail!("flatten-progn refuses to rewrite inside a declaration");
        }
        ancestor = ancestor_path.parent();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dialect::Dialect;

    const DIALECTS: [Dialect; 7] = [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ];

    fn request<'a>(input: &'a str, dialect: Dialect, path: &str) -> FlattenPrognRequest<'a> {
        FlattenPrognRequest {
            input,
            dialect,
            path: path.parse().expect("path"),
        }
    }

    #[test]
    fn all_dialects_are_gated_before_parsing() {
        let support_error = "flatten-progn supports only Common Lisp and Emacs Lisp";
        for dialect in DIALECTS {
            let error = plan_flatten_progn(request(")", dialect, "0.1")).unwrap_err();
            if matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
                assert_ne!(error.to_string(), support_error, "{dialect:?}: {error:#}");
            } else {
                assert_eq!(error.to_string(), support_error, "{dialect:?}");
            }
        }
    }

    #[test]
    fn supported_reader_collisions_use_the_requested_dialect() {
        for (dialect, input) in [
            (Dialect::CommonLisp, r"(progn (progn a (progn b c))) #\)"),
            (Dialect::EmacsLisp, r"(progn (progn a (progn b c))) ?\)"),
        ] {
            let plan = plan_flatten_progn(request(input, dialect, "0.1")).expect("flatten");
            assert!(plan.changed);
        }
    }
}
