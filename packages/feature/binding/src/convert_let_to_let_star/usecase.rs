//! Application facade for converting dependency-free `let` into `let*`.

use crate::error::BindingResult;
use paredit_core_edit::let_binding as domain;
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::sexpr::SyntaxTree;

pub use domain::{ConvertLetToLetStarPlan, ConvertLetToLetStarRequest};

pub fn plan_convert_let_to_let_star(
    request: ConvertLetToLetStarRequest<'_>,
) -> BindingResult<ConvertLetToLetStarPlan> {
    domain::validate_convert_let_to_let_star_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    // The use case unions three error types - the edit's EditRefusal, a
    // ParseError, and ReaderConditionalSafetyError - so it stays anyhow until
    // this package's own section 9.2 pass.
    Ok(domain::plan_convert_let_to_let_star(request)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    #[test]
    fn accepts_supported_dialect_reader_literals() {
        let cases = [
            (
                Dialect::CommonLisp,
                r"#\) (let ((value 1)) value)",
                r"#\) (let* ((value 1)) value)",
            ),
            (
                Dialect::EmacsLisp,
                r"?\) (let ((value 1)) value)",
                r"?\) (let* ((value 1)) value)",
            ),
        ];

        for (dialect, input, expected) in cases {
            let plan = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input,
                dialect,
                path: "1".parse().expect("path"),
            })
            .unwrap_or_else(|error| panic!("{}: {error}", dialect.label()));

            assert_eq!(plan.rewritten, expected);
        }
    }

    #[test]
    fn unsupported_dialect_gate_precedes_parsing() {
        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");

            assert_eq!(
                error.to_string(),
                "convert-let-to-let-star supports only Common Lisp and Emacs Lisp"
            );
        }
    }
}
