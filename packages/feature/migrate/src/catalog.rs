//! The recipes a run can reach: the built-ins, plus a project's own.
//!
//! The built-ins are embedded source text parsed by the same
//! [`parse_recipes`] a user's file goes through, not a table of Rust structs.
//! That is deliberate: a built-in recipe is then a worked example of the
//! format, it cannot express anything a user's file cannot, and a mistake in
//! the format is caught by the same test either way.

use std::path::{Path, PathBuf};

use paredit_core_cli::CliResult;

use crate::error::MigrateError;
use paredit_core_cli::error::{ArgumentError, CliError};
use paredit_core_cli::shared::{MAX_SOURCE_INPUT_BYTES, read_text_file_with_limit};

use crate::recipe::{Migration, parse_recipes};

/// Where a project keeps its own recipes when it does not say otherwise.
///
/// Beside `.paredit/rules`, which is where the custom lint rules live, for
/// the same reason: a project that has taught this tool something keeps the
/// teaching in one directory.
pub const DEFAULT_RECIPE_DIRECTORY: &str = ".paredit/migrations";

/// Each built-in recipe as `(name, source)`.
const BUILT_IN: [(&str, &str); 2] = [
    ("elisp-cl-lib", include_str!("../recipes/elisp-cl-lib.lisp")),
    (
        "nil-conditionals",
        include_str!("../recipes/nil-conditionals.lisp"),
    ),
];

/// One recipe with where it came from, so `migrate list` can say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub migration: Migration,
    /// `built-in`, or the path the recipe was read from.
    pub origin: String,
}

/// Every built-in recipe.
///
/// Panics only if a shipped recipe stops parsing, which
/// `every_built_in_recipe_parses` below turns into a test failure rather than
/// a runtime one.
pub fn built_in() -> CliResult<Vec<CatalogEntry>> {
    let mut entries = Vec::new();
    for (name, source) in BUILT_IN {
        for migration in
            parse_recipes(source, name).map_err(|source| MigrateError::BuiltInMalformed {
                name: name.to_string(),
                source,
            })?
        {
            entries.push(CatalogEntry {
                migration,
                origin: "built-in".to_owned(),
            });
        }
    }
    Ok(entries)
}

/// Every recipe in `directory`, in filename order.
///
/// A directory that does not exist is *not* an error: the default location is
/// a convention, and most projects have no recipes of their own. A directory
/// that exists and holds a file that will not parse *is* an error — a project
/// that wrote a recipe and saw a green run has been told the recipe applied.
pub fn from_directory(directory: &Path) -> CliResult<Vec<CatalogEntry>> {
    let Ok(read) = std::fs::read_dir(directory) else {
        return Ok(Vec::new());
    };
    let mut paths: Vec<PathBuf> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "lisp")
        })
        .collect();
    paths.sort();

    let mut entries = Vec::new();
    for path in paths {
        let origin = path.display().to_string();
        // The same bounded, regular-file-only reader every *source* file goes
        // through — not `fs::read_to_string`. A recipe directory is repository
        // content, so `boom.lisp -> /dev/zero` is a file an attacker can put
        // there, and `read_to_string` on a character device never reaches EOF:
        // it allocates until the process dies. A FIFO blocks forever the same
        // way. `migrate list`, `explain` and `run` all resolve the catalogue,
        // so all three were reachable.
        let text = read_text_file_with_limit(&path, MAX_SOURCE_INPUT_BYTES).map_err(|source| {
            MigrateError::RecipeUnreadable {
                origin: origin.clone(),
                source,
            }
        })?;
        for migration in
            parse_recipes(&text, &origin).map_err(|source| MigrateError::RecipeMalformed {
                origin: origin.clone(),
                source,
            })?
        {
            entries.push(CatalogEntry {
                migration,
                origin: origin.clone(),
            });
        }
    }
    Ok(entries)
}

/// The built-ins plus `directory`'s, with a project's recipe shadowing a
/// built-in of the same name.
///
/// Shadowing rather than refusing: a project that wants `nil-conditionals` to
/// mean something slightly different for its codebase should be able to say
/// so, and the origin column in `migrate list` makes the override visible.
pub fn resolve(directory: &Path) -> CliResult<Vec<CatalogEntry>> {
    let project = from_directory(directory)?;
    let mut entries: Vec<CatalogEntry> = built_in()?
        .into_iter()
        .filter(|entry| {
            !project
                .iter()
                .any(|other| other.migration.name == entry.migration.name)
        })
        .collect();
    entries.extend(project);
    entries.sort_by(|left, right| left.migration.name.cmp(&right.migration.name));
    Ok(entries)
}

/// The one recipe named `name`, or a failure listing what is available.
pub fn find(entries: &[CatalogEntry], name: &str) -> CliResult<CatalogEntry> {
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.migration.name == name)
        .cloned()
    {
        return Ok(entry);
    }
    let available = entries
        .iter()
        .map(|entry| entry.migration.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    // A typed argument error rather than a bare `bail!`: an untyped one
    // classifies as `internal.unclassified`, whose documented meaning is "a
    // defect in this tool, please report it" — which sends someone to file a
    // bug about their own typo.
    Err(CliError::from(ArgumentError::UnknownName {
        what: "migration",
        name: name.to_owned(),
        available: if available.is_empty() {
            "none".to_owned()
        } else {
            available
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::selector::{Pattern, Template};

    /// Every shipped recipe has to parse, and every step of it has to be a
    /// pattern and a template that agree — a shipped recipe whose template
    /// names an unbound capture would fail only for whoever ran it.
    #[test]
    fn every_built_in_recipe_parses_and_every_step_type_checks() {
        let entries = built_in().expect("built-in recipes parse");
        assert!(!entries.is_empty());
        for entry in entries {
            let dialect = entry
                .migration
                .dialects
                .first()
                .copied()
                .unwrap_or(Dialect::CommonLisp);
            for (index, step) in entry.migration.steps.iter().enumerate() {
                let pattern = Pattern::parse(&step.query, dialect).unwrap_or_else(|error| {
                    panic!("{} step {index} query: {error}", entry.migration.name)
                });
                let template = Template::parse(&step.rewrite, dialect).unwrap_or_else(|error| {
                    panic!("{} step {index} rewrite: {error}", entry.migration.name)
                });
                template.check_against(&pattern).unwrap_or_else(|error| {
                    panic!("{} step {index}: {error}", entry.migration.name)
                });
            }
        }
    }

    #[test]
    fn the_built_in_names_are_unique() {
        let entries = built_in().expect("built-in recipes parse");
        let mut names: Vec<&str> = entries
            .iter()
            .map(|entry| entry.migration.name.as_str())
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two built-in recipes share a name");
    }

    #[test]
    fn a_recipe_directory_that_does_not_exist_contributes_nothing() {
        let entries = from_directory(Path::new("does/not/exist")).expect("absent is not an error");
        assert!(entries.is_empty());
    }

    #[test]
    fn an_unknown_name_lists_what_is_available() {
        let entries = built_in().expect("built-in recipes parse");
        let error = find(&entries, "no-such-recipe").expect_err("must refuse");
        assert!(error.to_string().contains("nil-conditionals"));
    }

    /// The cl-lib recipe is scoped to Emacs Lisp, and that scope is the only
    /// thing standing between it and rewriting `(incf x)` in a Common Lisp
    /// file, where `incf` is the correct spelling and `cl-incf` does not
    /// exist.
    #[test]
    fn the_cl_lib_recipe_refuses_common_lisp() {
        let entries = built_in().expect("built-in recipes parse");
        let entry = find(&entries, "elisp-cl-lib").expect("shipped");
        assert!(entry.migration.covers(Dialect::EmacsLisp));
        assert!(!entry.migration.covers(Dialect::CommonLisp));
    }
}
