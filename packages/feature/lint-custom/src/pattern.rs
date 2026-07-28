//! The S-expression pattern language custom rules match with.
//!
//! A pattern is an S-expression in which three spellings are special:
//!
//! | Spelling | Matches |
//! | --- | --- |
//! | `?name` | one form, and binds it; a repeated `?name` must match the same form |
//! | `?_` | one form, binding nothing (so two `?_` need not agree) |
//! | `...` | the rest of the enclosing list, however many forms |
//!
//! Everything else matches itself: a symbol case- and package-insensitively (so
//! `(cl:print x)` matches `(print ?x)`), a string or number exactly.
//!
//! ## Why binding-by-repetition
//!
//! `(setf ?place ?place)` is a self-assignment and `(setf ?a ?b)` is not, and
//! the *only* difference is whether the same name appears twice. Making
//! repetition mean equality is what lets a project express that class of rule —
//! which is most of the interesting ones — without a `:where` clause or a
//! predicate language.
//!
//! ## Why `...` is only a tail
//!
//! `(defentity ?name ... :table ?t)` would need backtracking, and backtracking
//! turns "does this pattern match?" from a question with an obvious answer into
//! one with a performance story. A tail ellipsis is enough for the rules people
//! write and costs one length comparison.

use std::collections::BTreeMap;

use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, symbol_is};

/// One node of a pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// A literal that matches itself.
    Literal(String),
    /// `?name`: one form, bound to `name`.
    Variable(String),
    /// `?_`: one form, bound to nothing.
    Wildcard,
    /// A list, with `ellipsis` set when it ended in `...`.
    List {
        delimiter: Delimiter,
        items: Vec<Pattern>,
        ellipsis: bool,
    },
}

/// The forms a successful match bound, keyed by variable name.
///
/// A `BTreeMap` rather than a `HashMap`: a fix template substitutes these in
/// source order and a report may list them, and neither should depend on hash
/// iteration order.
pub type Bindings = BTreeMap<String, String>;

/// Reads a parsed form as a pattern.
///
/// The pattern language has no syntax of its own — it is read from the same
/// parser as the code it matches — so a malformed pattern is a malformed
/// S-expression, which the reader has already rejected by the time this runs.
#[must_use]
pub fn from_view(view: &ExpressionView) -> Pattern {
    if let Some(text) = atom_text(view) {
        if text == "?_" {
            return Pattern::Wildcard;
        }
        if let Some(name) = text.strip_prefix('?') {
            if !name.is_empty() {
                return Pattern::Variable(name.to_owned());
            }
        }
        return Pattern::Literal(text.to_owned());
    }
    if view.kind != ExpressionKind::List {
        return Pattern::Literal(view.text.clone().unwrap_or_default());
    }

    let mut items: Vec<Pattern> = view.children.iter().map(from_view).collect();
    let ellipsis = matches!(items.last(), Some(Pattern::Literal(text)) if text == "...");
    if ellipsis {
        items.pop();
    }
    Pattern::List {
        delimiter: view.delimiter.unwrap_or(Delimiter::Paren),
        items,
        ellipsis,
    }
}

/// Whether `view` matches `pattern`, and what it bound if so.
///
/// A pattern with no variables still returns `Some(empty)` on a match, which is
/// why the return type is an `Option<Bindings>` rather than a `bool` plus an
/// out-parameter: "matched, bound nothing" and "did not match" are different
/// answers.
#[must_use]
pub fn match_view(pattern: &Pattern, view: &ExpressionView, source: &str) -> Option<Bindings> {
    let mut bindings = Bindings::new();
    matches_into(pattern, view, source, &mut bindings).then_some(bindings)
}

/// The exact source of a view, or `""` when the span is not a character
/// boundary — which cannot happen for a span that came from the parser.
fn slice<'a>(source: &'a str, view: &ExpressionView) -> &'a str {
    source
        .get(view.span.start().get()..view.span.end().get())
        .unwrap_or_default()
}

fn matches_into(
    pattern: &Pattern,
    view: &ExpressionView,
    source: &str,
    bindings: &mut Bindings,
) -> bool {
    match pattern {
        Pattern::Wildcard => true,
        Pattern::Variable(name) => {
            let text = slice(source, view);
            match bindings.get(name) {
                // A repeated variable must match the same form. Comparing the
                // *reparsed* text would be exact but expensive; comparing the
                // recorded source and then falling back to a case-insensitive
                // comparison covers the spellings that differ only in case.
                Some(previous) => previous == text || previous.eq_ignore_ascii_case(text),
                None => {
                    bindings.insert(name.clone(), text.to_owned());
                    true
                }
            }
        }
        Pattern::Literal(text) => {
            let Some(actual) = atom_text(view) else {
                return false;
            };
            // A string or number is matched exactly; a symbol case- and
            // package-insensitively, matching how the reader treats it.
            if text.starts_with('"') || text.starts_with(|c: char| c.is_ascii_digit()) {
                actual == text
            } else {
                symbol_is(actual, text)
            }
        }
        Pattern::List {
            delimiter,
            items,
            ellipsis,
        } => {
            if view.kind != ExpressionKind::List || view.delimiter != Some(*delimiter) {
                return false;
            }
            let long_enough = if *ellipsis {
                view.children.len() >= items.len()
            } else {
                view.children.len() == items.len()
            };
            if !long_enough {
                return false;
            }
            items
                .iter()
                .zip(&view.children)
                .all(|(item, child)| matches_into(item, child, source, bindings))
        }
    }
}

/// Substitutes `bindings` into a template read as a pattern, producing source
/// text.
///
/// Printed from the pattern rather than copied from the source, because a
/// template is a *new* form: nothing in the file has its text. Bound variables
/// keep their original source verbatim, which is what preserves the formatting
/// of the parts the fix does not change.
///
/// An unbound variable renders as itself (`?x`), which is visible nonsense
/// rather than silent nonsense — and [`super::ruleset`] rejects a template
/// naming a variable the pattern does not bind, so it cannot reach a file.
#[must_use]
pub fn render(template: &Pattern, bindings: &Bindings) -> String {
    match template {
        Pattern::Wildcard => "?_".to_owned(),
        Pattern::Variable(name) => bindings
            .get(name)
            .cloned()
            .unwrap_or_else(|| format!("?{name}")),
        Pattern::Literal(text) => text.clone(),
        Pattern::List {
            delimiter,
            items,
            ellipsis,
        } => {
            let (open, close) = match delimiter {
                Delimiter::Paren => ('(', ')'),
                Delimiter::Bracket => ('[', ']'),
                Delimiter::Brace => ('{', '}'),
            };
            let mut rendered: Vec<String> =
                items.iter().map(|item| render(item, bindings)).collect();
            if *ellipsis {
                rendered.push("...".to_owned());
            }
            format!("{open}{}{close}", rendered.join(" "))
        }
    }
}

/// Every variable a pattern names, in source order.
#[must_use]
pub fn variables(pattern: &Pattern) -> Vec<String> {
    let mut names = Vec::new();
    collect_variables(pattern, &mut names);
    names
}

fn collect_variables(pattern: &Pattern, into: &mut Vec<String>) {
    match pattern {
        Pattern::Variable(name) => {
            if !into.contains(name) {
                into.push(name.clone());
            }
        }
        Pattern::List { items, .. } => {
            for item in items {
                collect_variables(item, into);
            }
        }
        Pattern::Literal(_) | Pattern::Wildcard => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn parse(input: &str) -> (String, ExpressionView) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        (input.to_owned(), view)
    }

    fn pattern(input: &str) -> Pattern {
        from_view(&parse(input).1)
    }

    fn matched(pattern_source: &str, code: &str) -> Option<Bindings> {
        let (text, view) = parse(code);
        match_view(&pattern(pattern_source), &view, &text)
    }

    #[test]
    fn a_literal_pattern_matches_itself() {
        assert!(matched("(print x)", "(print x)").is_some());
        assert!(matched("(print x)", "(print y)").is_none());
    }

    #[test]
    fn a_symbol_literal_matches_case_and_package_insensitively() {
        assert!(matched("(print ?x)", "(PRINT y)").is_some());
        assert!(matched("(print ?x)", "(cl:print y)").is_some());
    }

    #[test]
    fn a_variable_binds_the_form_it_matched() {
        let bindings = matched("(print ?x)", "(print (compute 1))").expect("a match");
        assert_eq!(bindings["x"], "(compute 1)");
    }

    #[test]
    fn a_repeated_variable_requires_the_same_form() {
        assert!(matched("(setf ?p ?p)", "(setf x x)").is_some());
        assert!(matched("(setf ?p ?p)", "(setf x y)").is_none());
        // The reader upcases both, so these are the same place.
        assert!(matched("(setf ?p ?p)", "(setf X x)").is_some());
    }

    #[test]
    fn a_wildcard_binds_nothing_so_two_need_not_agree() {
        assert!(matched("(setf ?_ ?_)", "(setf x y)").is_some());
        let bindings = matched("(print ?_)", "(print x)").expect("a match");
        assert!(bindings.is_empty());
    }

    #[test]
    fn an_ellipsis_matches_the_rest_of_a_list() {
        assert!(matched("(defentity ?name ...)", "(defentity user)").is_some());
        assert!(
            matched(
                "(defentity ?name ...)",
                "(defentity user :table \"t\" :key id)"
            )
            .is_some()
        );
        // But not a list too short for the fixed part.
        assert!(matched("(defentity ?name ...)", "(defentity)").is_none());
    }

    #[test]
    fn without_an_ellipsis_the_length_must_match_exactly() {
        assert!(matched("(print ?x)", "(print x y)").is_none());
        assert!(matched("(print ?x ?y)", "(print x)").is_none());
    }

    #[test]
    fn a_nested_pattern_matches_structurally() {
        let bindings =
            matched("(when (null ?x) ?body)", "(when (null items) (bail))").expect("a match");
        assert_eq!(bindings["x"], "items");
        assert_eq!(bindings["body"], "(bail)");
    }

    #[test]
    fn a_delimiter_is_part_of_the_shape() {
        let tree = SyntaxTree::parse_with_dialect("[a b]", Dialect::Clojure).expect("parse");
        let bracket = tree.select_path(&Path::root_child(0)).expect("form").view();
        let paren = pattern("(a b)");
        assert!(match_view(&paren, &bracket, "[a b]").is_none());
    }

    #[test]
    fn a_string_literal_is_matched_exactly() {
        assert!(matched(r#"(f "a")"#, r#"(f "a")"#).is_some());
        assert!(matched(r#"(f "a")"#, r#"(f "A")"#).is_none());
    }

    #[test]
    fn rendering_substitutes_the_bound_source_verbatim() {
        let bindings = matched("(print ?x)", "(print (compute  1))").expect("a match");
        let template = pattern(r#"(format t "~a~%" ?x)"#);
        assert_eq!(
            render(&template, &bindings),
            r#"(format t "~a~%" (compute  1))"#
        );
    }

    #[test]
    fn rendering_an_unbound_variable_leaves_it_visible() {
        assert_eq!(
            render(&pattern("(f ?missing)"), &Bindings::new()),
            "(f ?missing)"
        );
    }

    #[test]
    fn variables_are_listed_once_in_source_order() {
        assert_eq!(
            variables(&pattern("(f ?b (g ?a ?b) ?_)")),
            vec!["b".to_owned(), "a".to_owned()]
        );
        assert!(variables(&pattern("(f x)")).is_empty());
    }
}
