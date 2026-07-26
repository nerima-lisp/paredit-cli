//! Application facade for converting independent `let*` into `let`.

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::let_binding as domain;
use crate::domain::sexpr::SyntaxTree;
use anyhow::Result;

pub use domain::{ConvertLetStarToLetPlan, ConvertLetStarToLetRequest};

pub fn plan_convert_let_star_to_let(
    request: ConvertLetStarToLetRequest<'_>,
) -> Result<ConvertLetStarToLetPlan> {
    domain::validate_convert_let_star_to_let_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    domain::plan_convert_let_star_to_let(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dialect::Dialect;

    #[test]
    fn accepts_common_lisp_reader_literal() {
        let input = r"#\) (let* ((value 1)) value)";
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("plan");

        assert_eq!(plan.rewritten, r"#\) (let ((value 1)) value)");
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
            let error = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");

            assert_eq!(
                error.to_string(),
                "convert-let-star-to-let currently supports only Common Lisp"
            );
        }
    }
}
