//! The S-expression pattern language behind `--query`.
//!
//! A pattern is written in the source language it matches, with three tokens
//! given meaning:
//!
//! | token | meaning |
//! | --- | --- |
//! | `_` | one form of any shape |
//! | `?name` | one form of any shape, bound to `name` |
//! | `...` | zero or more forms, at most once per list |
//!
//! `?name` may carry a kind: `?name:list`, `?name:atom`, `?name:symbol`,
//! `?name:keyword`, `?name:string`, `?name:number`. The anonymous `_` takes
//! the same suffix (`_:list`). A rest is spelled `...`, or `?name...` to bind
//! the run of forms it swallowed.
//!
//! Everything else is a literal and matches itself: `(defun ?name ...)` finds
//! every `defun`, `(if ?test ?then)` finds two-branch `if`s and not
//! three-branch ones, `(let ((?var ?init)) ...)` finds single-binding `let`s.
//! Repeating a name constrains it — `(eq ?x ?x)` finds self-comparisons.
//!
//! # The pattern is read by the source reader
//!
//! `parse` runs the dialect's own reader over the pattern text rather than a
//! bespoke tokenizer. That is what makes `--query "(format t \"~a\" ?x)"`,
//! `--query "#'?fn"`, and `--query "'(a ...)"` behave the way the same text
//! behaves in a file: strings, character literals, reader prefixes, and
//! bracket dialects are already handled, exactly once, in one place.
//!
//! The cost is that the three special tokens have to be *valid atoms* in every
//! dialect, which `_`, `?name` and `...` are.

use crate::dialect::Dialect;
use crate::sexpr::{Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree};

use super::error::PatternError;

/// How deep a pattern may nest.
///
/// Matching walks a pattern recursively, so an adversarial `--query` of ten
/// thousand open parens would otherwise be a stack overflow. Real patterns are
/// a handful of levels deep; this is three orders of magnitude above them.
const MAX_PATTERN_DEPTH: usize = 64;

/// A constraint on what a wildcard may bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    /// Any single form.
    Any,
    /// Any atom, including strings, numbers and keywords.
    Atom,
    /// Any list, whatever its delimiter.
    List,
    /// An atom that is neither a string, a number, nor a keyword.
    Symbol,
    /// An atom beginning with `:` (Common Lisp) or `:`/`::` (Clojure).
    Keyword,
    /// A `"…"` string literal.
    String,
    /// An atom that reads as a decimal, ratio, or floating-point number.
    Number,
}

impl CaptureKind {
    const NAMES: [(&'static str, Self); 7] = [
        ("any", Self::Any),
        ("form", Self::Any),
        ("atom", Self::Atom),
        ("list", Self::List),
        ("symbol", Self::Symbol),
        ("keyword", Self::Keyword),
        ("string", Self::String),
    ];

    fn parse(name: &str) -> Option<Self> {
        if name == "number" {
            return Some(Self::Number);
        }
        Self::NAMES
            .iter()
            .find(|(label, _)| *label == name)
            .map(|(_, kind)| *kind)
    }

    fn expected() -> String {
        "any, form, atom, list, symbol, keyword, string, number".to_owned()
    }

    /// Whether `view` satisfies this constraint.
    #[must_use]
    pub fn accepts(self, view: &ExpressionView) -> bool {
        let atom_text = || {
            (view.kind == ExpressionKind::Atom)
                .then(|| view.text.as_deref().unwrap_or_default())
                .map(|text| &text[view.symbol_offset.min(text.len())..])
        };
        match self {
            Self::Any => true,
            Self::List => view.kind == ExpressionKind::List,
            Self::Atom => view.kind == ExpressionKind::Atom,
            Self::String => atom_text().is_some_and(|text| text.starts_with('"')),
            Self::Keyword => atom_text().is_some_and(|text| text.starts_with(':')),
            Self::Number => atom_text().is_some_and(is_number_literal),
            Self::Symbol => atom_text().is_some_and(|text| {
                !text.starts_with('"') && !text.starts_with(':') && !is_number_literal(text)
            }),
        }
    }

    /// The spelling this kind is written with in a pattern.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Atom => "atom",
            Self::List => "list",
            Self::Symbol => "symbol",
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Number => "number",
        }
    }
}

/// Whether an atom reads as a number in any of the supported dialects.
///
/// Deliberately syntactic: `1`, `-2.5`, `1/3`, `1e10`, `#xff` is not (the
/// reader keeps the dispatch prefix on the atom, and a radix-prefixed literal
/// is rare enough in a selector that treating it as a symbol is the safer
/// default than a half-right radix parser).
fn is_number_literal(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty() {
        return false;
    }
    let mut seen_digit = false;
    let mut seen_separator = false;
    let mut previous_exponent = false;
    for (index, character) in body.char_indices() {
        match character {
            '0'..='9' => {
                seen_digit = true;
                previous_exponent = false;
            }
            '.' | '/' if !seen_separator && seen_digit => {
                seen_separator = true;
                previous_exponent = false;
            }
            'e' | 'E' | 'd' | 'D' | 's' | 'S' | 'f' | 'F' | 'l' | 'L'
                if seen_digit && index + 1 < body.len() =>
            {
                previous_exponent = true;
            }
            '+' | '-' if previous_exponent => previous_exponent = false,
            _ => return false,
        }
    }
    seen_digit
}

/// A rest position inside a list pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rest {
    /// The name `?name...` binds, or `None` for a bare `...`.
    pub capture: Option<String>,
}

/// One node of a parsed pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// `_`, `?name`, or either with a kind suffix.
    Wildcard {
        capture: Option<String>,
        kind: CaptureKind,
        /// Reader prefixes the matched form must carry.
        ///
        /// Empty means "any", not "none": a bare `?x` matches `'a` as
        /// readily as `a`, because a caller who did not mention prefixes did
        /// not mean to exclude them. Writing `#'?fn` constrains instead —
        /// which is the whole reason to write it.
        ///
        /// Literal atoms and lists are strict in both directions: `foo` does
        /// not match `'foo`, because a quoted symbol is data where a bare one
        /// is a reference, and a query that conflated them would be wrong in
        /// the direction that matters.
        prefixes: Vec<ReaderPrefix>,
    },
    /// A literal atom, compared with the dialect's symbol rules.
    Atom {
        /// The atom text with its reader prefixes stripped.
        text: String,
        prefixes: Vec<ReaderPrefix>,
    },
    /// A list, matched positionally around at most one [`Rest`].
    List {
        delimiter: Delimiter,
        prefixes: Vec<ReaderPrefix>,
        /// Sub-patterns before the rest, matched left to right.
        before: Vec<Pattern>,
        rest: Option<Rest>,
        /// Sub-patterns after the rest, matched right to left.
        after: Vec<Pattern>,
    },
}

impl Pattern {
    /// Parses pattern text with `dialect`'s reader.
    pub fn parse(text: &str, dialect: Dialect) -> Result<Self, PatternError> {
        if text.trim().is_empty() {
            return Err(PatternError::Empty);
        }

        let tree = SyntaxTree::parse_with_dialect(text, dialect).map_err(|source| {
            PatternError::Malformed {
                detail: source.to_string(),
            }
        })?;
        let root = tree.root_view();
        let [form] = root.children.as_slice() else {
            return Err(PatternError::NotOneForm {
                count: root.children.len(),
            });
        };

        let mut kinds = Vec::new();
        convert(form, 0, &mut kinds)
    }

    /// Every capture name this pattern binds, in first-appearance order.
    #[must_use]
    pub fn capture_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut pending = vec![self];
        while let Some(pattern) = pending.pop() {
            match pattern {
                Self::Wildcard {
                    capture: Some(name),
                    ..
                } => push_unique(&mut names, name),
                Self::Wildcard { .. } | Self::Atom { .. } => {}
                Self::List {
                    before,
                    rest,
                    after,
                    ..
                } => {
                    if let Some(Rest {
                        capture: Some(name),
                    }) = rest
                    {
                        push_unique(&mut names, name);
                    }
                    pending.extend(after.iter().rev());
                    pending.extend(before.iter().rev());
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }
}

fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|existing| existing == name) {
        names.push(name.to_owned());
    }
}

/// The token a pattern atom carries, once its special spellings are read off.
///
/// Visible to [`super::rewrite`] so a `--rewrite` template reads `?name`,
/// `?name:kind` and `?name...` with the tokenizer the `--query` pattern uses.
/// Two copies of this grammar would drift, and the drift would be a template
/// that silently writes a literal `?name` into the source.
pub(super) enum AtomToken {
    Wildcard {
        capture: Option<String>,
        kind: CaptureKind,
    },
    Rest(Rest),
    Literal,
}

pub(super) fn classify_atom(text: &str) -> Result<AtomToken, PatternError> {
    if text == "_" {
        return Ok(AtomToken::Wildcard {
            capture: None,
            kind: CaptureKind::Any,
        });
    }
    if text == "..." {
        return Ok(AtomToken::Rest(Rest { capture: None }));
    }
    if let Some(rest) = text.strip_prefix('_').and_then(|it| it.strip_prefix(':')) {
        let kind = CaptureKind::parse(rest).ok_or_else(|| PatternError::UnknownCaptureKind {
            kind: rest.to_owned(),
            token: text.to_owned(),
            expected: CaptureKind::expected(),
        })?;
        return Ok(AtomToken::Wildcard {
            capture: None,
            kind,
        });
    }

    let Some(body) = text.strip_prefix('?') else {
        return Ok(AtomToken::Literal);
    };

    if let Some(name) = body.strip_suffix("...") {
        if name.is_empty() {
            return Err(PatternError::EmptyCaptureName {
                token: text.to_owned(),
            });
        }
        return Ok(AtomToken::Rest(Rest {
            capture: Some(name.to_owned()),
        }));
    }

    let (name, kind) = match body.split_once(':') {
        Some((name, kind)) => (
            name,
            CaptureKind::parse(kind).ok_or_else(|| PatternError::UnknownCaptureKind {
                kind: kind.to_owned(),
                token: text.to_owned(),
                expected: CaptureKind::expected(),
            })?,
        ),
        None => (body, CaptureKind::Any),
    };
    if name.is_empty() {
        return Err(PatternError::EmptyCaptureName {
            token: text.to_owned(),
        });
    }
    Ok(AtomToken::Wildcard {
        capture: Some(name.to_owned()),
        kind,
    })
}

fn convert(
    view: &ExpressionView,
    depth: usize,
    kinds: &mut Vec<(String, CaptureKind)>,
) -> Result<Pattern, PatternError> {
    if depth > MAX_PATTERN_DEPTH {
        return Err(PatternError::TooDeep {
            depth,
            limit: MAX_PATTERN_DEPTH,
        });
    }

    match view.kind {
        ExpressionKind::Root => Err(PatternError::NotOneForm {
            count: view.children.len(),
        }),
        ExpressionKind::Atom => {
            let text = view.text.as_deref().unwrap_or_default();
            let symbol = &text[view.symbol_offset.min(text.len())..];
            match classify_atom(symbol)? {
                AtomToken::Wildcard { capture, kind } => {
                    if let Some(name) = &capture {
                        record_kind(kinds, name, kind)?;
                    }
                    Ok(Pattern::Wildcard {
                        capture,
                        kind,
                        prefixes: view.reader_prefixes.clone(),
                    })
                }
                AtomToken::Rest(_) if view.reader_prefixes.is_empty() => {
                    Err(PatternError::TopLevelEllipsis)
                }
                // `#'...` is not a rest -- a prefix cannot apply to a run of
                // forms -- so it falls back to the literal atom `...`.
                AtomToken::Rest(_) | AtomToken::Literal => Ok(Pattern::Atom {
                    text: symbol.to_owned(),
                    prefixes: view.reader_prefixes.clone(),
                }),
            }
        }
        ExpressionKind::List => {
            let mut before = Vec::new();
            let mut after = Vec::new();
            let mut rest = None;
            let mut ellipsis_count = 0usize;

            for child in &view.children {
                let token = (child.kind == ExpressionKind::Atom
                    && child.reader_prefixes.is_empty())
                .then(|| {
                    let text = child.text.as_deref().unwrap_or_default();
                    classify_atom(&text[child.symbol_offset.min(text.len())..])
                })
                .transpose()?;
                if let Some(AtomToken::Rest(marker)) = token {
                    ellipsis_count += 1;
                    if let Some(name) = &marker.capture {
                        record_kind(kinds, name, CaptureKind::Any)?;
                    }
                    rest = Some(marker);
                    continue;
                }
                let converted = convert(child, depth + 1, kinds)?;
                if rest.is_some() {
                    after.push(converted);
                } else {
                    before.push(converted);
                }
            }

            if ellipsis_count > 1 {
                return Err(PatternError::MultipleEllipses {
                    count: ellipsis_count,
                });
            }

            Ok(Pattern::List {
                delimiter: view.delimiter.unwrap_or(Delimiter::Paren),
                prefixes: view.reader_prefixes.clone(),
                before,
                rest,
                after,
            })
        }
    }
}

fn record_kind(
    kinds: &mut Vec<(String, CaptureKind)>,
    name: &str,
    kind: CaptureKind,
) -> Result<(), PatternError> {
    match kinds.iter().find(|(existing, _)| existing == name) {
        Some((_, existing)) if *existing != kind => Err(PatternError::ConflictingCaptureKind {
            name: name.to_owned(),
        }),
        Some(_) => Ok(()),
        None => {
            kinds.push((name.to_owned(), kind));
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Pattern {
        Pattern::parse(text, Dialect::CommonLisp).unwrap()
    }

    #[test]
    fn a_definition_pattern_reads_head_capture_and_rest() {
        let Pattern::List {
            before,
            rest,
            after,
            ..
        } = parse("(defun ?name ...)")
        else {
            panic!("expected a list pattern");
        };
        assert_eq!(before.len(), 2);
        assert_eq!(after.len(), 0);
        assert_eq!(rest, Some(Rest { capture: None }));
        assert!(matches!(&before[0], Pattern::Atom { text, .. } if text == "defun"));
        assert!(matches!(
            &before[1],
            Pattern::Wildcard {
                capture: Some(name),
                kind: CaptureKind::Any,
                ..
            } if name == "name"
        ));
    }

    #[test]
    fn a_rest_may_sit_in_the_middle_and_split_the_children() {
        let Pattern::List { before, after, .. } = parse("(defun ?name ... ?last)") else {
            panic!("expected a list pattern");
        };
        assert_eq!(before.len(), 2);
        assert_eq!(after.len(), 1);
    }

    #[test]
    fn capture_kinds_parse_and_are_reported() {
        let pattern = parse("(let ((?var:symbol ?init:list)) ?body...)");
        assert_eq!(
            pattern.capture_names(),
            vec!["body".to_owned(), "init".to_owned(), "var".to_owned()]
        );
    }

    #[test]
    fn one_name_cannot_carry_two_kinds() {
        let error = Pattern::parse("(f ?x:list ?x:atom)", Dialect::CommonLisp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "capture `?x` is bound twice with different kinds"
        );
    }

    #[test]
    fn two_rests_in_one_list_are_refused_as_ambiguous() {
        let error = Pattern::parse("(f ... ...)", Dialect::CommonLisp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "a list pattern may hold at most one `...`; found 2"
        );
    }

    #[test]
    fn an_unknown_kind_names_the_ones_that_exist() {
        let error = Pattern::parse("(f ?x:lst)", Dialect::CommonLisp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown capture kind `lst` in `?x:lst`: expected one of \
             any, form, atom, list, symbol, keyword, string, number"
        );
    }

    #[test]
    fn a_malformed_pattern_carries_the_readers_own_message() {
        let error = Pattern::parse("(defun", Dialect::CommonLisp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "pattern does not read as an S-expression: unclosed list starting at byte 0"
        );
    }

    #[test]
    fn a_pattern_must_be_exactly_one_form() {
        let error = Pattern::parse("(a) (b)", Dialect::CommonLisp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "pattern has 2 top-level forms; a pattern must be exactly one form"
        );
        assert_eq!(
            Pattern::parse("  ", Dialect::CommonLisp)
                .unwrap_err()
                .to_string(),
            "pattern is empty"
        );
    }

    #[test]
    fn a_bare_ellipsis_is_meaningless_outside_a_list() {
        let error = Pattern::parse("...", Dialect::CommonLisp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "`...` is only meaningful inside a list pattern"
        );
    }

    #[test]
    fn reader_prefixes_are_part_of_the_pattern() {
        let Pattern::Atom { text, prefixes } = parse("#'handler") else {
            panic!("expected an atom pattern");
        };
        assert_eq!(text, "handler");
        assert_eq!(prefixes, vec![ReaderPrefix::Function]);
    }

    #[test]
    fn number_recognition_covers_the_spellings_a_selector_meets() {
        for text in ["1", "-2.5", "+3", "1/3", "1e10", "1.0d0"] {
            assert!(is_number_literal(text), "{text} should read as a number");
        }
        for text in ["", "-", "x", "1x", "a1", ":1", "1.2.3"] {
            assert!(
                !is_number_literal(text),
                "{text} should not read as a number"
            );
        }
    }
}
