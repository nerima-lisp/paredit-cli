//! Application facade for converting a capture-free Common Lisp `flet` into `labels`.

use anyhow::Result;

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::local_function_binding as domain;
use crate::domain::sexpr::SyntaxTree;

pub use domain::{ConvertFletToLabelsPlan, ConvertFletToLabelsRequest};

pub fn plan_convert_flet_to_labels(
    request: ConvertFletToLabelsRequest<'_>,
) -> Result<ConvertFletToLabelsPlan> {
    domain::validate_convert_flet_to_labels_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    domain::plan_convert_flet_to_labels(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dialect::Dialect;

    #[test]
    fn accepts_common_lisp_reader_literal() {
        let input = r"#\) (flet ((helper (value) value)) (helper 1))";
        let plan = plan_convert_flet_to_labels(ConvertFletToLabelsRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("plan");

        assert_eq!(
            plan.rewritten,
            r"#\) (labels ((helper (value) value)) (helper 1))"
        );
    }

    #[test]
    fn unsupported_dialect_gate_precedes_parsing() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_flet_to_labels(ConvertFletToLabelsRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");

            assert_eq!(
                error.to_string(),
                "convert-flet-to-labels supports only Common Lisp"
            );
        }
    }
}
