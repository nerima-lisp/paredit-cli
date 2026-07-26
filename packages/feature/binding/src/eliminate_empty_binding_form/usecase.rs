//! Application safety policy for eliminating empty binding forms.

use anyhow::{Context, Result, bail};

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::progn as domain;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{Path, SyntaxTree};

pub use domain::{EliminateEmptyBindingFormPlan, EliminateEmptyBindingFormRequest};

pub fn plan_eliminate_empty_binding_form(
    request: EliminateEmptyBindingFormRequest<'_>,
) -> Result<EliminateEmptyBindingFormPlan> {
    domain::require_supported(request.dialect, "eliminate-empty-binding-form")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("input is not valid")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    require_known_expression_context(&tree, &request.path, request.dialect)?;
    domain::plan_eliminate_empty_binding_form(request)
}

fn require_known_expression_context(
    tree: &SyntaxTree,
    path: &Path,
    dialect: Dialect,
) -> Result<()> {
    let indexes = path.to_raw_indexes();
    if indexes.len() < 2 {
        bail!("eliminate-empty-binding-form refuses top-level forms");
    }
    for depth in 1..indexes.len() {
        if !tree
            .select_path(&Path::from_indexes(indexes[..depth].to_vec()))?
            .view()
            .reader_prefixes
            .is_empty()
        {
            bail!("refuses reader-prefixed contexts");
        }
    }
    let child_index = *indexes.last().context("non-empty path")?;
    let parent = tree
        .select_path(&Path::from_indexes(indexes[..indexes.len() - 1].to_vec()))?
        .view();
    let head = parent
        .children
        .first()
        .and_then(atom_symbol_text)
        .context("known expression context required")?;
    let is = |expected| {
        if dialect == Dialect::CommonLisp {
            common_lisp_symbol_reference_eq(head, expected)
        } else {
            head == expected
        }
    };
    let known = (is("progn") && child_index >= 1)
        || (is("if") && (1..=3).contains(&child_index))
        || ((is("when") || is("unless")) && child_index >= 1)
        || ((is("let") || is("let*")) && child_index >= 2)
        || (is("lambda") && child_index >= 2)
        || (is("defun") && child_index >= 3);
    if known {
        Ok(())
    } else {
        bail!("eliminate-empty-binding-form requires a known expression position")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALECTS: [Dialect; 7] = [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ];

    fn request<'a>(
        input: &'a str,
        dialect: Dialect,
        path: &str,
    ) -> EliminateEmptyBindingFormRequest<'a> {
        EliminateEmptyBindingFormRequest {
            input,
            dialect,
            path: path.parse().expect("path"),
        }
    }

    #[test]
    fn all_dialects_are_gated_before_parsing() {
        let support_error = "eliminate-empty-binding-form supports only Common Lisp and Emacs Lisp";
        for dialect in DIALECTS {
            let error =
                plan_eliminate_empty_binding_form(request(")", dialect, "0.1")).unwrap_err();
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
            (Dialect::CommonLisp, r"(progn (let () a b)) #\)"),
            (Dialect::EmacsLisp, r"(progn (let () a b)) ?\)"),
        ] {
            let plan = plan_eliminate_empty_binding_form(request(input, dialect, "0.1"))
                .expect("elimination");
            assert!(plan.changed);
        }
    }
}
