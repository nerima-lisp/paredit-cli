//! Application facade for converting independent `let*` into `let`.

use crate::error::BindingResult;
use paredit_core_edit::let_binding as domain;
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::sexpr::SyntaxTree;

pub use domain::{ConvertLetStarToLetPlan, ConvertLetStarToLetRequest};

pub fn plan_convert_let_star_to_let(
    request: ConvertLetStarToLetRequest<'_>,
) -> BindingResult<ConvertLetStarToLetPlan> {
    domain::validate_convert_let_star_to_let_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    // The use case unions three error types - the edit's EditRefusal, a
    // ParseError, and ReaderConditionalSafetyError - so it stays anyhow until
    // this package's own section 9.2 pass.
    Ok(domain::plan_convert_let_star_to_let(request)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    #[test]
    fn accepts_common_lisp_reader_literal() {
        let input = r"#\) (let* ((value 1)) value)";
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
            allow_partial: false,
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
                allow_partial: false,
            })
            .expect_err("unsupported dialect");

            assert_eq!(
                error.to_string(),
                "convert-let-star-to-let currently supports only Common Lisp"
            );
        }
    }

    #[test]
    fn allow_partial_threads_through_the_facade() {
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "(let* ((a 1) (b (+ a 1))) (+ a b))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect("plan");

        assert!(plan.partial);
        assert_eq!(plan.rewritten, "(let ((a 1)) (let* ((b (+ a 1))) (+ a b)))");
    }

    #[test]
    fn reader_conditional_refusal_still_fires_with_allow_partial() {
        // `reject_common_lisp_reader_conditionals` runs before the domain
        // plan is even attempted, so this guard is identical whether or not
        // `allow_partial` is set.
        let error = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "#+sbcl (let* ((x 1) (y (+ x 1))) (+ x y))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect_err("reader conditional must still be refused");

        assert!(
            error.to_string().contains("reader conditional"),
            "{error:#}"
        );
    }
}
