//! Application safety facade for conditional-sugar domain plans.

use anyhow::Result;

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::conditional_sugar as domain;
use crate::domain::sexpr::SyntaxTree;

pub use domain::{ConditionalConversionPlan, ConditionalConversionRequest};

fn safe(request: &ConditionalConversionRequest<'_>) -> Result<()> {
    domain::require_supported_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    Ok(reject_common_lisp_reader_conditionals(
        &tree,
        request.dialect,
    )?)
}

pub fn plan_convert_when_to_if(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    safe(&request)?;
    domain::plan_convert_when_to_if(request)
}
pub fn plan_convert_unless_to_if(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    safe(&request)?;
    domain::plan_convert_unless_to_if(request)
}
pub fn plan_convert_if_to_when(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    safe(&request)?;
    domain::plan_convert_if_to_when(request)
}
pub fn plan_convert_if_to_unless(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    safe(&request)?;
    domain::plan_convert_if_to_unless(request)
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

    fn request(input: &str, dialect: Dialect) -> ConditionalConversionRequest<'_> {
        ConditionalConversionRequest {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    #[test]
    fn every_command_gates_all_dialects_before_parsing() {
        let support_error = "conditional conversion supports only Common Lisp and Emacs Lisp";
        for dialect in DIALECTS {
            let errors = [
                plan_convert_when_to_if(request(")", dialect)).unwrap_err(),
                plan_convert_unless_to_if(request(")", dialect)).unwrap_err(),
                plan_convert_if_to_when(request(")", dialect)).unwrap_err(),
                plan_convert_if_to_unless(request(")", dialect)).unwrap_err(),
            ];
            for error in errors {
                if matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
                    assert_ne!(error.to_string(), support_error, "{dialect:?}: {error:#}");
                } else {
                    assert_eq!(error.to_string(), support_error, "{dialect:?}");
                }
            }
        }
    }

    #[test]
    fn supported_reader_collisions_use_the_requested_dialect() {
        for (dialect, input) in [
            (Dialect::CommonLisp, r"(when ok one two) #\)"),
            (Dialect::EmacsLisp, r"(when ok one two) ?\)"),
        ] {
            let plan = plan_convert_when_to_if(request(input, dialect)).expect("conversion");
            assert!(plan.changed);
        }
    }
}
