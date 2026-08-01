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
//!
//! # `:dialects` is a guard, not a hint
//!
//! A `defrule` may add `:dialects (common-lisp emacs-lisp)`, the same clause
//! and the same meaning as `paredit_feature_migrate::recipe::Migration`'s: it
//! is not "prefer this dialect", it is "this rule does not apply outside
//! these dialects at all". Omitting it, as every rule written before this
//! clause existed does, keeps the rule scoped to every dialect — unchanged,
//! not a breaking change for any existing rule file.
//!
//! # `defpattern`: a named, reusable pattern fragment
//!
//! ```lisp
//! (defpattern bare-print (print ?x))
//!
//! (defrule no-print-in-handler
//!   :pattern (handler-case (:fragment bare-print) ...)
//!   :message "do not print from inside a handler")
//! ```
//!
//! `(:fragment name)` inside a `:pattern` (or inside another `defpattern`)
//! stands for that fragment's own pattern, substituted in whole before any
//! rule ever matches — see [`Ruleset::resolve_fragments`]. It composes with
//! the existing pattern grammar without changing it: `:fragment` is an
//! ordinary keyword atom, so `(:fragment name)` parses as an ordinary
//! two-atom list pattern until [`Ruleset::resolve_fragments`] recognizes and
//! replaces it. A `defrule` (or `defpattern`) naming an undefined fragment,
//! or fragments referencing each other in a cycle, is a load-time error, not
//! a silent no-op.

use std::collections::BTreeMap;

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

    #[error("two custom rules are named {name:?}; a rule name has to identify one rule")]
    DuplicateName { name: String },

    #[error("custom rule {rule:?} names the unknown dialect {dialect:?}; valid values: {valid}")]
    UnknownDialect {
        rule: String,
        dialect: String,
        valid: String,
    },

    #[error("(defpattern {name:?} ...) needs exactly one pattern form")]
    MissingFragmentBody { name: String },

    #[error("pattern fragment {name:?} is not a valid pattern: {source}")]
    InvalidFragment {
        name: String,
        #[source]
        source: PatternError,
    },

    #[error("{referrer} references the undefined pattern fragment {fragment:?}")]
    UndefinedFragment { referrer: String, fragment: String },

    #[error("pattern fragments form a cycle: {}", chain.join(" -> "))]
    FragmentCycle { chain: Vec<String> },

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
    /// The dialects this rule applies to. Empty means every dialect — see
    /// the module documentation's `:dialects` section.
    pub dialects: Vec<Dialect>,
}

impl CustomRule {
    /// Whether this rule may be applied to a file of `dialect`.
    ///
    /// Mirrors `paredit_feature_migrate::recipe::Migration::covers`: a guard
    /// checked once per file, not a hint that narrows matching within one.
    #[must_use]
    pub fn covers(&self, dialect: Dialect) -> bool {
        self.dialects.is_empty() || self.dialects.contains(&dialect)
    }
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

/// A whole rule set: the rules, the tests that keep them honest, and the
/// named pattern fragments a rule's `:pattern` may reference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ruleset {
    pub rules: Vec<CustomRule>,
    pub tests: Vec<RuleTest>,
    /// `(name, unresolved pattern)`, in the order each `defpattern` was read.
    /// "Unresolved" because a fragment may itself reference another fragment
    /// — see [`Self::resolve_fragments`].
    pub fragments: Vec<(String, Pattern)>,
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
        self.fragments.extend(other.fragments);
    }

    /// Substitutes every `(:fragment name)` reference — in every rule's
    /// pattern, and inside other fragments — with the named fragment's own,
    /// recursively resolved pattern.
    ///
    /// Must run after every file has been merged in (a rule in one file may
    /// reference a fragment defined in another, the same way `deftest` may
    /// precede the `defrule` it tests) and before [`Self::validate`], since a
    /// `:fix` naming a variable only a fragment binds cannot be checked
    /// until the fragment it came from is substituted in.
    ///
    /// Every `defpattern` is resolved here even when no rule references it
    /// yet, so a self-referential fragment is caught at load time rather
    /// than only the day something finally uses it.
    pub fn resolve_fragments(&mut self) -> Result<(), RulesetError> {
        let mut resolved: BTreeMap<String, Pattern> = BTreeMap::new();
        for (name, _) in self.fragments.clone() {
            let mut visiting = Vec::new();
            resolve_named_fragment(&name, &self.fragments, &mut resolved, &mut visiting)?;
        }

        for rule in &mut self.rules {
            let mut visiting = Vec::new();
            rule.pattern = resolve_pattern(
                &format!("defrule {}", rule.name),
                &rule.pattern,
                &self.fragments,
                &mut resolved,
                &mut visiting,
            )?;
            // Deferred from `read_defrule`: a fix naming a variable a
            // fragment binds could not be checked until the fragment it
            // came from was substituted in above. Rechecking every rule
            // here (not only fragment-referencing ones) costs nothing
            // measurable and keeps this the one place the check runs after
            // resolution.
            if let Some(fix) = &rule.fix {
                let bound = rule.pattern.capture_names();
                for variable in fix.capture_names() {
                    if !bound.contains(&variable) {
                        return Err(RulesetError::UnboundFixVariable {
                            rule: rule.name.clone(),
                            variable,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks the whole set against the shipped rule names, and against
    /// itself.
    ///
    /// Done after every file is read rather than per file, because a collision
    /// is a property of the set and a test may precede its rule. Checking a
    /// custom rule against every *earlier* custom rule as well as the shipped
    /// catalogue is what makes two rules of one name a load-time error rather
    /// than three different "which one wins" answers depending on which code
    /// path asks (the pass execution loop, the metadata cache, and `deftest`
    /// resolution each picked a different one before this existed).
    pub fn validate(&self, shipped: &[&str]) -> Result<(), RulesetError> {
        for (index, rule) in self.rules.iter().enumerate() {
            if shipped.contains(&rule.name.as_str()) {
                return Err(RulesetError::NameCollision {
                    name: rule.name.clone(),
                });
            }
            if self.rules[..index]
                .iter()
                .any(|earlier| earlier.name == rule.name)
            {
                return Err(RulesetError::DuplicateName {
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

/// The `(:fragment name)` shape [`Ruleset::resolve_fragments`] recognizes, or
/// `None` when `pattern` is not that shape.
///
/// Deliberately an ordinary two-atom list pattern (`:fragment` is just a
/// keyword atom) rather than a new [`Pattern`] variant: recognizing it here,
/// after the pattern has already been converted, is what lets a fragment
/// reference compose with the selector's existing grammar with no change to
/// [`crate::pattern::convert`] or to the selector crate itself.
fn fragment_ref_name(pattern: &Pattern) -> Option<String> {
    let Pattern::List {
        delimiter: Delimiter::Paren,
        prefixes,
        before,
        rest: None,
        after,
    } = pattern
    else {
        return None;
    };
    if !prefixes.is_empty() || !after.is_empty() {
        return None;
    }
    let [
        Pattern::Atom {
            text: head,
            prefixes: head_prefixes,
        },
        Pattern::Atom {
            text: name,
            prefixes: name_prefixes,
        },
    ] = before.as_slice()
    else {
        return None;
    };
    if head != ":fragment" || !head_prefixes.is_empty() || !name_prefixes.is_empty() {
        return None;
    }
    Some(name.clone())
}

/// Resolves `name` to its fully-substituted pattern, using and populating the
/// shared `resolved` cache so a fragment referenced from several places (or
/// referenced by another fragment) is only ever walked once.
///
/// `visiting` is the chain of fragment names currently being resolved, which
/// is what turns a fragment cycle into [`RulesetError::FragmentCycle`]
/// instead of infinite recursion.
fn resolve_named_fragment(
    name: &str,
    raw: &[(String, Pattern)],
    resolved: &mut BTreeMap<String, Pattern>,
    visiting: &mut Vec<String>,
) -> Result<Pattern, RulesetError> {
    if let Some(cached) = resolved.get(name) {
        return Ok(cached.clone());
    }
    if visiting.iter().any(|seen| seen == name) {
        let mut chain = visiting.clone();
        chain.push(name.to_owned());
        return Err(RulesetError::FragmentCycle { chain });
    }
    let Some((_, raw_pattern)) = raw.iter().find(|(candidate, _)| candidate == name) else {
        // Only reached when a caller asks to force-resolve a name that was
        // never defined; `resolve_pattern` itself already checks
        // definedness before ever calling this.
        return Err(RulesetError::UndefinedFragment {
            referrer: format!("defpattern {name}"),
            fragment: name.to_owned(),
        });
    };

    visiting.push(name.to_owned());
    let value = resolve_pattern(
        &format!("defpattern {name}"),
        raw_pattern,
        raw,
        resolved,
        visiting,
    )?;
    visiting.pop();
    resolved.insert(name.to_owned(), value.clone());
    Ok(value)
}

/// Walks `pattern`, substituting every `(:fragment name)` node it contains.
///
/// `referrer` names whoever owns `pattern`, for the error a reference to an
/// undefined fragment carries — `defrule "r"` or `defpattern "f"`.
fn resolve_pattern(
    referrer: &str,
    pattern: &Pattern,
    raw: &[(String, Pattern)],
    resolved: &mut BTreeMap<String, Pattern>,
    visiting: &mut Vec<String>,
) -> Result<Pattern, RulesetError> {
    if let Some(name) = fragment_ref_name(pattern) {
        if raw.iter().any(|(candidate, _)| *candidate == name) {
            return resolve_named_fragment(&name, raw, resolved, visiting);
        }
        return Err(RulesetError::UndefinedFragment {
            referrer: referrer.to_owned(),
            fragment: name,
        });
    }
    match pattern {
        Pattern::Atom { .. } | Pattern::Wildcard { .. } => Ok(pattern.clone()),
        Pattern::List {
            delimiter,
            prefixes,
            before,
            rest,
            after,
        } => Ok(Pattern::List {
            delimiter: *delimiter,
            prefixes: prefixes.clone(),
            before: before
                .iter()
                .map(|child| resolve_pattern(referrer, child, raw, resolved, visiting))
                .collect::<Result<_, _>>()?,
            rest: rest.clone(),
            after: after
                .iter()
                .map(|child| resolve_pattern(referrer, child, raw, resolved, visiting))
                .collect::<Result<_, _>>()?,
        }),
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

/// Whether `pattern` contains an unresolved `(:fragment name)` reference
/// anywhere in it.
///
/// [`read_defrule`] uses this to defer its unbound-fix-variable check to
/// [`Ruleset::resolve_fragments`] for exactly the rules that need it: a
/// fragment can bind captures `read_defrule` cannot see yet (the fragment
/// may be defined in a file not read until later), so checking here would
/// reject a rule whose `:fix` correctly names a variable the fragment binds.
/// A rule with no fragment reference is checked immediately, unchanged from
/// before this existed.
fn contains_fragment_ref(pattern: &Pattern) -> bool {
    if fragment_ref_name(pattern).is_some() {
        return true;
    }
    match pattern {
        Pattern::Atom { .. } | Pattern::Wildcard { .. } => false,
        Pattern::List { before, after, .. } => {
            before.iter().any(contains_fragment_ref) || after.iter().any(contains_fragment_ref)
        }
    }
}

/// The dialects a `:dialects (…)` clause names, or every dialect (an empty
/// `Vec`) when the rule states none. See the module documentation's
/// `:dialects` section.
fn dialects_of(rule: &str, form: &ExpressionView) -> Result<Vec<Dialect>, RulesetError> {
    let Some(view) = option(form, ":dialects") else {
        return Ok(Vec::new());
    };
    view.children
        .iter()
        .map(|child| {
            let label = atom_text(child).unwrap_or_default();
            label
                .parse::<Dialect>()
                .ok()
                .filter(|dialect| *dialect != Dialect::Unknown)
                .ok_or_else(|| RulesetError::UnknownDialect {
                    rule: rule.to_owned(),
                    dialect: label.to_owned(),
                    valid: Dialect::ALL
                        .iter()
                        .map(|dialect| dialect.label())
                        .collect::<Vec<_>>()
                        .join(", "),
                })
        })
        .collect()
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
        //
        // Skipped when the pattern references a fragment: see
        // `contains_fragment_ref`.
        if !contains_fragment_ref(&pattern) {
            for variable in fix.capture_names() {
                if !bound.contains(&variable) {
                    return Err(RulesetError::UnboundFixVariable {
                        rule: name.clone(),
                        variable,
                    });
                }
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
        dialects: dialects_of(&name, form)?,
        name,
    })
}

/// Reads one `(defpattern name <pattern-form>)`.
///
/// Registers a named pattern fragment; nothing runs a `defpattern` by
/// itself. See the module documentation's `defpattern` section and
/// [`Ruleset::resolve_fragments`], which substitutes every `(:fragment
/// name)` reference to it before any rule ever matches.
fn read_defpattern(form: &ExpressionView) -> Result<(String, Pattern), RulesetError> {
    let name = form
        .children
        .get(1)
        .and_then(atom_text)
        .ok_or_else(|| RulesetError::MissingName {
            form: "defpattern".to_owned(),
        })?
        .to_owned();

    let body = form
        .children
        .get(2)
        .ok_or_else(|| RulesetError::MissingFragmentBody { name: name.clone() })?;
    let pattern = from_view(body).map_err(|source| RulesetError::InvalidFragment {
        name: name.clone(),
        source,
    })?;
    Ok((name, pattern))
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
        // `deprecate` has no `:dialects` clause of its own; a call to the
        // deprecated name is worth flagging in every dialect it could appear
        // in.
        dialects: Vec::new(),
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
        } else if symbol_is(head, "defpattern") {
            ruleset.fragments.push(read_defpattern(form)?);
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

    // -----------------------------------------------------------------
    // FR-E12: `:dialects` scopes a `defrule` to specific dialects.
    // -----------------------------------------------------------------

    #[test]
    fn a_rule_naming_no_dialects_covers_every_dialect() {
        let ruleset = parse(r#"(defrule r :pattern (f) :message "m")"#);
        let rule = &ruleset.rules[0];
        assert!(rule.dialects.is_empty());
        assert!(rule.covers(Dialect::CommonLisp));
        assert!(rule.covers(Dialect::Clojure));
    }

    #[test]
    fn a_dialect_scope_admits_only_the_dialects_it_names() {
        let ruleset =
            parse(r#"(defrule r :dialects (common-lisp emacs-lisp) :pattern (f) :message "m")"#);
        let rule = &ruleset.rules[0];
        assert!(rule.covers(Dialect::CommonLisp));
        assert!(rule.covers(Dialect::EmacsLisp));
        assert!(!rule.covers(Dialect::Clojure));
    }

    #[test]
    fn an_unknown_dialect_is_rejected_and_lists_the_known_ones() {
        let error = parse_ruleset(
            "r.lisp",
            r#"(defrule r :dialects (perl) :pattern (f) :message "m")"#,
        )
        .expect_err("unknown dialect");
        let RulesetError::UnknownDialect { rule, valid, .. } = error else {
            panic!("expected an unknown-dialect error");
        };
        assert_eq!(rule, "r");
        assert!(valid.contains("common-lisp"));
    }

    #[test]
    fn a_deprecation_still_covers_every_dialect() {
        let ruleset = parse("(deprecate old-fn)");
        assert!(ruleset.rules[0].dialects.is_empty());
    }

    // -----------------------------------------------------------------
    // FR-E14: `defpattern` fragments, referenced with `(:fragment name)`.
    // -----------------------------------------------------------------

    #[test]
    fn a_rule_pattern_resolves_a_referenced_fragment() {
        let mut ruleset = parse(
            r#"(defpattern bare-print (print ?x))
               (defrule r :pattern (progn (:fragment bare-print)) :message "m")"#,
        );
        ruleset.resolve_fragments().expect("resolves");
        let rule = &ruleset.rules[0];
        // The fragment's own capture is now the rule's capture too.
        assert_eq!(rule.pattern.capture_names(), vec!["x".to_owned()]);
        assert!(
            matches!(&rule.pattern, Pattern::List { before, .. } if matches!(
                &before[1],
                Pattern::List { before: inner, .. } if matches!(&inner[0], Pattern::Atom { text, .. } if text == "print")
            ))
        );
    }

    #[test]
    fn a_fix_may_name_a_variable_only_a_fragment_binds() {
        // `read_defrule`'s immediate check cannot see this binding (the
        // fragment is not substituted yet); it must not reject it, and
        // `resolve_fragments` must accept it once the fragment is in.
        let mut ruleset = parse(
            r#"(defpattern captures-x (print ?x))
               (defrule r :pattern (:fragment captures-x) :message "m" :fix (princ ?x))"#,
        );
        ruleset.resolve_fragments().expect("resolves");
        assert!(ruleset.rules[0].fix.is_some());
    }

    #[test]
    fn a_fragment_may_reference_another_fragment() {
        let mut ruleset = parse(
            r#"(defpattern inner (print ?x))
               (defpattern outer (progn (:fragment inner)))
               (defrule r :pattern (:fragment outer) :message "m")"#,
        );
        ruleset.resolve_fragments().expect("resolves");
        assert_eq!(
            ruleset.rules[0].pattern.capture_names(),
            vec!["x".to_owned()]
        );
    }

    #[test]
    fn a_rule_referencing_an_undefined_fragment_is_rejected() {
        let mut ruleset = parse(r#"(defrule r :pattern (:fragment ghost) :message "m")"#);
        let error = ruleset.resolve_fragments().expect_err("undefined fragment");
        assert_eq!(
            error,
            RulesetError::UndefinedFragment {
                referrer: "defrule r".to_owned(),
                fragment: "ghost".to_owned(),
            }
        );
    }

    #[test]
    fn a_fragment_referencing_itself_is_a_cycle() {
        let mut ruleset = parse(r#"(defpattern loopy (progn (:fragment loopy)))"#);
        let error = ruleset.resolve_fragments().expect_err("self cycle");
        assert!(matches!(error, RulesetError::FragmentCycle { .. }));
    }

    #[test]
    fn two_fragments_referencing_each_other_are_a_cycle() {
        let mut ruleset = parse(
            r#"(defpattern a (progn (:fragment b)))
               (defpattern b (progn (:fragment a)))"#,
        );
        let error = ruleset.resolve_fragments().expect_err("mutual cycle");
        assert!(matches!(error, RulesetError::FragmentCycle { .. }));
    }

    #[test]
    fn an_unreferenced_fragment_cycle_is_still_caught() {
        // No rule ever uses `loopy`; the cycle is still a load-time error,
        // not something a project only discovers once it writes a rule that
        // uses the fragment.
        let mut ruleset = parse(
            r#"(defpattern loopy (progn (:fragment loopy)))
               (defrule r :pattern (f) :message "m")"#,
        );
        assert!(ruleset.resolve_fragments().is_err());
    }

    // -----------------------------------------------------------------
    // FR-E17: two custom rules sharing a name are rejected.
    // -----------------------------------------------------------------

    #[test]
    fn two_custom_rules_of_one_name_in_one_file_are_rejected() {
        let ruleset = parse(
            r#"(defrule dup :pattern (f) :message "m")
               (defrule dup :pattern (g) :message "m")"#,
        );
        assert_eq!(
            ruleset.validate(&[]),
            Err(RulesetError::DuplicateName {
                name: "dup".to_owned(),
            })
        );
    }

    #[test]
    fn two_custom_rules_of_one_name_across_files_are_rejected() {
        let mut first = parse(r#"(defrule dup :pattern (f) :message "m")"#);
        first.extend(parse(r#"(defrule dup :pattern (g) :message "m")"#));
        assert_eq!(
            first.validate(&[]),
            Err(RulesetError::DuplicateName {
                name: "dup".to_owned(),
            })
        );
    }
}
