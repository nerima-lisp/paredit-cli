//! Why a migration recipe could not be read or applied.
//!
//! Section 9.2. A migration is a *recipe* — a named list of query/rewrite
//! steps, either built into this binary or read from a directory — so the
//! failures divide by whose fault they are, which is what a caller needs:
//!
//! - a built-in recipe that does not parse is a defect in this tool;
//! - a recipe file on disk that does not parse is the caller's file;
//! - a step whose `:query` or `:rewrite` is malformed names the step, because
//!   a recipe has many and "which one" is the first question.

use thiserror::Error;

use paredit_core_cli::diagnosis::ErrorCode;

use crate::recipe::RecipeError;

/// A recipe, or a step inside one, could not be used.
#[derive(Debug, Error)]
pub enum MigrateError {
    /// A recipe compiled into this binary does not parse.
    #[error("built-in recipe {name} is malformed")]
    BuiltInMalformed {
        name: String,
        #[source]
        source: RecipeError,
    },

    /// A recipe file on disk could not be read.
    #[error("reading migration recipe {origin}")]
    RecipeUnreadable {
        origin: String,
        #[source]
        source: paredit_core_cli::CliError,
    },

    /// A recipe file on disk does not parse as recipes.
    #[error("reading migration recipe {origin}")]
    RecipeMalformed {
        origin: String,
        #[source]
        source: RecipeError,
    },

    /// A step rewrote a file into something that no longer parses.
    #[error("migration {migration} step {index} left {path} unparseable")]
    StepLeftFileUnparseable {
        migration: String,
        index: usize,
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// One step of a recipe is malformed. `part` names which half — `:query`,
    /// `:rewrite`, or the step as a whole.
    #[error("migration {migration} step {index}{part}")]
    Step {
        migration: String,
        index: usize,
        /// Carries its own leading `": "`, or is empty for the whole step, so
        /// the three messages this replaced are reproduced exactly.
        part: String,
        #[source]
        source: Box<paredit_core_cli::CliError>,
    },
}

impl MigrateError {
    /// Names a step failure, with the `part` suffix already formatted.
    pub fn step<E: Into<paredit_core_cli::CliError>>(
        migration: &str,
        index: usize,
        part: &'static str,
    ) -> impl FnOnce(E) -> Self + use<E> {
        let migration = migration.to_owned();
        move |source| Self::Step {
            migration,
            index,
            part: if part.is_empty() {
                String::new()
            } else {
                format!(": {part}")
            },
            source: Box::new(source.into()),
        }
    }
}

paredit_core_cli::impl_classified_refusal!(MigrateError, |error| match error {
    // A recipe this binary ships with is our defect, not the caller's.
    MigrateError::BuiltInMalformed { .. } => ErrorCode::Internal,
    // Their file: unreadable, or unparseable as a recipe.
    MigrateError::RecipeUnreadable { source, .. } =>
        paredit_core_cli::diagnosis::code_for_cli_error(source),
    // The file was read; it is not a recipe.
    MigrateError::RecipeMalformed { .. } => ErrorCode::InputUnparsable,
    // A `:query` or `:rewrite` the pattern language rejects — a recipe the
    // caller can edit, not a defect.
    MigrateError::Step { .. } => ErrorCode::SelectionPatternInvalid,
    // Same code as the CLI's own write guard: this tool does not leave
    // behind a file it cannot read back.
    MigrateError::StepLeftFileUnparseable { .. } => ErrorCode::RefusalRewriteDoesNotReparse,
});
