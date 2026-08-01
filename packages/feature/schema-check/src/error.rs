//! Why a schema file or an instance file could not be checked.
//!
//! Divided by whose fault it is, the same discipline `migrate`'s
//! `MigrateError` uses: a schema file's own shape is the caller's to fix
//! (`SchemaFileMalformed`, `InstanceShapeUnrecognized`, ...), while an
//! unreadable path is whatever `paredit_core_cli::CliError` already says it
//! is (a permission error, a missing file, a device this tool refuses to
//! read from).

use thiserror::Error;

use paredit_core_cli::diagnosis::ErrorCode;

use crate::dsl::DslError;

/// A schema file or an instance file could not be used.
#[derive(Debug, Error)]
pub enum SchemaCheckError {
    #[error("reading schema file {path}")]
    SchemaFileUnreadable {
        path: String,
        #[source]
        source: paredit_core_cli::CliError,
    },

    #[error("reading schema file {path}")]
    SchemaFileMalformed {
        path: String,
        #[source]
        source: DslError,
    },

    #[error("schema file {path} defines no (defschema ...) forms")]
    NoSchemas { path: String },

    #[error(
        "schema file {path} defines more than one schema ({available}); pass --schema-name to choose one"
    )]
    SchemaNameRequired { path: String, available: String },

    #[error("schema file {path} has no schema named {name:?}; available: {available}")]
    SchemaNameNotFound {
        path: String,
        name: String,
        available: String,
    },

    #[error("reading instance file {path}")]
    InstanceFileUnreadable {
        path: String,
        #[source]
        source: paredit_core_cli::CliError,
    },

    #[error("instance file {path} does not parse as Lisp: {reason}")]
    InstanceFileUnparsable { path: String, reason: String },

    #[error(
        "{path} is not shaped like an alist or a plist; schema check validates only alist- or plist-shaped data"
    )]
    InstanceShapeUnrecognized { path: String },
}

paredit_core_cli::impl_classified_refusal!(SchemaCheckError, |error| match error {
    // Their file: unreadable for an ordinary I/O reason (permissions, a
    // missing path, a device this tool refuses to read).
    SchemaCheckError::SchemaFileUnreadable { source, .. } =>
        paredit_core_cli::diagnosis::code_for_cli_error(source),
    // The file was read; it does not parse as `defschema` forms.
    SchemaCheckError::SchemaFileMalformed { .. } => ErrorCode::InputUnparsable,
    SchemaCheckError::NoSchemas { .. } => ErrorCode::InputUnparsable,
    // A name the caller has to supply or correct, the same code
    // `migrate`/`fix` use for an unknown recipe or rule name.
    SchemaCheckError::SchemaNameRequired { .. } => ErrorCode::ArgumentUnknownName,
    SchemaCheckError::SchemaNameNotFound { .. } => ErrorCode::ArgumentUnknownName,
    SchemaCheckError::InstanceFileUnreadable { source, .. } =>
        paredit_core_cli::diagnosis::code_for_cli_error(source),
    SchemaCheckError::InstanceFileUnparsable { .. } => ErrorCode::InputUnparsable,
    // Read and parsed as Lisp, but not shaped like data this command
    // understands — the caller's cue to select a different file, the same
    // meaning `input.shape-refused` already carries elsewhere.
    SchemaCheckError::InstanceShapeUnrecognized { .. } => ErrorCode::InputShapeRefused,
});

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_cli::CliError;
    use paredit_core_cli::diagnosis::{ErrorCode, code_for_cli_error};

    #[test]
    fn every_variant_classifies_to_a_documented_code() {
        let cases: Vec<(SchemaCheckError, ErrorCode)> = vec![
            (
                SchemaCheckError::SchemaFileMalformed {
                    path: "s.lisp".to_owned(),
                    source: DslError::MissingName,
                },
                ErrorCode::InputUnparsable,
            ),
            (
                SchemaCheckError::NoSchemas {
                    path: "s.lisp".to_owned(),
                },
                ErrorCode::InputUnparsable,
            ),
            (
                SchemaCheckError::SchemaNameRequired {
                    path: "s.lisp".to_owned(),
                    available: "a, b".to_owned(),
                },
                ErrorCode::ArgumentUnknownName,
            ),
            (
                SchemaCheckError::SchemaNameNotFound {
                    path: "s.lisp".to_owned(),
                    name: "c".to_owned(),
                    available: "a, b".to_owned(),
                },
                ErrorCode::ArgumentUnknownName,
            ),
            (
                SchemaCheckError::InstanceFileUnparsable {
                    path: "i.lisp".to_owned(),
                    reason: "boom".to_owned(),
                },
                ErrorCode::InputUnparsable,
            ),
            (
                SchemaCheckError::InstanceShapeUnrecognized {
                    path: "i.lisp".to_owned(),
                },
                ErrorCode::InputShapeRefused,
            ),
        ];
        for (error, expected) in cases {
            let failure = CliError::from(error);
            assert_eq!(code_for_cli_error(&failure), expected, "{failure}");
        }
    }
}
