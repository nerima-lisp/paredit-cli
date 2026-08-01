//! The `defschema` grammar, read from Lisp.
//!
//! A schema file holds one or more `(defschema ...)` forms:
//!
//! ```lisp
//! (defschema config
//!   (fields
//!     (:name (:type string))
//!     (:port (:type integer :min 1 :max 65535))
//!     (:mode (:type string :one-of ("dev" "staging" "prod")))
//!     (:label (:type string :matches "^[a-z][a-z0-9-]*$" :optional t))))
//! ```
//!
//! # Why Lisp, and why a closed grammar
//!
//! Lisp for the same reason the custom lint rule format and the migration
//! recipe format made the same choice: a project already writing Lisp should
//! not learn a second syntax to describe its own data.
//!
//! The grammar is deliberately closed rather than extensible. `:type` accepts
//! exactly five names (`string`, `integer`, `boolean`, `symbol`, `list`), and
//! a refinement is one of exactly four (`:min`, `:max`, `:one-of`,
//! `:matches`), each meaningful only for the type it was written for
//! (`:min`/`:max` on `integer`, `:one-of`/`:matches` on `string`). There are
//! no predicates, no lambdas, and no evaluated expressions anywhere in this
//! grammar: an unrecognized `:type` or refinement keyword is rejected by
//! name, with a [`DslError`], rather than treated as code to interpret. This
//! tool never evaluates what it inspects, and a schema language that *could*
//! run arbitrary code would break that invariant at exactly the point a
//! caller is most likely to hand it something untrusted (see
//! `docs/src/guide/safety.md`).
//!
//! `:matches`'s pattern is a small hand-rolled glob (`*` = any run of
//! characters, `?` = one character, everything else literal), not a regular
//! expression — seeing this crate's `Cargo.toml` list no regex engine is
//! deliberate, not an oversight.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head};
use thiserror::Error;

/// What can be wrong with a schema file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DslError {
    #[error("schema file {path} does not parse as Lisp: {reason}")]
    Unreadable { path: String, reason: String },

    #[error("{path} holds a top-level {found}; a schema file holds only (defschema ...) forms")]
    NotADefschema { path: String, found: String },

    #[error("(defschema ...) needs a name as its first argument")]
    MissingName,

    #[error("schema {schema:?} needs a (fields ...) form as its second argument")]
    MissingFields { schema: String },

    #[error("schema {schema:?} has a field with no :<keyword> key")]
    FieldMissingKey { schema: String },

    #[error("schema {schema:?} field {found:?} is not a keyword; field keys are spelled :name")]
    FieldKeyNotAKeyword { schema: String, found: String },

    #[error(
        "schema {schema:?} field {key:?} needs a (:type ...) specification as its second element"
    )]
    FieldMissingSpec { schema: String, key: String },

    #[error(
        "schema {schema:?} field {key:?}'s specification is malformed: expected (:type <type-name> [refinements...])"
    )]
    FieldSpecMalformed { schema: String, key: String },

    #[error("schema {schema:?} field {key:?} is missing the required :type")]
    FieldMissingType { schema: String, key: String },

    #[error(
        "schema {schema:?} field {key:?} names the unknown type {type_name:?}; valid types: string, integer, boolean, symbol, list"
    )]
    UnknownType {
        schema: String,
        key: String,
        type_name: String,
    },

    #[error("schema {schema:?} field {key:?} declares the unrecognized refinement :{refinement}")]
    UnknownRefinement {
        schema: String,
        key: String,
        refinement: String,
    },

    #[error("schema {schema:?} field {key:?} declares :{keyword} more than once")]
    DuplicateSpecKeyword {
        schema: String,
        key: String,
        keyword: &'static str,
    },

    #[error(
        "schema {schema:?} field {key:?} declares :{refinement} on a {field_type} field; :{refinement} only applies to a {expected} field"
    )]
    RefinementWrongType {
        schema: String,
        key: String,
        refinement: &'static str,
        field_type: &'static str,
        expected: &'static str,
    },

    #[error("schema {schema:?} field {key:?}'s :{refinement} value is malformed: {reason}")]
    RefinementValueInvalid {
        schema: String,
        key: String,
        refinement: &'static str,
        reason: String,
    },

    #[error(
        "two schemas in one file are both named {name:?}; a schema name has to identify one schema"
    )]
    DuplicateName { name: String },
}

/// The closed set of field types `defschema` recognizes.
///
/// Closed deliberately: this phase does not support nested schemas or union
/// types, so a `:type` naming anything outside this set is a mistake to
/// report, not a feature to guess at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    String,
    Integer,
    Boolean,
    Symbol,
    List,
}

impl FieldType {
    pub const ALL: [Self; 5] = [
        Self::String,
        Self::Integer,
        Self::Boolean,
        Self::Symbol,
        Self::List,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Symbol => "symbol",
            Self::List => "list",
        }
    }

    /// The type named by `name`, or `None` for anything outside the closed
    /// vocabulary.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.label() == name)
    }
}

/// One field's declaration: its type and the refinements that narrow it.
///
/// Refinements are typed fields rather than a generic list, on purpose: it
/// makes "does this refinement apply to this field's type" a fact the parser
/// checks once, here, rather than something every reader of a `Vec<Refinement>`
/// has to re-derive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    /// The field's name, with the declaring keyword's leading `:` already
    /// stripped — `:name` becomes `"name"` — so it compares equal to both an
    /// alist's bare symbol key and a plist's keyword key without either side
    /// having to know which shape the other came from.
    pub key: String,
    pub field_type: FieldType,
    /// `:min`, meaningful only for [`FieldType::Integer`].
    pub min: Option<i64>,
    /// `:max`, meaningful only for [`FieldType::Integer`].
    pub max: Option<i64>,
    /// `:one-of`, meaningful only for [`FieldType::String`].
    pub one_of: Option<Vec<String>>,
    /// `:matches`, meaningful only for [`FieldType::String`].
    pub matches: Option<String>,
    /// `:optional t`. A field with no `:optional` is required.
    pub optional: bool,
}

/// One named schema: `(defschema <name> (fields ...))`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDef {
    pub name: String,
    pub fields: Vec<FieldSpec>,
}

/// Reads every `(defschema ...)` in one file's text.
///
/// `path` names the file only for the diagnostics; nothing is read from disk
/// here, matching [`paredit_feature_migrate::recipe::parse_recipes`]'s own
/// contract, so a built-in schema and a user's file would go through one code
/// path and fail the same way.
pub fn parse_schemas(text: &str, path: &str) -> Result<Vec<SchemaDef>, DslError> {
    // Common Lisp's reader, the same choice the custom lint rule format and
    // the migration recipe format both make: a schema file is data rather
    // than a program, never loaded by any Lisp implementation, and the CL
    // reader is the most permissive of the ten for the tokens a schema
    // writes.
    let tree = SyntaxTree::parse_with_dialect(text, Dialect::CommonLisp).map_err(|error| {
        DslError::Unreadable {
            path: path.to_owned(),
            reason: error.to_string(),
        }
    })?;

    let mut schemas = Vec::new();
    for form in &tree.root_view().children {
        if list_head(form) != Some("defschema") {
            return Err(DslError::NotADefschema {
                path: path.to_owned(),
                found: describe(form),
            });
        }
        schemas.push(read_schema(form)?);
    }

    for (index, schema) in schemas.iter().enumerate() {
        if schemas[..index]
            .iter()
            .any(|earlier| earlier.name == schema.name)
        {
            return Err(DslError::DuplicateName {
                name: schema.name.clone(),
            });
        }
    }
    Ok(schemas)
}

fn describe(view: &ExpressionView) -> String {
    match view.kind {
        ExpressionKind::Atom => atom_text(view).unwrap_or("atom").to_owned(),
        _ => list_head(view).map_or_else(|| "list".to_owned(), |head| format!("({head} ...)")),
    }
}

fn read_schema(form: &ExpressionView) -> Result<SchemaDef, DslError> {
    let name = form
        .children
        .get(1)
        .and_then(atom_text)
        .ok_or(DslError::MissingName)?
        .to_owned();

    let fields_form = form
        .children
        .get(2)
        .filter(|view| list_head(view) == Some("fields"))
        .ok_or_else(|| DslError::MissingFields {
            schema: name.clone(),
        })?;

    let fields = fields_form
        .children
        .iter()
        .skip(1)
        .map(|entry| read_field(&name, entry))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SchemaDef { name, fields })
}

const OPTIONAL_KEYWORD: &str = ":optional";

fn read_field(schema: &str, entry: &ExpressionView) -> Result<FieldSpec, DslError> {
    let key_text =
        entry
            .children
            .first()
            .and_then(atom_text)
            .ok_or_else(|| DslError::FieldMissingKey {
                schema: schema.to_owned(),
            })?;
    let key = key_text
        .strip_prefix(':')
        .ok_or_else(|| DslError::FieldKeyNotAKeyword {
            schema: schema.to_owned(),
            found: key_text.to_owned(),
        })?
        .to_owned();

    if entry.children.len() != 2 {
        return Err(DslError::FieldMissingSpec {
            schema: schema.to_owned(),
            key,
        });
    }
    let spec = &entry.children[1];
    if !is_paren_list(spec) || spec.children.is_empty() || spec.children.len() % 2 != 0 {
        return Err(DslError::FieldSpecMalformed {
            schema: schema.to_owned(),
            key,
        });
    }

    let mut field_type = None;
    let mut min = None;
    let mut max = None;
    let mut one_of = None;
    let mut matches = None;
    let mut optional = false;

    for pair in spec.children.chunks_exact(2) {
        let keyword = atom_text(&pair[0]).ok_or_else(|| DslError::FieldSpecMalformed {
            schema: schema.to_owned(),
            key: key.clone(),
        })?;
        let value = &pair[1];
        match keyword {
            ":type" => {
                if field_type.is_some() {
                    return Err(DslError::DuplicateSpecKeyword {
                        schema: schema.to_owned(),
                        key: key.clone(),
                        keyword: "type",
                    });
                }
                let type_name = atom_text(value).ok_or_else(|| DslError::FieldSpecMalformed {
                    schema: schema.to_owned(),
                    key: key.clone(),
                })?;
                field_type =
                    Some(
                        FieldType::parse(type_name).ok_or_else(|| DslError::UnknownType {
                            schema: schema.to_owned(),
                            key: key.clone(),
                            type_name: type_name.to_owned(),
                        })?,
                    );
            }
            ":min" if min.is_some() => {
                return Err(DslError::DuplicateSpecKeyword {
                    schema: schema.to_owned(),
                    key: key.clone(),
                    keyword: "min",
                });
            }
            ":min" => min = Some(read_integer(schema, &key, "min", value)?),
            ":max" if max.is_some() => {
                return Err(DslError::DuplicateSpecKeyword {
                    schema: schema.to_owned(),
                    key: key.clone(),
                    keyword: "max",
                });
            }
            ":max" => max = Some(read_integer(schema, &key, "max", value)?),
            ":one-of" if one_of.is_some() => {
                return Err(DslError::DuplicateSpecKeyword {
                    schema: schema.to_owned(),
                    key: key.clone(),
                    keyword: "one-of",
                });
            }
            ":one-of" => one_of = Some(read_string_list(schema, &key, "one-of", value)?),
            ":matches" if matches.is_some() => {
                return Err(DslError::DuplicateSpecKeyword {
                    schema: schema.to_owned(),
                    key: key.clone(),
                    keyword: "matches",
                });
            }
            ":matches" => matches = Some(read_string(schema, &key, "matches", value)?),
            OPTIONAL_KEYWORD => optional = read_bool(schema, &key, "optional", value)?,
            other if !other.starts_with(':') => {
                return Err(DslError::FieldSpecMalformed {
                    schema: schema.to_owned(),
                    key: key.clone(),
                });
            }
            other => {
                return Err(DslError::UnknownRefinement {
                    schema: schema.to_owned(),
                    key: key.clone(),
                    refinement: other.trim_start_matches(':').to_owned(),
                });
            }
        }
    }

    let field_type = field_type.ok_or_else(|| DslError::FieldMissingType {
        schema: schema.to_owned(),
        key: key.clone(),
    })?;

    // Refinement/type compatibility is checked once here, after the whole
    // specification has been read, so it holds regardless of what order the
    // caller wrote `:type` and its refinements in.
    if (min.is_some() || max.is_some()) && field_type != FieldType::Integer {
        return Err(DslError::RefinementWrongType {
            schema: schema.to_owned(),
            key,
            refinement: if min.is_some() { "min" } else { "max" },
            field_type: field_type.label(),
            expected: FieldType::Integer.label(),
        });
    }
    if (one_of.is_some() || matches.is_some()) && field_type != FieldType::String {
        return Err(DslError::RefinementWrongType {
            schema: schema.to_owned(),
            key,
            refinement: if one_of.is_some() {
                "one-of"
            } else {
                "matches"
            },
            field_type: field_type.label(),
            expected: FieldType::String.label(),
        });
    }

    Ok(FieldSpec {
        key,
        field_type,
        min,
        max,
        one_of,
        matches,
        optional,
    })
}

fn read_integer(
    schema: &str,
    key: &str,
    refinement: &'static str,
    value: &ExpressionView,
) -> Result<i64, DslError> {
    atom_text(value)
        .and_then(|text| text.parse::<i64>().ok())
        .ok_or_else(|| DslError::RefinementValueInvalid {
            schema: schema.to_owned(),
            key: key.to_owned(),
            refinement,
            reason: "expected an integer literal".to_owned(),
        })
}

fn read_string(
    schema: &str,
    key: &str,
    refinement: &'static str,
    value: &ExpressionView,
) -> Result<String, DslError> {
    atom_text(value)
        .and_then(string_literal_contents)
        .ok_or_else(|| DslError::RefinementValueInvalid {
            schema: schema.to_owned(),
            key: key.to_owned(),
            refinement,
            reason: "expected a string literal".to_owned(),
        })
}

fn read_string_list(
    schema: &str,
    key: &str,
    refinement: &'static str,
    value: &ExpressionView,
) -> Result<Vec<String>, DslError> {
    let malformed = || DslError::RefinementValueInvalid {
        schema: schema.to_owned(),
        key: key.to_owned(),
        refinement,
        reason: "expected a list of string literals".to_owned(),
    };
    if !is_paren_list(value) {
        return Err(malformed());
    }
    value
        .children
        .iter()
        .map(|child| {
            atom_text(child)
                .and_then(string_literal_contents)
                .ok_or_else(malformed)
        })
        .collect()
}

fn read_bool(
    schema: &str,
    key: &str,
    refinement: &'static str,
    value: &ExpressionView,
) -> Result<bool, DslError> {
    match atom_text(value) {
        Some(text) if text.eq_ignore_ascii_case("t") => Ok(true),
        Some(text) if text.eq_ignore_ascii_case("nil") => Ok(false),
        _ => Err(DslError::RefinementValueInvalid {
            schema: schema.to_owned(),
            key: key.to_owned(),
            refinement,
            reason: "expected t or nil".to_owned(),
        }),
    }
}

/// A `"…"` literal's contents, with the reader's backslash escapes resolved.
///
/// `atom_text` already returns one token's exact source slice, quotes
/// included, so — unlike a multi-token form's source span — no whitespace
/// normalization is needed before the escapes are read.
///
/// `pub(crate)`: `domain`'s own `string` type check reads a field's raw atom
/// text the same way a `:matches`/`:one-of` refinement value does here, and
/// a second copy of escape resolution is a second place for the two to
/// disagree about what a string literal means.
pub(crate) fn string_literal_contents(text: &str) -> Option<String> {
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
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
(defschema config
  (fields
    (:name (:type string))
    (:port (:type integer :min 1 :max 65535))
    (:mode (:type string :one-of ("dev" "staging" "prod")))
    (:label (:type string :matches "a*b?" :optional t))))
"#;

    #[test]
    fn a_well_formed_schema_reads_every_field_with_its_refinements() {
        let schemas = parse_schemas(SAMPLE, "sample").expect("parse");
        assert_eq!(schemas.len(), 1);
        let schema = &schemas[0];
        assert_eq!(schema.name, "config");
        assert_eq!(schema.fields.len(), 4);

        let name = &schema.fields[0];
        assert_eq!(name.key, "name");
        assert_eq!(name.field_type, FieldType::String);
        assert!(!name.optional);

        let port = &schema.fields[1];
        assert_eq!(port.field_type, FieldType::Integer);
        assert_eq!(port.min, Some(1));
        assert_eq!(port.max, Some(65535));

        let mode = &schema.fields[2];
        assert_eq!(mode.field_type, FieldType::String);
        assert_eq!(
            mode.one_of,
            Some(vec![
                "dev".to_owned(),
                "staging".to_owned(),
                "prod".to_owned()
            ])
        );

        let label = &schema.fields[3];
        assert_eq!(label.field_type, FieldType::String);
        assert_eq!(label.matches.as_deref(), Some("a*b?"));
        assert!(label.optional);
    }

    #[test]
    fn every_supported_type_parses() {
        for type_name in ["string", "integer", "boolean", "symbol", "list"] {
            let text = format!("(defschema s (fields (:f (:type {type_name}))))");
            let schemas = parse_schemas(&text, "sample").expect("parse");
            assert_eq!(
                schemas[0].fields[0].field_type,
                FieldType::parse(type_name).unwrap()
            );
        }
    }

    #[test]
    fn an_unrecognized_type_is_rejected_by_name() {
        let text = "(defschema s (fields (:f (:type frobnicator))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::UnknownType { type_name, .. } if type_name == "frobnicator"
        ));
    }

    #[test]
    fn an_unrecognized_refinement_keyword_is_rejected() {
        let text = "(defschema s (fields (:f (:type string :frobnicate t))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::UnknownRefinement { refinement, .. } if refinement == "frobnicate"
        ));
    }

    #[test]
    fn a_duplicate_type_keyword_is_rejected() {
        let text = "(defschema s (fields (:f (:type string :type integer))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::DuplicateSpecKeyword {
                keyword: "type",
                ..
            }
        ));
    }

    #[test]
    fn a_duplicate_refinement_keyword_is_rejected() {
        let text = "(defschema s (fields (:f (:type integer :min 1 :min 2))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::DuplicateSpecKeyword { keyword: "min", .. }
        ));
    }

    #[test]
    fn min_on_a_string_field_is_rejected() {
        let text = r#"(defschema s (fields (:f (:type string :min 1))))"#;
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::RefinementWrongType {
                refinement: "min",
                field_type: "string",
                ..
            }
        ));
    }

    #[test]
    fn max_on_a_boolean_field_is_rejected() {
        let text = "(defschema s (fields (:f (:type boolean :max 5))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::RefinementWrongType {
                refinement: "max",
                field_type: "boolean",
                ..
            }
        ));
    }

    #[test]
    fn one_of_on_an_integer_field_is_rejected() {
        let text = r#"(defschema s (fields (:f (:type integer :one-of ("a" "b")))))"#;
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::RefinementWrongType {
                refinement: "one-of",
                field_type: "integer",
                ..
            }
        ));
    }

    #[test]
    fn matches_on_a_symbol_field_is_rejected() {
        let text = r#"(defschema s (fields (:f (:type symbol :matches "a*"))))"#;
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(
            error,
            DslError::RefinementWrongType {
                refinement: "matches",
                field_type: "symbol",
                ..
            }
        ));
    }

    #[test]
    fn a_field_missing_type_is_rejected() {
        let text = "(defschema s (fields (:f (:min 1))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(error, DslError::FieldMissingType { .. }));
    }

    #[test]
    fn two_schemas_of_one_name_in_one_file_are_rejected() {
        let text = r#"
(defschema s (fields (:a (:type string))))
(defschema s (fields (:b (:type integer))))
"#;
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(error, DslError::DuplicateName { name } if name == "s"));
    }

    #[test]
    fn malformed_lisp_syntax_is_a_typed_error_not_a_panic() {
        let error = parse_schemas("(defschema s (fields", "sample").expect_err("must refuse");
        assert!(matches!(error, DslError::Unreadable { .. }));
    }

    #[test]
    fn a_top_level_form_that_is_not_a_defschema_is_refused() {
        let error = parse_schemas("(defun foo ())", "sample").expect_err("must refuse");
        assert!(matches!(error, DslError::NotADefschema { .. }));
    }

    #[test]
    fn a_field_key_that_is_not_a_keyword_is_refused() {
        let text = "(defschema s (fields (name (:type string))))";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(error, DslError::FieldKeyNotAKeyword { .. }));
    }

    #[test]
    fn a_schema_naming_no_fields_form_is_refused() {
        let text = "(defschema s)";
        let error = parse_schemas(text, "sample").expect_err("must refuse");
        assert!(matches!(error, DslError::MissingFields { .. }));
    }

    #[test]
    fn two_defschemas_with_distinct_names_both_parse() {
        let text = r#"
(defschema a (fields (:x (:type string))))
(defschema b (fields (:y (:type integer))))
"#;
        let schemas = parse_schemas(text, "sample").expect("parse");
        assert_eq!(schemas.len(), 2);
        assert_eq!(schemas[0].name, "a");
        assert_eq!(schemas[1].name, "b");
    }

    #[test]
    fn optional_defaults_to_false() {
        let text = "(defschema s (fields (:x (:type string))))";
        let schemas = parse_schemas(text, "sample").expect("parse");
        assert!(!schemas[0].fields[0].optional);
    }

    #[test]
    fn optional_nil_is_explicitly_false() {
        let text = "(defschema s (fields (:x (:type string :optional nil))))";
        let schemas = parse_schemas(text, "sample").expect("parse");
        assert!(!schemas[0].fields[0].optional);
    }
}
