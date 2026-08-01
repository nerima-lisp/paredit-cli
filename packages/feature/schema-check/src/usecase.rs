//! `schema check` reporting: picking the schema a run validates against, and
//! building its report.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, ReportPolicy};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::domain::{SchemaIssue, validate_instance};
use crate::dsl::SchemaDef;
use crate::error::SchemaCheckError;

/// The one schema a run validates against: the file's only schema, the one
/// named by `--schema-name`, or a refusal listing what is available.
pub fn select_schema<'a>(
    schemas: &'a [SchemaDef],
    requested: Option<&str>,
    path: &str,
) -> Result<&'a SchemaDef, SchemaCheckError> {
    if schemas.is_empty() {
        return Err(SchemaCheckError::NoSchemas {
            path: path.to_owned(),
        });
    }

    if let Some(name) = requested {
        return schemas
            .iter()
            .find(|schema| schema.name == name)
            .ok_or_else(|| SchemaCheckError::SchemaNameNotFound {
                path: path.to_owned(),
                name: name.to_owned(),
                available: schema_names(schemas),
            });
    }

    match schemas {
        [only] => Ok(only),
        many => Err(SchemaCheckError::SchemaNameRequired {
            path: path.to_owned(),
            available: schema_names(many),
        }),
    }
}

fn schema_names(schemas: &[SchemaDef]) -> String {
    schemas
        .iter()
        .map(|schema| schema.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Builds the instance's report, or `None` when the file's parsed tree does
/// not hold exactly one top-level form shaped like a plist or an alist.
///
/// Exactly one top-level form, deliberately: an instance file is one datum,
/// not a sequence of them, so an empty file and a file holding several
/// top-level forms are both refused the same "not a recognizable instance
/// shape" way as a form that parses but is shaped like neither a plist nor
/// an alist — one error for every way the file fails to be a single
/// instance, rather than a special case for each.
#[must_use]
pub fn build_schema_check_report(
    path: &Path,
    tree: &SyntaxTree,
    schema: &SchemaDef,
) -> Option<FileFindings<SchemaIssue>> {
    let root = tree.root_view();
    let instance = match root.children.as_slice() {
        [only] => only,
        _ => return None,
    };
    let issues = validate_instance(schema, instance)?;

    Some(FileFindings::new(
        path.to_path_buf(),
        Dialect::CommonLisp,
        true,
        tree.source(),
        issues,
        Vec::new(),
    ))
}

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, the same choice
/// `data-check`'s `--fail-on-finding` makes: a schema violation is a fact
/// about the instance, not a failure by definition — it is one only in a
/// run that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SchemaIssue>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} finding(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::parse_schemas;

    fn schemas(text: &str) -> Vec<SchemaDef> {
        parse_schemas(text, "sample").expect("parse")
    }

    #[test]
    fn a_single_schema_file_needs_no_name() {
        let schemas = schemas("(defschema only (fields (:x (:type string))))");
        let selected = select_schema(&schemas, None, "s.lisp").expect("select");
        assert_eq!(selected.name, "only");
    }

    #[test]
    fn several_schemas_with_no_name_requested_is_a_disambiguation_error() {
        let schemas = schemas(
            "(defschema a (fields (:x (:type string)))) (defschema b (fields (:y (:type integer))))",
        );
        let error = select_schema(&schemas, None, "s.lisp").expect_err("must refuse");
        assert!(matches!(
            error,
            SchemaCheckError::SchemaNameRequired { available, .. } if available.contains('a') && available.contains('b')
        ));
    }

    #[test]
    fn several_schemas_with_a_matching_name_selects_it() {
        let schemas = schemas(
            "(defschema a (fields (:x (:type string)))) (defschema b (fields (:y (:type integer))))",
        );
        let selected = select_schema(&schemas, Some("b"), "s.lisp").expect("select");
        assert_eq!(selected.name, "b");
    }

    #[test]
    fn an_unknown_requested_name_lists_what_is_available() {
        let schemas = schemas("(defschema a (fields (:x (:type string))))");
        let error = select_schema(&schemas, Some("nope"), "s.lisp").expect_err("must refuse");
        assert!(matches!(
            error,
            SchemaCheckError::SchemaNameNotFound { name, available, .. }
                if name == "nope" && available.contains('a')
        ));
    }

    #[test]
    fn a_schema_file_with_no_schemas_is_refused() {
        let error = select_schema(&[], None, "s.lisp").expect_err("must refuse");
        assert!(matches!(error, SchemaCheckError::NoSchemas { .. }));
    }
}
