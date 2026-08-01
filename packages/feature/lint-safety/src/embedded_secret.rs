//! `embedded-secret`: a credential-shaped string literal sitting in source.
//!
//! `(defparameter *api-key* "sk-1234567890abcdefghij")` puts a live credential
//! in version control the moment it is committed, and a credential in version
//! control stays leaked even after the line is deleted — history keeps it.
//!
//! Two independent signals, either one sufficient:
//!
//! - The **value** is shaped like a real credential regardless of what it is
//!   bound to: an AWS access key id (`AKIA`/`ASIA` + 16 uppercase-or-digit
//!   characters) or a recognisable vendor-token prefix (`sk-`, `ghp_`, `gho_`,
//!   `github_pat_`) followed by a long enough tail. These shapes are
//!   vanishingly unlikely to appear by accident, so the name binding them is
//!   not consulted at all.
//! - The **name** is shaped like a secret (`*api-key*`, `password`, `token`,
//!   `*secret*`, …) and the value is a non-trivial string — not `nil`, not
//!   empty, and not one of the placeholder spellings (`CHANGEME`, `TODO`,
//!   `REDACTED`, …) a template or a doc example would use instead of a real
//!   value.
//!
//! Scope, by design: only the definition site is read — `defparameter`,
//! `defvar`, `defconstant`, and `let`/`let*` bindings. A `setf`/`setq` onto an
//! existing name is not covered; the definition is where a literal is
//! actually typed into source, and where a review would look first.
//!
//! Report-only. What to do about a real credential — rotate it, move it to an
//! environment variable, a secrets manager — is not this tool's decision.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "embedded-secret",
    RuleCategory::Suspicious,
    Severity::Error,
    "a definition whose name or literal value is shaped like an embedded credential",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A credential typed directly into source is leaked the moment it is committed, and stays \
         leaked in history even after the line is deleted. This rule looks for two independent \
         signals: a value shaped like a real vendor credential, and a suspiciously-named binding \
         holding a non-placeholder string.",
    )
    .with_example(
        "(defparameter *api-key* \"sk-1234567890abcdefghij\")",
        "(defparameter *api-key* (uiop:getenv \"API_KEY\"))",
    )
    .with_caveat(
        "Only the definition site is read: `defparameter`/`defvar`/`defconstant` and \
         `let`/`let*` bindings. A later `setf`/`setq` onto the same name is not covered.",
    ),
);

const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("defparameter"),
    NormalizedHead::new("defvar"),
    NormalizedHead::new("defconstant"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
];

/// Substrings that mark a name as a credential holder. Matched against the
/// unqualified, lower-cased name, so `*api-key*`, `App:API-Key`, and
/// `access_key` (read loosely) all match the same entry.
const NAME_MARKERS: [&str; 9] = [
    "secret",
    "password",
    "passwd",
    "token",
    "api-key",
    "apikey",
    "access-key",
    "private-key",
    "credential",
];

/// Values that look like a placeholder rather than a real credential, so a
/// documented example or an unfilled template does not get reported. Compared
/// case-insensitively after the surrounding punctuation this rule already
/// strips.
const PLACEHOLDER_VALUES: [&str; 9] = [
    "",
    "todo",
    "fixme",
    "changeme",
    "change-me",
    "xxx",
    "redacted",
    "placeholder",
    "example",
];

/// One embedded-secret finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedSecretItem {
    pub span: ByteSpan,
    pub name: String,
    /// Why this one fired: the value's own shape, or the name plus a
    /// non-placeholder value.
    pub reason: &'static str,
}

/// Whether `name` reads as a credential holder.
#[must_use]
pub fn looks_like_secret_name(name: &str) -> bool {
    let lowered = unqualified(name).trim_matches('*').to_ascii_lowercase();
    NAME_MARKERS.iter().any(|marker| lowered.contains(marker))
}

/// Whether `value` is itself shaped like a real credential, independent of
/// what it is bound to.
#[must_use]
pub fn looks_like_secret_value(value: &str) -> bool {
    looks_like_aws_key(value) || looks_like_vendor_token(value)
}

fn looks_like_aws_key(value: &str) -> bool {
    let Some(rest) = value
        .strip_prefix("AKIA")
        .or_else(|| value.strip_prefix("ASIA"))
    else {
        return false;
    };
    rest.len() == 16
        && rest
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

const VENDOR_PREFIXES: [&str; 4] = ["sk-", "ghp_", "gho_", "github_pat_"];

fn looks_like_vendor_token(value: &str) -> bool {
    VENDOR_PREFIXES.iter().any(|prefix| {
        value
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.len() >= 16 && rest.chars().all(is_token_char))
    })
}

const fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Whether `value` is a placeholder rather than a real secret.
fn is_placeholder(value: &str) -> bool {
    let normalized = value.trim().trim_matches(['<', '>']).to_ascii_lowercase();
    PLACEHOLDER_VALUES.contains(&normalized.as_str()) || value.trim().len() < 6
}

fn string_literal<'a>(view: &ExpressionView, context: &RuleContext<'a>) -> Option<&'a str> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = context.slice(view.span);
    text.strip_prefix('"')?.strip_suffix('"')
}

/// Judges one `(name value)` pair, from either a definition's own two leading
/// arguments or one `let`/`let*` binding.
fn judge(
    name: &str,
    value_view: &ExpressionView,
    context: &RuleContext<'_>,
) -> Option<EmbeddedSecretItem> {
    let value = string_literal(value_view, context)?;

    if looks_like_secret_value(value) {
        return Some(EmbeddedSecretItem {
            span: value_view.span,
            name: name.to_owned(),
            reason: "value is shaped like a real vendor credential",
        });
    }

    if looks_like_secret_name(name) && !is_placeholder(value) {
        return Some(EmbeddedSecretItem {
            span: value_view.span,
            name: name.to_owned(),
            reason: "name marks it as a credential and the value is not a placeholder",
        });
    }

    None
}

/// Every embedded-secret finding in one matched node: a `defparameter`-family
/// definition, or a `let`/`let*` form's own binding list.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Vec<EmbeddedSecretItem> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };

    if symbol_in(head, &["defparameter", "defvar", "defconstant"]) {
        let Some(name_view) = view.children.get(1) else {
            return Vec::new();
        };
        let Some(name) = atom_symbol(name_view) else {
            return Vec::new();
        };
        let Some(value_view) = view.children.get(2) else {
            return Vec::new();
        };
        return judge(name, value_view, context).into_iter().collect();
    }

    if symbol_in(head, &["let", "let*"]) {
        let Some(bindings) = view.children.get(1) else {
            return Vec::new();
        };
        if bindings.kind != ExpressionKind::List {
            return Vec::new();
        }
        let mut found = Vec::new();
        for binding in &bindings.children {
            if binding.kind != ExpressionKind::List {
                continue;
            }
            let Some(name_view) = binding.children.first() else {
                continue;
            };
            let Some(name) = atom_symbol(name_view) else {
                continue;
            };
            let Some(value_view) = binding.children.get(1) else {
                continue;
            };
            found.extend(judge(name, value_view, context));
        }
        return found;
    }

    Vec::new()
}

fn atom_symbol(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for item in examine(view, context) {
            sink.report(
                item.span,
                format!(
                    "{} looks like an embedded secret: {}",
                    item.name, item.reason
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path as FsPath;

    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn findings(input: &str) -> Vec<EmbeddedSecretItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("root form")
            .view();
        let context = RuleContext::new(FsPath::new("t.lisp"), Dialect::CommonLisp, &tree, input);
        examine(&view, &context)
    }

    #[test]
    fn flags_a_name_and_value_both_shaped_like_a_secret() {
        let found = findings(r#"(defparameter *api-key* "sk-1234567890abcdefghij")"#);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "*api-key*");
    }

    #[test]
    fn flags_an_aws_shaped_value_regardless_of_name() {
        let found = findings(r#"(defvar *setting* "AKIAABCDEFGHIJKLMNOP")"#); // gitleaks:allow
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].reason,
            "value is shaped like a real vendor credential"
        );
    }

    #[test]
    fn flags_a_secret_shaped_let_binding() {
        let found = findings(r#"(let ((password "hunter2-not-a-placeholder")) password)"#);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "password");
    }

    #[test]
    fn does_not_flag_an_ordinary_value() {
        assert!(findings(r#"(defparameter *timeout* "30")"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_non_string_value() {
        assert!(findings("(defparameter *api-key* nil)").is_empty());
    }

    #[test]
    fn does_not_flag_a_placeholder_value() {
        assert!(findings(r#"(defparameter *api-key* "CHANGEME")"#).is_empty());
        assert!(findings(r#"(defparameter *secret* "")"#).is_empty());
    }
}
