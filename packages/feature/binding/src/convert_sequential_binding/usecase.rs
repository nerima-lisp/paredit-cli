//! Application safety facade for sequential-binding domain plans.

use anyhow::Result;

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::convert_sequential_binding as domain;
use crate::domain::sexpr::SyntaxTree;

pub use domain::{ConvertSequentialBindingPlan, ConvertSequentialBindingRequest};

fn safe(request: &ConvertSequentialBindingRequest<'_>, command: &str) -> Result<()> {
    domain::require_supported_dialect(request.dialect, command)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    Ok(reject_common_lisp_reader_conditionals(
        &tree,
        request.dialect,
    )?)
}

pub fn plan_convert_do_star_to_do(
    request: ConvertSequentialBindingRequest<'_>,
) -> Result<ConvertSequentialBindingPlan> {
    safe(&request, "convert-do-star-to-do")?;
    domain::plan_convert_do_star_to_do(request)
}
pub fn plan_convert_prog_star_to_prog(
    request: ConvertSequentialBindingRequest<'_>,
) -> Result<ConvertSequentialBindingPlan> {
    safe(&request, "convert-prog-star-to-prog")?;
    domain::plan_convert_prog_star_to_prog(request)
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

    fn request(input: &str, dialect: Dialect) -> ConvertSequentialBindingRequest<'_> {
        ConvertSequentialBindingRequest {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    #[test]
    fn every_command_gates_all_dialects_before_parsing() {
        for dialect in DIALECTS {
            let cases = [
                (
                    plan_convert_do_star_to_do(request(")", dialect)).unwrap_err(),
                    "convert-do-star-to-do currently supports only Common Lisp",
                ),
                (
                    plan_convert_prog_star_to_prog(request(")", dialect)).unwrap_err(),
                    "convert-prog-star-to-prog currently supports only Common Lisp",
                ),
            ];
            for (error, support_error) in cases {
                if dialect == Dialect::CommonLisp {
                    assert_ne!(error.to_string(), support_error, "{dialect:?}: {error:#}");
                } else {
                    assert_eq!(error.to_string(), support_error, "{dialect:?}");
                }
            }
        }
    }

    #[test]
    fn common_lisp_reader_collision_uses_the_requested_dialect() {
        let input =
            r"(do* ((x (first) (next-x)) (y (second) (next-y))) ((done-p x y) y) (work x)) #\)";
        let plan =
            plan_convert_do_star_to_do(request(input, Dialect::CommonLisp)).expect("conversion");
        assert!(plan.changed);
    }
}
