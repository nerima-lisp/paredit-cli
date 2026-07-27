//! Application facade for converting a selected `if` form into `cond`.

use anyhow::Result;

use crate::application::usecase::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::convert_control as domain;
use crate::domain::sexpr::SyntaxTree;

pub use domain::{ConvertIfToCondPlan, ConvertIfToCondRequest};

pub fn plan_convert_if_to_cond(request: ConvertIfToCondRequest<'_>) -> Result<ConvertIfToCondPlan> {
    domain::require_supported_dialect(request.dialect, "convert-if-to-cond")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    domain::plan_convert_if_to_cond(request)
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

    fn request<'a>(input: &'a str, dialect: Dialect, path: &str) -> ConvertIfToCondRequest<'a> {
        ConvertIfToCondRequest {
            input,
            dialect,
            path: path.parse().expect("path"),
        }
    }

    #[test]
    fn all_dialects_are_gated_before_parsing() {
        let support_error = "convert-if-to-cond currently supports only Common Lisp and Emacs Lisp";
        for dialect in DIALECTS {
            let error = plan_convert_if_to_cond(request(")", dialect, "0")).unwrap_err();
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
            (Dialect::CommonLisp, r"#\) (if ready yes no)"),
            (Dialect::EmacsLisp, r"?\) (if ready yes no)"),
        ] {
            let plan = plan_convert_if_to_cond(request(input, dialect, "1")).expect("conversion");
            assert!(plan.changed);
        }
    }
}
