//! Validating one S-expression instance against a parsed [`SchemaDef`].
//!
//! Nothing here evaluates the instance or the schema: every check reads only
//! the shape and the literal atoms already in the parsed tree, the same
//! discipline `data-report`'s schema-free checks follow.

use std::collections::HashSet;

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, is_paren_list};
use serde_json::{Value, json};

use paredit_core_cli::report::{Finding, FindingSeverity};

use crate::dsl::{FieldSpec, FieldType, SchemaDef};

/// What kind of schema violation one finding reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaIssueKind {
    /// A required field (no `:optional t`) is absent from the instance.
    MissingRequiredField,
    /// A present field's value does not match its declared `:type`.
    TypeMismatch,
    /// An integer field is below its `:min`.
    MinViolation,
    /// An integer field is above its `:max`.
    MaxViolation,
    /// A string field's value is not in its `:one-of` set.
    OneOfViolation,
    /// A string field's value does not match its `:matches` glob.
    MatchesViolation,
    /// A field present in the instance is not declared by the schema.
    UnknownField,
}

impl SchemaIssueKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MissingRequiredField => "missing-required-field",
            Self::TypeMismatch => "type-mismatch",
            Self::MinViolation => "min-violation",
            Self::MaxViolation => "max-violation",
            Self::OneOfViolation => "one-of-violation",
            Self::MatchesViolation => "matches-violation",
            Self::UnknownField => "unknown-field",
        }
    }
}

/// One schema violation `schema check` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIssue {
    pub kind: SchemaIssueKind,
    pub detail: String,
    pub span: ByteSpan,
}

impl Finding for SchemaIssue {
    fn kind(&self) -> &'static str {
        self.kind.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.detail.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("detail", json!(self.detail))]
    }

    /// An unknown field is often a typo of a real one, worth surfacing, but
    /// it is not by itself evidence the instance is wrong the way a missing
    /// required field or a type mismatch is — so it is reported at a lower
    /// rung than every other kind here.
    fn severity(&self) -> FindingSeverity {
        match self.kind {
            SchemaIssueKind::UnknownField => FindingSeverity::Note,
            _ => FindingSeverity::Warning,
        }
    }
}

/// One instance field: its declared-or-observed name, the key token that
/// named it, and its value.
struct InstanceField {
    key: String,
    key_view: ExpressionView,
    value_view: ExpressionView,
}

/// Reads `view`'s children as the instance's top-level fields — a plist
/// (`:key value :key value ...`) or an alist (`(key . value)` / `(key
/// value)` entries) — or `None` when `view` is shaped like neither.
///
/// A plist key is a keyword (`:name`) and an alist key is ordinarily a bare
/// symbol (`name`); both are normalized to the same colon-free name here, so
/// a schema's field key matches an instance field the same way whichever
/// shape wrote it.
fn instance_fields(view: &ExpressionView) -> Option<Vec<InstanceField>> {
    plist_fields(view).or_else(|| alist_fields(view))
}

fn keyword_name(view: &ExpressionView) -> Option<String> {
    atom_text(view).map(|text| text.trim_start_matches(':').to_owned())
}

fn plist_fields(view: &ExpressionView) -> Option<Vec<InstanceField>> {
    if !is_paren_list(view) || view.children.is_empty() || view.children.len() % 2 != 0 {
        return None;
    }
    let mut entries = Vec::with_capacity(view.children.len() / 2);
    for pair in view.children.chunks_exact(2) {
        let key_text = atom_text(&pair[0]).filter(|text| text.starts_with(':'))?;
        entries.push(InstanceField {
            key: key_text.trim_start_matches(':').to_owned(),
            key_view: pair[0].clone(),
            value_view: pair[1].clone(),
        });
    }
    Some(entries)
}

fn alist_fields(view: &ExpressionView) -> Option<Vec<InstanceField>> {
    if !is_paren_list(view) || view.children.is_empty() {
        return None;
    }
    let mut entries = Vec::with_capacity(view.children.len());
    for child in &view.children {
        let (key_view, value_view) = alist_entry(child)?;
        let key = keyword_name(key_view)?;
        entries.push(InstanceField {
            key,
            key_view: key_view.clone(),
            value_view: value_view.clone(),
        });
    }
    Some(entries)
}

/// `(key . value)` or `(key value)` — the same dotted-pair-or-2-list reading
/// `data-report`'s alist checks use.
fn alist_entry(child: &ExpressionView) -> Option<(&ExpressionView, &ExpressionView)> {
    if !is_paren_list(child) {
        return None;
    }
    let key = child.children.first()?;
    atom_text(key)?;
    match child.children.len() {
        2 => Some((key, &child.children[1])),
        3 if is_dot(&child.children[1]) => Some((key, &child.children[2])),
        _ => None,
    }
}

/// Whether `view` is the `.` of a dotted pair — punctuation, not a value.
fn is_dot(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view.reader_prefixes.is_empty()
        && atom_text(view) == Some(".")
}

/// Validates `instance` against `schema`, or `None` when `instance` is
/// shaped like neither a plist nor an alist — the caller's cue to refuse
/// with "not a recognizable instance shape" rather than cascading into a
/// wall of confusing per-field mismatches.
#[must_use]
pub fn validate_instance(
    schema: &SchemaDef,
    instance: &ExpressionView,
) -> Option<Vec<SchemaIssue>> {
    let fields = instance_fields(instance)?;
    let mut issues = Vec::new();

    for field in &schema.fields {
        match fields.iter().find(|entry| entry.key == field.key) {
            Some(entry) => issues.extend(field_issues(field, &entry.value_view)),
            None if !field.optional => issues.push(SchemaIssue {
                kind: SchemaIssueKind::MissingRequiredField,
                detail: format!(":{} is required and is absent from the instance", field.key),
                span: instance.span,
            }),
            None => {}
        }
    }

    let declared: HashSet<&str> = schema
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    for entry in &fields {
        if !declared.contains(entry.key.as_str()) {
            issues.push(SchemaIssue {
                kind: SchemaIssueKind::UnknownField,
                detail: format!(":{} is not declared by this schema", entry.key),
                span: entry.key_view.span,
            });
        }
    }

    Some(issues)
}

/// The value a field's atom or list resolved to, once it has passed its
/// `:type` check — kept so a refinement is checked against the already-typed
/// value rather than re-parsing the raw text a second time.
enum TypedValue {
    String(String),
    Integer(i64),
    /// No refinement applies to `boolean`, `symbol`, or `list`, so nothing
    /// downstream needs the value itself — only that it typed successfully.
    Boolean,
    Symbol,
    List,
}

fn field_issues(field: &FieldSpec, value: &ExpressionView) -> Vec<SchemaIssue> {
    let Some(typed) = type_check(field.field_type, value) else {
        return vec![SchemaIssue {
            kind: SchemaIssueKind::TypeMismatch,
            detail: format!(
                ":{} expected a {}, found {}",
                field.key,
                field.field_type.label(),
                describe_value(value)
            ),
            span: value.span,
        }];
    };
    refinement_issues(field, value, &typed)
}

/// Whether `value` matches `field_type`, and — for `string`/`integer` — the
/// value read out of it for the refinement checks that follow.
///
/// `boolean` accepts `t`/`nil` case-insensitively (the reader's own
/// convention for `T`/`NIL`). `symbol` accepts any bare atom that is not a
/// string literal, a keyword, or an integer — deliberately excluding
/// keywords, since this DSL already has a distinct name (a plist key) for
/// that shape and a field declared `symbol` is asking for an ordinary,
/// unqualified identifier.
fn type_check(field_type: FieldType, value: &ExpressionView) -> Option<TypedValue> {
    match field_type {
        FieldType::List => is_paren_list(value).then_some(TypedValue::List),
        FieldType::String => atom_text(value)
            .and_then(crate::dsl::string_literal_contents)
            .map(TypedValue::String),
        FieldType::Integer => atom_text(value)
            .filter(|text| !text.starts_with('"') && !text.starts_with(':'))
            .and_then(|text| text.parse::<i64>().ok())
            .map(TypedValue::Integer),
        FieldType::Boolean => match atom_text(value) {
            Some(text) if text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("nil") => {
                Some(TypedValue::Boolean)
            }
            _ => None,
        },
        FieldType::Symbol => atom_text(value)
            .filter(|text| {
                !text.starts_with('"') && !text.starts_with(':') && text.parse::<i64>().is_err()
            })
            .map(|_| TypedValue::Symbol),
    }
}

/// A short label of what `value` actually is, for a type-mismatch message.
fn describe_value(value: &ExpressionView) -> String {
    if is_paren_list(value) {
        return "a list".to_owned();
    }
    match atom_text(value) {
        Some(text) if text.starts_with('"') => "a string".to_owned(),
        Some(text) if text.starts_with(':') => format!("the keyword {text}"),
        Some(text) if text.parse::<i64>().is_ok() => "an integer".to_owned(),
        Some(text) => format!("the symbol {text}"),
        None => "an unrecognized value".to_owned(),
    }
}

fn refinement_issues(
    field: &FieldSpec,
    value: &ExpressionView,
    typed: &TypedValue,
) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    match typed {
        TypedValue::Integer(number) => {
            if let Some(min) = field.min {
                if *number < min {
                    issues.push(SchemaIssue {
                        kind: SchemaIssueKind::MinViolation,
                        detail: format!(":{} is {number}, below the minimum {min}", field.key),
                        span: value.span,
                    });
                }
            }
            if let Some(max) = field.max {
                if *number > max {
                    issues.push(SchemaIssue {
                        kind: SchemaIssueKind::MaxViolation,
                        detail: format!(":{} is {number}, above the maximum {max}", field.key),
                        span: value.span,
                    });
                }
            }
        }
        TypedValue::String(text) => {
            // `field.one_of`/`field.matches` can only be `Some` on a `string`
            // field: the parser already refused either refinement on any
            // other type, so `TypedValue::String` (produced only by
            // `FieldType::String`) is the one arm that needs to look at them.
            if let Some(allowed) = &field.one_of {
                if !allowed.iter().any(|candidate| candidate == text) {
                    issues.push(SchemaIssue {
                        kind: SchemaIssueKind::OneOfViolation,
                        detail: format!(
                            ":{} is {text:?}, not one of {}",
                            field.key,
                            allowed.join(", ")
                        ),
                        span: value.span,
                    });
                }
            }
            if let Some(pattern) = &field.matches {
                if !glob_match(pattern, text) {
                    issues.push(SchemaIssue {
                        kind: SchemaIssueKind::MatchesViolation,
                        detail: format!(
                            ":{} is {text:?}, which does not match the pattern {pattern:?}",
                            field.key
                        ),
                        span: value.span,
                    });
                }
            }
        }
        TypedValue::Boolean | TypedValue::Symbol | TypedValue::List => {}
    }
    issues
}

/// A small hand-rolled glob matcher: `*` matches any run of characters
/// (including none), `?` matches exactly one character, and every other
/// character matches itself literally. Deliberately not a regular
/// expression — see this crate's README and `dsl`'s module doc comment for
/// why.
///
/// The classic two-pointer wildcard-matching algorithm, not a naive
/// backtracking recursion: a pattern like `a*a*a*a*a*b` against a long
/// non-matching text would make the naive version's runtime blow up, and
/// this tool budgets its own work rather than trusting every input to be
/// friendly.
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_t = 0usize;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_t = t;
            p += 1;
        } else if let Some(star_p) = star {
            p = star_p + 1;
            star_t += 1;
            t = star_t;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use crate::dsl::parse_schemas;

    const SCHEMA: &str = r#"
(defschema config
  (fields
    (:name (:type string))
    (:port (:type integer :min 1 :max 65535))
    (:mode (:type string :one-of ("dev" "staging" "prod")))
    (:label (:type string :matches "a*b?" :optional t))))
"#;

    fn schema() -> SchemaDef {
        parse_schemas(SCHEMA, "sample").expect("parse")[0].clone()
    }

    fn issues(instance_source: &str) -> Vec<SchemaIssue> {
        let tree =
            SyntaxTree::parse_with_dialect(instance_source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let instance = &root.children[0];
        validate_instance(&schema(), instance).expect("recognizable instance shape")
    }

    #[test]
    fn a_fully_valid_plist_instance_has_no_findings() {
        let found = issues(r#"(:name "svc" :port 8080 :mode "dev" :label "abZ")"#);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_fully_valid_alist_instance_has_no_findings() {
        let found = issues(r#"((name . "svc") (port . 8080) (mode . "dev") (label . "abZ"))"#);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn plist_and_alist_shapes_validate_identically() {
        let plist = issues(r#"(:name "svc" :port 99999 :mode "dev")"#);
        let alist = issues(r#"((name . "svc") (port . 99999) (mode . "dev"))"#);
        let plist_kinds: Vec<_> = plist.iter().map(|issue| issue.kind).collect();
        let alist_kinds: Vec<_> = alist.iter().map(|issue| issue.kind).collect();
        assert_eq!(plist_kinds, alist_kinds);
    }

    #[test]
    fn a_missing_required_field_is_reported() {
        let found = issues(r#"(:port 8080 :mode "dev")"#);
        assert!(
            found
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::MissingRequiredField
                    && issue.detail.contains("name")),
            "{found:?}"
        );
    }

    #[test]
    fn a_present_but_optional_field_absent_reports_nothing_about_it() {
        let found = issues(r#"(:name "svc" :port 8080 :mode "dev")"#);
        assert!(
            found
                .iter()
                .all(|issue| issue.kind != SchemaIssueKind::MissingRequiredField),
            "{found:?}"
        );
    }

    #[test]
    fn a_type_mismatch_is_reported_for_every_supported_type() {
        assert!(
            issues(r#"(:name 42 :port 1 :mode "dev")"#)
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::TypeMismatch),
            "string field given an integer"
        );
        assert!(
            issues(r#"(:name "svc" :port "not-a-number" :mode "dev")"#)
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::TypeMismatch),
            "integer field given a string"
        );
    }

    #[test]
    fn boolean_and_symbol_and_list_types_accept_their_own_shape_and_reject_others() {
        let text = r#"
(defschema t
  (fields (:flag (:type boolean)) (:sym (:type symbol)) (:items (:type list))))
"#;
        let schema = &parse_schemas(text, "sample").expect("parse")[0];

        let ok_tree = SyntaxTree::parse_with_dialect(
            r#"(:flag t :sym active :items (1 2 3))"#,
            Dialect::CommonLisp,
        )
        .expect("parse");
        let ok_root = ok_tree.root_view();
        let ok = validate_instance(schema, &ok_root.children[0]).expect("shape");
        assert!(ok.is_empty(), "{ok:?}");

        let bad_tree = SyntaxTree::parse_with_dialect(
            r#"(:flag "not-a-bool" :sym "not-a-symbol" :items 5)"#,
            Dialect::CommonLisp,
        )
        .expect("parse");
        let bad_root = bad_tree.root_view();
        let bad = validate_instance(schema, &bad_root.children[0]).expect("shape");
        assert_eq!(bad.len(), 3, "{bad:?}");
        assert!(
            bad.iter()
                .all(|issue| issue.kind == SchemaIssueKind::TypeMismatch)
        );
    }

    #[test]
    fn min_boundary_values() {
        assert!(
            issues(r#"(:name "s" :port 1 :mode "dev")"#)
                .iter()
                .all(|issue| issue.kind != SchemaIssueKind::MinViolation),
            "exactly at min must not violate"
        );
        assert!(
            issues(r#"(:name "s" :port 0 :mode "dev")"#)
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::MinViolation),
            "one below min must violate"
        );
    }

    #[test]
    fn max_boundary_values() {
        assert!(
            issues(r#"(:name "s" :port 65535 :mode "dev")"#)
                .iter()
                .all(|issue| issue.kind != SchemaIssueKind::MaxViolation),
            "exactly at max must not violate"
        );
        assert!(
            issues(r#"(:name "s" :port 65536 :mode "dev")"#)
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::MaxViolation),
            "one above max must violate"
        );
    }

    #[test]
    fn one_of_accepts_a_member_and_rejects_a_non_member() {
        assert!(
            issues(r#"(:name "s" :port 1 :mode "dev")"#)
                .iter()
                .all(|issue| issue.kind != SchemaIssueKind::OneOfViolation)
        );
        assert!(
            issues(r#"(:name "s" :port 1 :mode "prehistoric")"#)
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::OneOfViolation)
        );
    }

    #[test]
    fn matches_accepts_a_matching_glob_and_rejects_a_non_matching_one() {
        assert!(
            issues(r#"(:name "s" :port 1 :mode "dev" :label "axbz")"#)
                .iter()
                .all(|issue| issue.kind != SchemaIssueKind::MatchesViolation)
        );
        assert!(
            issues(r#"(:name "s" :port 1 :mode "dev" :label "zzz")"#)
                .iter()
                .any(|issue| issue.kind == SchemaIssueKind::MatchesViolation)
        );
    }

    #[test]
    fn an_unknown_field_is_reported_at_a_lower_severity_than_a_violation() {
        let found = issues(r#"(:name "s" :port 1 :mode "dev" :bogus 1)"#);
        let unknown = found
            .iter()
            .find(|issue| issue.kind == SchemaIssueKind::UnknownField)
            .expect("unknown field reported");
        assert_eq!(unknown.severity(), FindingSeverity::Note);

        let violation = SchemaIssue {
            kind: SchemaIssueKind::TypeMismatch,
            detail: String::new(),
            span: unknown.span,
        };
        assert!(unknown.severity() < violation.severity());
    }

    #[test]
    fn an_instance_that_is_neither_alist_nor_plist_shaped_is_unrecognized() {
        let tree = SyntaxTree::parse_with_dialect("(1 2 3)", Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        assert!(validate_instance(&schema(), &root.children[0]).is_none());
    }

    mod glob {
        use super::*;

        #[test]
        fn a_literal_pattern_matches_only_itself() {
            assert!(glob_match("abc", "abc"));
            assert!(!glob_match("abc", "abd"));
        }

        #[test]
        fn star_matches_any_run_including_empty() {
            assert!(glob_match("a*b", "ab"));
            assert!(glob_match("a*b", "axyzb"));
            assert!(!glob_match("a*b", "axyzc"));
        }

        #[test]
        fn question_mark_matches_exactly_one_character() {
            assert!(glob_match("a?c", "abc"));
            assert!(!glob_match("a?c", "ac"));
            assert!(!glob_match("a?c", "abbc"));
        }

        #[test]
        fn combined_wildcards() {
            assert!(glob_match("*-?.lisp", "foo-1.lisp"));
            assert!(!glob_match("*-?.lisp", "foo-12.lisp"));
        }

        #[test]
        fn a_backtracking_hostile_pattern_still_resolves_quickly() {
            let pattern = "a*a*a*a*a*a*a*a*a*a*b";
            let text = "a".repeat(30);
            assert!(!glob_match(pattern, &text));
        }
    }
}
