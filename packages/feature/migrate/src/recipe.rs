//! The recipe format, read from Lisp.
//!
//! A migration is an ordered list of `--query`/`--rewrite` pairs with a
//! dialect scope, written as a Lisp form:
//!
//! ```lisp
//! (defmigration nil-conditionals
//!   :description "one-armed `if` to `when` and `unless`"
//!   :dialects (common-lisp emacs-lisp)
//!   :steps ((:query (if (not ?test) ?then nil)
//!            :rewrite (unless ?test ?then)
//!            :note "the negated arm first, so the general rule cannot claim it")
//!           (:query (if ?test ?then nil)
//!            :rewrite (when ?test ?then))))
//! ```
//!
//! Lisp rather than TOML for two reasons. The custom lint rule format
//! (`.paredit/rules/*.lisp`) already made this choice and gave the reason — a
//! project writing Lisp should not learn a second syntax to say something
//! about its own Lisp. And the queries and rewrites *are* S-expressions:
//! writing them as TOML strings would mean escaping quotes inside a language
//! whose whole point is that it does not need them.
//!
//! # `:dialects` is a guard, not a hint
//!
//! `elisp-cl-lib` rewrites `(incf x)` to `(cl-incf x)`. Run over Common Lisp
//! that is not a modernization, it is breakage: `incf` *is* the Common Lisp
//! spelling. So a recipe that names dialects skips every file outside them,
//! and reports how many it skipped. A recipe naming none applies everywhere,
//! which is only correct for rewrites that are dialect-neutral.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};
use thiserror::Error;

/// What can be wrong with a recipe file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecipeError {
    #[error("recipe file {path} does not parse as Lisp: {reason}")]
    Unreadable { path: String, reason: String },

    #[error("{path} holds a top-level {found}; a recipe file holds only (defmigration ...) forms")]
    NotAMigration { path: String, found: String },

    #[error("(defmigration ...) needs a name as its first argument")]
    MissingName,

    #[error("migration {name:?} is missing the required option {option}")]
    MissingOption { name: String, option: &'static str },

    #[error("migration {name:?} names the unknown dialect {dialect:?}; valid values: {valid}")]
    UnknownDialect {
        name: String,
        dialect: String,
        valid: String,
    },

    #[error("migration {name:?} step {index} is missing {option}")]
    StepMissingOption {
        name: String,
        index: usize,
        option: &'static str,
    },

    #[error("migration {name:?} declares no steps; a migration that rewrites nothing is a typo")]
    NoSteps { name: String },

    #[error("two migrations are named {name:?}; a recipe name has to identify one recipe")]
    DuplicateName { name: String },
}

/// One `--query`/`--rewrite` pair, as source text.
///
/// Kept as text rather than as a parsed `Pattern` and `Template` because a
/// step's pattern is parsed with the *dialect of the run*, which is not known
/// until the recipe is matched against a file set. Parsing here would fix the
/// reader at load time and make `migrate list` fail on a Clojure recipe that
/// nobody asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub query: String,
    pub rewrite: String,
    /// Why this step is here, printed by `migrate explain`.
    pub note: Option<String>,
}

/// One named migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub name: String,
    pub description: String,
    /// The dialects this recipe is correct for. Empty means every dialect.
    pub dialects: Vec<Dialect>,
    pub steps: Vec<Step>,
}

impl Migration {
    /// Whether this recipe may be applied to a file of `dialect`.
    #[must_use]
    pub fn covers(&self, dialect: Dialect) -> bool {
        self.dialects.is_empty() || self.dialects.contains(&dialect)
    }

    /// The `:dialects` list as text, or `any` when the recipe named none.
    #[must_use]
    pub fn dialect_labels(&self) -> Vec<&'static str> {
        if self.dialects.is_empty() {
            return vec!["any"];
        }
        self.dialects
            .iter()
            .map(|dialect| dialect.label())
            .collect()
    }
}

/// Reads every `(defmigration ...)` in one file's text.
///
/// `path` names the file only for the diagnostics; nothing is read from disk
/// here, so a built-in recipe and a user's file go through one code path and
/// fail the same way.
pub fn parse_recipes(text: &str, path: &str) -> Result<Vec<Migration>, RecipeError> {
    // Common Lisp's reader, because a recipe file is data rather than a
    // program: it is never loaded by an implementation, and the CL reader is
    // the most permissive of the ten for the tokens a recipe writes.
    let tree = SyntaxTree::parse_with_dialect(text, Dialect::CommonLisp).map_err(|error| {
        RecipeError::Unreadable {
            path: path.to_owned(),
            reason: error.to_string(),
        }
    })?;

    let mut migrations = Vec::new();
    for form in &tree.root_view().children {
        if list_head(form) != Some("defmigration") {
            return Err(RecipeError::NotAMigration {
                path: path.to_owned(),
                found: describe(form),
            });
        }
        migrations.push(read_migration(form, tree.source())?);
    }

    for (index, migration) in migrations.iter().enumerate() {
        if migrations[..index]
            .iter()
            .any(|earlier| earlier.name == migration.name)
        {
            return Err(RecipeError::DuplicateName {
                name: migration.name.clone(),
            });
        }
    }
    Ok(migrations)
}

fn describe(view: &ExpressionView) -> String {
    match view.kind {
        ExpressionKind::Atom => atom_text(view).unwrap_or("atom").to_owned(),
        _ => list_head(view).map_or_else(|| "list".to_owned(), |head| format!("({head} ...)")),
    }
}

fn read_migration(form: &ExpressionView, source: &str) -> Result<Migration, RecipeError> {
    let name = form
        .children
        .get(1)
        .and_then(atom_text)
        .ok_or(RecipeError::MissingName)?
        .to_owned();

    let description = keyword_value(form, ":description")
        .and_then(|view| string_text(view, source))
        .ok_or_else(|| RecipeError::MissingOption {
            name: name.clone(),
            option: ":description",
        })?;

    let dialects = match keyword_value(form, ":dialects") {
        Some(view) => view
            .children
            .iter()
            .map(|child| {
                let label = atom_text(child).unwrap_or_default();
                label
                    .parse::<Dialect>()
                    .ok()
                    .filter(|dialect| *dialect != Dialect::Unknown)
                    .ok_or_else(|| RecipeError::UnknownDialect {
                        name: name.clone(),
                        dialect: label.to_owned(),
                        valid: Dialect::ALL
                            .iter()
                            .map(|dialect| dialect.label())
                            .collect::<Vec<_>>()
                            .join(", "),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let steps_view = keyword_value(form, ":steps").ok_or_else(|| RecipeError::MissingOption {
        name: name.clone(),
        option: ":steps",
    })?;
    let steps = steps_view
        .children
        .iter()
        .enumerate()
        .map(|(index, step)| read_step(&name, index, step, source))
        .collect::<Result<Vec<_>, _>>()?;
    if steps.is_empty() {
        return Err(RecipeError::NoSteps { name });
    }

    Ok(Migration {
        name,
        description,
        dialects,
        steps,
    })
}

fn read_step(
    name: &str,
    index: usize,
    step: &ExpressionView,
    source: &str,
) -> Result<Step, RecipeError> {
    let text_of = |option: &'static str| -> Result<String, RecipeError> {
        keyword_value(step, option)
            .map(|view| span_text(view, source))
            .ok_or_else(|| RecipeError::StepMissingOption {
                name: name.to_owned(),
                index,
                option,
            })
    };
    Ok(Step {
        query: text_of(":query")?,
        rewrite: text_of(":rewrite")?,
        note: keyword_value(step, ":note").and_then(|view| string_text(view, source)),
    })
}

/// The form following `keyword` in a plist-shaped list.
fn keyword_value<'a>(form: &'a ExpressionView, keyword: &str) -> Option<&'a ExpressionView> {
    form.children
        .iter()
        .position(|child| atom_text(child) == Some(keyword))
        .and_then(|index| form.children.get(index + 1))
}

/// A form's source text, verbatim — which is what a query or a rewrite is.
fn span_text(view: &ExpressionView, source: &str) -> String {
    source[view.span.start().get()..view.span.end().get()].to_owned()
}

/// A `"…"` literal's contents, with the reader's escapes resolved.
fn string_text(view: &ExpressionView, source: &str) -> Option<String> {
    let raw = span_text(view, source);
    let body = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut escaped = false;
    for character in body.chars() {
        match (escaped, character) {
            (true, other) => {
                out.push(other);
                escaped = false;
            }
            (false, '\\') => escaped = true,
            (false, other) => out.push(other),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
(defmigration nil-conditionals
  :description "one-armed if"
  :dialects (common-lisp emacs-lisp)
  :steps ((:query (if (not ?t) ?a nil) :rewrite (unless ?t ?a) :note "negated first")
          (:query (if ?t ?a nil) :rewrite (when ?t ?a))))
"#;

    #[test]
    fn reads_a_recipe_with_its_steps_in_written_order() {
        let recipes = parse_recipes(SAMPLE, "sample").expect("parse");
        assert_eq!(recipes.len(), 1);
        let recipe = &recipes[0];
        assert_eq!(recipe.name, "nil-conditionals");
        assert_eq!(recipe.steps.len(), 2);
        assert_eq!(recipe.steps[0].query, "(if (not ?t) ?a nil)");
        assert_eq!(recipe.steps[0].rewrite, "(unless ?t ?a)");
        assert_eq!(recipe.steps[0].note.as_deref(), Some("negated first"));
        assert_eq!(recipe.steps[1].note, None);
    }

    #[test]
    fn a_dialect_scope_admits_only_the_dialects_it_names() {
        let recipe = &parse_recipes(SAMPLE, "sample").expect("parse")[0];
        assert!(recipe.covers(Dialect::CommonLisp));
        assert!(recipe.covers(Dialect::EmacsLisp));
        assert!(!recipe.covers(Dialect::Clojure));
    }

    #[test]
    fn a_recipe_naming_no_dialect_covers_every_dialect() {
        let text = r#"(defmigration any :description "d" :steps ((:query (a) :rewrite (b))))"#;
        let recipe = &parse_recipes(text, "sample").expect("parse")[0];
        assert!(recipe.covers(Dialect::Clojure));
        assert_eq!(recipe.dialect_labels(), vec!["any"]);
    }

    #[test]
    fn a_query_keeps_its_reader_prefixes_verbatim() {
        let text =
            r#"(defmigration p :description "d" :steps ((:query (f #'?g) :rewrite (h #'?g))))"#;
        let recipe = &parse_recipes(text, "sample").expect("parse")[0];
        assert_eq!(recipe.steps[0].query, "(f #'?g)");
    }

    #[test]
    fn a_missing_description_names_the_option_and_the_migration() {
        let text = "(defmigration p :steps ((:query (a) :rewrite (b))))";
        let error = parse_recipes(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            RecipeError::MissingOption {
                option: ":description",
                ..
            }
        ));
    }

    #[test]
    fn a_migration_with_no_steps_is_refused_rather_than_run_as_a_no_op() {
        let text = r#"(defmigration p :description "d" :steps ())"#;
        let error = parse_recipes(text, "sample").expect_err("must refuse");
        assert!(matches!(error, RecipeError::NoSteps { .. }));
    }

    #[test]
    fn an_unknown_dialect_is_refused_and_lists_the_known_ones() {
        let text = r#"(defmigration p :description "d" :dialects (perl) :steps ((:query (a) :rewrite (b))))"#;
        let error = parse_recipes(text, "sample").expect_err("must refuse");
        assert!(matches!(error, RecipeError::UnknownDialect { .. }));
    }

    #[test]
    fn two_recipes_of_one_name_are_refused() {
        let text = r#"
(defmigration p :description "d" :steps ((:query (a) :rewrite (b))))
(defmigration p :description "e" :steps ((:query (c) :rewrite (d))))
"#;
        let error = parse_recipes(text, "sample").expect_err("must refuse");
        assert!(matches!(error, RecipeError::DuplicateName { .. }));
    }

    #[test]
    fn a_top_level_form_that_is_not_a_migration_is_refused() {
        let error = parse_recipes("(defun foo ())", "sample").expect_err("must refuse");
        assert!(matches!(error, RecipeError::NotAMigration { .. }));
    }

    #[test]
    fn a_description_resolves_the_readers_escapes() {
        let text = r#"(defmigration p :description "a \"quoted\" word" :steps ((:query (a) :rewrite (b))))"#;
        let recipe = &parse_recipes(text, "sample").expect("parse")[0];
        assert_eq!(recipe.description, "a \"quoted\" word");
    }
}
