//! Use-case helpers for extracting one expression into a top-level constant.

use anyhow::Result;

use crate::application::usecase::extract_shared::TopLevelInsert;
use crate::application::usecase::mutation_safety::reject_overlapping_common_lisp_reader_time_forms;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path, Selection, SymbolName, SyntaxTree};

pub type ExtractConstantInsert = TopLevelInsert;

#[derive(Debug, Clone)]
pub struct ExtractConstantRequest<'a> {
    pub input: &'a str,
    pub tree: &'a SyntaxTree,
    pub selection: Selection<'a>,
    pub path: Path,
    pub dialect: Dialect,
    pub name: SymbolName,
    pub insert: ExtractConstantInsert,
    pub anchor_path: Option<Path>,
}

#[derive(Debug, Clone)]
pub struct ExtractConstantPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub span_start: usize,
    pub span_end: usize,
    pub name: SymbolName,
    pub insert: ExtractConstantInsert,
    pub anchor_path: Option<Path>,
    pub anchor_span: Option<ByteSpan>,
    pub replacement: String,
    pub definition: String,
    pub rewritten: String,
    pub changed: bool,
}

pub fn path_for_selection(tree: &SyntaxTree, selection: Selection<'_>) -> Result<Path> {
    crate::domain::extract_constant::path_for_selection(tree, selection)
}

pub fn plan_extract_constant(request: ExtractConstantRequest<'_>) -> Result<ExtractConstantPlan> {
    request
        .selection
        .validate_context(request.input, request.tree)?;
    if request.dialect == Dialect::CommonLisp {
        reject_overlapping_common_lisp_reader_time_forms(
            request.tree,
            request.dialect,
            [request.selection.span()],
        )?;
    }
    let plan = crate::domain::extract_constant::plan(crate::domain::extract_constant::Request {
        input: request.input,
        tree: request.tree,
        selection: request.selection,
        path: request.path,
        dialect: request.dialect,
        name: request.name,
        insert: request.insert,
        anchor_path: request.anchor_path,
    })?;
    Ok(ExtractConstantPlan {
        dialect: plan.dialect,
        path: plan.path,
        span_start: plan.span_start,
        span_end: plan.span_end,
        name: plan.name,
        insert: request.insert,
        anchor_path: plan.anchor_path,
        anchor_span: plan.anchor_span,
        replacement: plan.replacement,
        definition: plan.definition,
        changed: plan.changed,
        rewritten: plan.rewritten,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn plan(input: &str, path: &str, dialect: Dialect) -> Result<ExtractConstantPlan> {
        let tree = SyntaxTree::parse(input)?;
        let path = Path::from_str(path)?;
        let selection = tree.select_path(&path)?;
        plan_extract_constant(ExtractConstantRequest {
            input,
            tree: &tree,
            selection,
            path,
            dialect,
            name: SymbolName::new("answer")?,
            insert: ExtractConstantInsert::Append,
            anchor_path: None,
        })
    }

    #[test]
    fn plans_common_lisp_constant() {
        let plan = plan("(defun f () (+ 40 2))", "0.3", Dialect::CommonLisp).unwrap();
        assert_eq!(plan.definition, "(defconstant answer (+ 40 2))");
        assert_eq!(
            plan.rewritten,
            "(defun f () answer)\n\n(defconstant answer (+ 40 2))\n"
        );
    }

    #[test]
    fn plans_emacs_lisp_constant() {
        let plan = plan("(defun f () (+ 40 2))", "0.3", Dialect::EmacsLisp).unwrap();
        assert_eq!(plan.definition, "(defconst answer (+ 40 2))");
    }

    #[test]
    fn rejects_quote_and_definition_head() {
        assert!(
            plan("(defun f () '(+ 40 2))", "0.3.1", Dialect::CommonLisp)
                .unwrap_err()
                .to_string()
                .contains("quote")
        );
        assert!(
            plan("(defun f () 42)", "0.0", Dialect::CommonLisp)
                .unwrap_err()
                .to_string()
                .contains("definition head")
        );
    }

    #[test]
    fn resolves_selection_path_in_domain() {
        let tree = SyntaxTree::parse("(defun f () (+ 40 2))").unwrap();
        let selection = tree.select_path(&Path::from_str("0.3").unwrap()).unwrap();

        assert_eq!(
            path_for_selection(&tree, selection).unwrap().to_string(),
            "0.3"
        );
    }

    #[test]
    fn resolves_top_level_selection_path() {
        let tree = SyntaxTree::parse("(defun f () (+ 40 2))").unwrap();
        let selection = tree.select_at(0).unwrap();
        let path = path_for_selection(&tree, selection).unwrap();

        assert_eq!(path.to_string(), "0");
    }

    #[test]
    fn rejects_selection_source_mismatches_without_panicking() {
        let source = "(defun f () α)";
        let tree = SyntaxTree::parse(source).unwrap();
        let path = Path::from_str("0.3").unwrap();
        let selection = tree.select_path(&path).unwrap();

        for input in ["(defun g () β)", "(x)", "(defun f () aé)"] {
            let error = plan_extract_constant(ExtractConstantRequest {
                input,
                tree: &tree,
                selection,
                path: path.clone(),
                dialect: Dialect::CommonLisp,
                name: SymbolName::new("answer").unwrap(),
                insert: ExtractConstantInsert::Append,
                anchor_path: None,
            })
            .expect_err("mismatched selection source must be rejected");

            assert!(
                error
                    .to_string()
                    .contains("source used to build the selection")
            );
        }
    }

    #[test]
    fn rejects_selection_from_a_different_tree_with_the_same_source() {
        let input = "(defun f () 42)";
        let selection_tree = SyntaxTree::parse(input).unwrap();
        let request_tree = SyntaxTree::parse(input).unwrap();
        let path = Path::from_str("0.3").unwrap();
        let selection = selection_tree.select_path(&path).unwrap();

        let error = plan_extract_constant(ExtractConstantRequest {
            input,
            tree: &request_tree,
            selection,
            path,
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("answer").unwrap(),
            insert: ExtractConstantInsert::Append,
            anchor_path: None,
        })
        .expect_err("selection from another tree must be rejected");

        assert!(error.to_string().contains("different syntax tree"));
    }
}
