//! Reading a project's `.paredit/rules/*.lisp` into rules, tests, and
//! deprecations.
//!
//! The file format is Lisp because the reader is already here and because a
//! project writing Lisp should not have to learn a second syntax to say
//! something about its own Lisp. Three top-level forms:
//!
//! ```lisp
//! (defrule entity-needs-table
//!   :category malformed
//!   :severity error
//!   :description "a defentity with no :table option"
//!   :pattern (defentity ?name ...)
//!   :message "defentity needs a :table"
//!   :fix (defentity ?name :table "TODO"))     ; optional
//!
//! (deftest entity-needs-table
//!   (:matches  "(defentity user)")
//!   (:no-match "(defentity user :table \"users\")")
//!   (:fix "(defentity user)" "(defentity user :table \"TODO\")"))
//!
//! (deprecate legacy-connect :use connect :reason "removed in 3.0")
//! ```
//!
//! Every error here is a *file* error, reported with the rule's name and the
//! option that was wrong. A rule file that does not load must fail the run
//! rather than silently contribute nothing: a project that has written a rule
//! and sees a green build has been told the rule passed.

use paredit_core_lint_engine::model::{RuleCategory, Severity};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::error::{PatternError, RewriteError};
use paredit_core_syntax::selector::pattern::Rest;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, symbol_is};
use thiserror::Error;

use crate::pattern::{Pattern, Template, fix_from_view, from_view};

/// What can be wrong with a rule file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RulesetError {
    #[error("rule file {path} does not parse as Lisp: {reason}")]
    Unreadable { path: String, reason: String },

    #[error("{form} needs a name as its first argument")]
    MissingName { form: String },

    #[error("custom rule {rule:?} is missing the required option {option}")]
    MissingOption { rule: String, option: &'static str },

    #[error("custom rule {rule:?} has an unknown {option} {value:?}; valid values: {valid}")]
    UnknownValue {
        rule: String,
        option: &'static str,
        value: String,
        valid: String,
    },

    #[error("custom rule {rule:?}'s :fix names ?{variable}, which its :pattern does not bind")]
    UnboundFixVariable { rule: String, variable: String },

    #[error("custom rule {rule:?}'s :pattern is not a valid pattern: {source}")]
    InvalidPattern {
        rule: String,
        #[source]
        source: PatternError,
    },

    #[error("custom rule {rule:?}'s :fix is not a valid rewrite template: {source}")]
    InvalidFix {
        rule: String,
        #[source]
        source: RewriteError,
    },

    #[error("custom rule name {name:?} collides with a shipped rule")]
    NameCollision { name: String },

    #[error("(deftest {name:?}) names no custom rule in this rule set")]
    TestWithoutRule { name: String },
}

/// One rule a project wrote for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRule {
    pub name: String,
    pub category: RuleCategory,
    pub severity: Severity,
    pub description: String,
    pub message: String,
    pub pattern: Pattern,
    /// The rewrite, when the rule declares one.
    pub fix: Option<Template>,
    /// The `-- reason` a deprecation carried, for the message. Empty for a
    /// `defrule`.
    pub note: String,
}

/// One declarative test of a custom rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleTest {
    pub rule: String,
    /// Source that must produce a finding.
    pub matches: Vec<String>,
    /// Source that must not.
    pub no_match: Vec<String>,
    /// `(before, after)` pairs the rule's fix must produce.
    pub fixes: Vec<(String, String)>,
}

/// A whole rule set: the rules, and the tests that keep them honest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ruleset {
    pub rules: Vec<CustomRule>,
    pub tests: Vec<RuleTest>,
}

impl Ruleset {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Merges another file's forms in.
    pub fn extend(&mut self, other: Self) {
        self.rules.extend(other.rules);
        self.tests.extend(other.tests);
    }

    /// Checks the whole set against the shipped rule names.
    ///
    /// Done after every file is read rather than per file, because a collision
    /// is a property of the set and a test may precede its rule.
    pub fn validate(&self, shipped: &[&str]) -> Result<(), RulesetError> {
        for rule in &self.rules {
            if shipped.contains(&rule.name.as_str()) {
                return Err(RulesetError::NameCollision {
                    name: rule.name.clone(),
                });
            }
        }
        for test in &self.tests {
            if !self.rules.iter().any(|rule| rule.name == test.rule) {
                return Err(RulesetError::TestWithoutRule {
                    name: test.rule.clone(),
                });
            }
        }
        Ok(())
    }
}

/// The string a `:keyword`-valued option carries, unquoted and unescaped.
///
/// Unescaping matters more than it looks. A `deftest` clause naming an expected
/// fix is *Lisp source inside a Lisp string*, so any fix that produces a string
/// — every `format` rewrite, which is most of them — arrives here as
/// `(format t \"~a\" x)`. Comparing that against what the fix actually produced
/// would fail on the backslashes alone, and the failure would be about the rule
/// file's quoting rather than about the rule.
fn option_text(view: &ExpressionView) -> Option<String> {
    let text = atom_text(view)?;
    let Some(inner) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        // A bare symbol option value (`:category malformed`) is itself.
        return Some(text.to_owned());
    };

    let mut unescaped = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            // The reader passes `\c` through as `c` for every `c`, so this needs
            // no table: only `\"` and `\\` can legally appear, and both mean
            // themselves.
            if let Some(escaped) = characters.next() {
                unescaped.push(escaped);
            }
            continue;
        }
        unescaped.push(character);
    }
    Some(unescaped)
}

/// The value following `key` in a `:key value` plist, as a view.
fn option<'a>(form: &'a ExpressionView, key: &str) -> Option<&'a ExpressionView> {
    form.children
        .iter()
        .zip(form.children.iter().skip(1))
        .find(|(name, _)| atom_text(name).is_some_and(|text| text.eq_ignore_ascii_case(key)))
        .map(|(_, value)| value)
}

fn category_of(rule: &str, form: &ExpressionView) -> Result<RuleCategory, RulesetError> {
    let Some(view) = option(form, ":category") else {
        // A rule that does not say is `suspicious`: the category the shipped
        // suite uses for "well-formed code that probably does not mean what it
        // says", which is what a project-specific rule almost always reports.
        return Ok(RuleCategory::Suspicious);
    };
    let value = option_text(view).unwrap_or_default();
    RuleCategory::ALL
        .into_iter()
        .find(|category| category.as_str().eq_ignore_ascii_case(&value))
        .ok_or_else(|| RulesetError::UnknownValue {
            rule: rule.to_owned(),
            option: ":category",
            value,
            valid: RuleCategory::ALL
                .iter()
                .map(|category| category.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        })
}

fn severity_of(rule: &str, form: &ExpressionView) -> Result<Severity, RulesetError> {
    let Some(view) = option(form, ":severity") else {
        return Ok(Severity::Warning);
    };
    let value = option_text(view).unwrap_or_default();
    match value.to_ascii_lowercase().as_str() {
        "error" => Ok(Severity::Error),
        "warning" => Ok(Severity::Warning),
        _ => Err(RulesetError::UnknownValue {
            rule: rule.to_owned(),
            option: ":severity",
            value,
            valid: "error, warning".to_owned(),
        }),
    }
}

/// Reads one `(defrule …)`.
fn read_defrule(form: &ExpressionView, text: &str) -> Result<CustomRule, RulesetError> {
    let name = form
        .children
        .get(1)
        .and_then(atom_text)
        .ok_or_else(|| RulesetError::MissingName {
            form: "defrule".to_owned(),
        })?
        .to_owned();

    let pattern_view = option(form, ":pattern").ok_or_else(|| RulesetError::MissingOption {
        rule: name.clone(),
        option: ":pattern",
    })?;
    let pattern = from_view(pattern_view).map_err(|source| RulesetError::InvalidPattern {
        rule: name.clone(),
        source,
    })?;

    let message = option(form, ":message")
        .and_then(option_text)
        .ok_or_else(|| RulesetError::MissingOption {
            rule: name.clone(),
            option: ":message",
        })?;

    let fix = option(form, ":fix")
        .map(|view| fix_from_view(view, text))
        .transpose()
        .map_err(|source| RulesetError::InvalidFix {
            rule: name.clone(),
            source,
        })?;
    if let Some(fix) = &fix {
        let bound = pattern.capture_names();
        // A fix naming something the pattern never binds would render as the
        // literal `?x` and write it into the file (or, for `?_`, ask to
        // substitute a capture `:pattern`'s own anonymous `?_` never binds —
        // see `pattern`'s module documentation). Rejecting it here is the
        // difference between a rule file that fails to load and a rule file
        // that corrupts source.
        for variable in fix.capture_names() {
            if !bound.contains(&variable) {
                return Err(RulesetError::UnboundFixVariable {
                    rule: name.clone(),
                    variable,
                });
            }
        }
    }

    Ok(CustomRule {
        category: category_of(&name, form)?,
        severity: severity_of(&name, form)?,
        description: option(form, ":description")
            .and_then(option_text)
            .unwrap_or_else(|| message.clone()),
        message,
        pattern,
        fix,
        note: String::new(),
        name,
    })
}

/// Reads one `(deprecate name :use replacement :reason "…")`.
///
/// A deprecation is a rule, not a separate mechanism: it is "match a call to
/// this name, say it is deprecated". Making it its own top-level form is only
/// about how it *reads* — `(deprecate legacy-connect :use connect)` says what
/// it means, and the equivalent `defrule` would spell out a pattern, a message,
/// and a fix to say the same thing three times.
fn read_deprecate(form: &ExpressionView) -> Result<CustomRule, RulesetError> {
    let name = form
        .children
        .get(1)
        .and_then(atom_text)
        .ok_or_else(|| RulesetError::MissingName {
            form: "deprecate".to_owned(),
        })?
        .to_owned();

    let replacement = option(form, ":use").and_then(option_text);
    let reason = option(form, ":reason").and_then(option_text);

    // `(name ...)`: any call to the deprecated operator, whatever its arity.
    let pattern = Pattern::List {
        delimiter: Delimiter::Paren,
        prefixes: Vec::new(),
        before: vec![Pattern::Atom {
            text: name.clone(),
            prefixes: Vec::new(),
        }],
        rest: Some(Rest { capture: None }),
        after: Vec::new(),
    };

    let mut message = format!("{name} is deprecated");
    if let Some(replacement) = &replacement {
        message.push_str(&format!("; use {replacement} instead"));
    }
    if let Some(reason) = &reason {
        message.push_str(&format!(" ({reason})"));
    }

    Ok(CustomRule {
        name: format!("deprecated-{name}"),
        category: RuleCategory::DeadCode,
        severity: severity_of(&name, form)?,
        description: format!("a call to the deprecated {name}"),
        message,
        pattern,
        // No fix: `(old a b)` and `(new a b)` take the same arguments only if
        // somebody has checked, and a deprecation note is not that check.
        fix: None,
        note: reason.unwrap_or_default(),
    })
}

/// Reads one `(deftest rule (:matches "…") (:no-match "…") (:fix "…" "…"))`.
fn read_deftest(form: &ExpressionView) -> Result<RuleTest, RulesetError> {
    let rule = form
        .children
        .get(1)
        .and_then(atom_text)
        .ok_or_else(|| RulesetError::MissingName {
            form: "deftest".to_owned(),
        })?
        .to_owned();

    let mut test = RuleTest {
        rule,
        matches: Vec::new(),
        no_match: Vec::new(),
        fixes: Vec::new(),
    };
    for clause in form.children.iter().skip(2) {
        let Some(kind) = clause.children.first().and_then(atom_text) else {
            continue;
        };
        let arguments: Vec<String> = clause
            .children
            .iter()
            .skip(1)
            .filter_map(option_text)
            .collect();
        match kind.to_ascii_lowercase().as_str() {
            ":matches" => test.matches.extend(arguments),
            ":no-match" => test.no_match.extend(arguments),
            ":fix" => {
                if let [before, after] = arguments.as_slice() {
                    test.fixes.push((before.clone(), after.clone()));
                }
            }
            _ => {}
        }
    }
    Ok(test)
}

/// Reads one rule file's text.
pub fn parse_ruleset(path: &str, text: &str) -> Result<Ruleset, RulesetError> {
    let tree = SyntaxTree::parse_with_dialect(text, Dialect::CommonLisp).map_err(|error| {
        RulesetError::Unreadable {
            path: path.to_owned(),
            reason: error.to_string(),
        }
    })?;

    let mut ruleset = Ruleset::default();
    for form in &tree.root_view().children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if symbol_is(head, "defrule") {
            ruleset.rules.push(read_defrule(form, text)?);
        } else if symbol_is(head, "deprecate") {
            ruleset.rules.push(read_deprecate(form)?);
        } else if symbol_in(head, &["deftest"]) {
            ruleset.tests.push(read_deftest(form)?);
        }
    }
    Ok(ruleset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Ruleset {
        parse_ruleset("rules.lisp", text).expect("a readable ruleset")
    }

    #[test]
    fn reads_a_complete_defrule() {
        let ruleset = parse(
            r#"(defrule entity-needs-table
                 :category malformed
                 :severity error
                 :description "a defentity with no :table"
                 :pattern (defentity ?name ...)
                 :message "defentity needs a :table")"#,
        );
        assert_eq!(ruleset.rules.len(), 1);
        let rule = &ruleset.rules[0];
        assert_eq!(rule.name, "entity-needs-table");
        assert_eq!(rule.category, RuleCategory::Malformed);
        assert_eq!(rule.severity, Severity::Error);
        assert_eq!(rule.description, "a defentity with no :table");
        assert_eq!(rule.message, "defentity needs a :table");
        assert!(rule.fix.is_none());
    }

    #[test]
    fn category_and_severity_default_when_unstated() {
        let ruleset = parse(r#"(defrule r :pattern (f) :message "m")"#);
        let rule = &ruleset.rules[0];
        assert_eq!(rule.category, RuleCategory::Suspicious);
        assert_eq!(rule.severity, Severity::Warning);
        // The description falls back to the message rather than being empty:
        // `--list-rules` has a column to fill either way.
        assert_eq!(rule.description, "m");
    }

    #[test]
    fn a_rule_without_a_pattern_or_message_is_rejected() {
        assert_eq!(
            parse_ruleset("r.lisp", r#"(defrule r :message "m")"#),
            Err(RulesetError::MissingOption {
                rule: "r".to_owned(),
                option: ":pattern",
            })
        );
        assert_eq!(
            parse_ruleset("r.lisp", "(defrule r :pattern (f))"),
            Err(RulesetError::MissingOption {
                rule: "r".to_owned(),
                option: ":message",
            })
        );
    }

    #[test]
    fn an_unknown_category_or_severity_is_rejected_with_the_valid_values() {
        let error = parse_ruleset(
            "r.lisp",
            r#"(defrule r :category nonsense :pattern (f) :message "m")"#,
        )
        .expect_err("unknown category");
        let RulesetError::UnknownValue { valid, .. } = error else {
            panic!("expected an unknown-value error");
        };
        assert!(valid.contains("malformed"));

        assert!(
            parse_ruleset(
                "r.lisp",
                r#"(defrule r :severity fatal :pattern (f) :message "m")"#
            )
            .is_err()
        );
    }

    #[test]
    fn a_fix_naming_an_unbound_variable_is_rejected() {
        assert_eq!(
            parse_ruleset(
                "r.lisp",
                r#"(defrule r :pattern (f ?a) :message "m" :fix (g ?b))"#
            ),
            Err(RulesetError::UnboundFixVariable {
                rule: "r".to_owned(),
                variable: "b".to_owned(),
            })
        );
    }

    #[test]
    fn a_fix_naming_a_bound_variable_is_accepted() {
        let ruleset = parse(r#"(defrule r :pattern (f ?a) :message "m" :fix (g ?a))"#);
        assert!(ruleset.rules[0].fix.is_some());
    }

    #[test]
    fn a_deprecation_becomes_a_rule_that_matches_any_call() {
        let ruleset = parse(r#"(deprecate legacy-connect :use connect :reason "gone in 3.0")"#);
        let rule = &ruleset.rules[0];
        assert_eq!(rule.name, "deprecated-legacy-connect");
        assert_eq!(rule.category, RuleCategory::DeadCode);
        assert_eq!(
            rule.message,
            "legacy-connect is deprecated; use connect instead (gone in 3.0)"
        );
        // Never a fix: same-name does not mean same arguments.
        assert!(rule.fix.is_none());
        assert!(matches!(
            &rule.pattern,
            Pattern::List {
                rest: Some(Rest { capture: None }),
                ..
            }
        ));
    }

    #[test]
    fn a_bare_deprecation_still_says_something_useful() {
        let ruleset = parse("(deprecate old-fn)");
        assert_eq!(ruleset.rules[0].message, "old-fn is deprecated");
    }

    #[test]
    fn reads_a_deftest_with_every_clause() {
        let ruleset = parse(
            r#"(defrule r :pattern (f ?a) :message "m" :fix (g ?a))
               (deftest r
                 (:matches "(f 1)")
                 (:no-match "(h 1)")
                 (:fix "(f 1)" "(g 1)"))"#,
        );
        let test = &ruleset.tests[0];
        assert_eq!(test.rule, "r");
        assert_eq!(test.matches, vec!["(f 1)"]);
        assert_eq!(test.no_match, vec!["(h 1)"]);
        assert_eq!(test.fixes, vec![("(f 1)".to_owned(), "(g 1)".to_owned())]);
    }

    #[test]
    fn a_string_option_is_unescaped() {
        // A `deftest` naming a fix that produces a string is Lisp inside a Lisp
        // string; without unescaping, every `format` rewrite's test would fail
        // on backslashes.
        let ruleset = parse(
            r#"(defrule r :pattern (print ?x) :message "m" :fix (format t "~a" ?x))
               (deftest r (:fix "(print 1)" "(format t \"~a\" 1)"))"#,
        );
        assert_eq!(ruleset.tests[0].fixes[0].1, r#"(format t "~a" 1)"#);
    }

    #[test]
    fn a_name_that_collides_with_a_shipped_rule_is_rejected() {
        let ruleset = parse(r#"(defrule redundant-quote :pattern (f) :message "m")"#);
        assert_eq!(
            ruleset.validate(&["redundant-quote"]),
            Err(RulesetError::NameCollision {
                name: "redundant-quote".to_owned(),
            })
        );
        assert!(ruleset.validate(&["something-else"]).is_ok());
    }

    #[test]
    fn a_test_naming_no_rule_is_rejected() {
        let ruleset = parse(r#"(deftest ghost (:matches "(f)"))"#);
        assert_eq!(
            ruleset.validate(&[]),
            Err(RulesetError::TestWithoutRule {
                name: "ghost".to_owned(),
            })
        );
    }

    #[test]
    fn a_form_that_is_not_a_rule_form_is_ignored() {
        // A rule file may carry a header comment or a `defpackage`; neither is
        // an error, and neither contributes a rule.
        let ruleset = parse(";; a header\n(defpackage :rules)\n");
        assert!(ruleset.is_empty());
    }

    #[test]
    fn several_files_merge() {
        let mut first = parse(r#"(defrule a :pattern (f) :message "m")"#);
        first.extend(parse(r#"(defrule b :pattern (g) :message "m")"#));
        assert_eq!(first.rules.len(), 2);
    }
}
