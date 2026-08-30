//! Reading a `defrule` `:pattern`/`:fix` clause as the `--query` engine's own
//! [`Pattern`]/[`Template`], preserving the two spellings whose meaning
//! predates it.
//!
//! `defrule` used to carry a second, bespoke ~230-line pattern language,
//! entirely independent of [`paredit_core_syntax::selector`]'s. That module's
//! own documentation says a custom lint-rule DSL is the second caller it was
//! written to expect, so this is now a thin front end over it: [`from_view`]
//! converts an already-parsed `:pattern`/`:fix` clause into the selector's
//! `Pattern`/[`Template`] using the selector's own [`classify_atom`]
//! tokenizer for every spelling but two, and every match, rewrite, and
//! rewrite-hazard check downstream ([`super::pass`]) is the selector's own
//! `match_all`, `plan_rewrite`, and [`Template::render`] — unchanged,
//! unduplicated.
//!
//! ## The two spellings kept at their old meaning
//!
//! | spelling | old `defrule` meaning | selector's own meaning | kept as |
//! | --- | --- | --- | --- |
//! | `_` (bare) | the literal symbol `_` | a wildcard, binds nothing | the literal symbol `_` |
//! | `?_` | one form, binds nothing; two need not agree | a *named* capture `_`, so two must agree | one form, binds nothing |
//!
//! A rule written before the pattern languages were unified used `?_` for
//! "match anything here, and do not require repeats to agree" — the whole
//! point of `(setf ?_ ?_)` reporting `(setf x y)` as well as `(setf x x)`.
//! Reading `?_` the way the selector reads every other `?name` would make
//! `?_` a capture spelled `_`, and two of them would then have to agree,
//! silently narrowing every such rule the day this module changed under it.
//! Keeping `?_` anonymous, and reserving bare `_` for the literal symbol
//! `defrule` always read it as, is what the pattern languages diverging
//! "unambiguously" (see the module's own contract test) comes down to in
//! practice: there is exactly one place they cannot simply merge.
//!
//! ## A `...` earlier than the last position
//!
//! The old grammar only ever recognised a *trailing* `...` as "the rest of
//! this list"; anywhere else, `...` was the literal atom `...` (matched only
//! by a source form that itself reads as three dots — vanishingly rare, but a
//! rule that wrote one meant it literally). The selector's own grammar treats
//! any bare `...` as a rest, wherever it sits, splitting the list around it.
//! Preserving the old, narrower reading for a non-trailing bare `...` is what
//! keeps an existing rule matching exactly what it matched before; the new,
//! wider meaning (an unnamed rest in the middle of a list) is reachable under
//! `defrule` only by using the named form `?name...`, which the old grammar
//! never gave any meaning to a rest at all — a genuinely new capability,
//! opted into with new syntax rather than inherited by a spelling that used
//! to mean something else.
//!
//! [`paredit inspect check --paredit-config`](../../../src/presentation/cli/analysis_report/workflow.rs)
//! flags a rule whose `:pattern` or `:fix` still uses this narrower,
//! non-trailing `...` spelling, as a migration nudge toward the clearer named
//! form — not because the old spelling stopped working.

use paredit_core_syntax::selector::error::PatternError;
use paredit_core_syntax::selector::pattern::{
    AtomToken, CaptureKind, MAX_PATTERN_DEPTH, Rest, classify_atom, record_kind,
};
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView};

/// Re-exported so callers that only need the pattern/template *types* do not
/// also have to depend on `paredit_core_syntax::selector` directly.
pub use paredit_core_syntax::selector::pattern::Pattern;
pub use paredit_core_syntax::selector::rewrite::Template;

/// Reads a parsed `:pattern` clause as a [`Pattern`].
///
/// See the module documentation for the two spellings this preserves rather
/// than handing straight to [`Pattern::from_view`].
pub fn from_view(view: &ExpressionView) -> Result<Pattern, PatternError> {
    let mut kinds = Vec::new();
    convert(view, 0, &mut kinds)
}

/// Reads a parsed `:fix` clause as a [`Template`].
///
/// `:fix` templates use the same tokenizer as `:pattern` clauses do (a `?x`
/// in a template and a `?x` in a pattern must agree on what a capture is,
/// or a fix could silently write a literal `?x` into the rewritten file), so
/// this is the selector's own [`Template::from_view`] unchanged — with one
/// consequence worth naming: a `:fix` that writes `?_` is asking to
/// substitute a capture named `_`, which [`from_view`]'s `:pattern` reading
/// never binds (`?_` there is anonymous, per the module documentation), so
/// such a rule is rejected by [`super::ruleset`]'s existing
/// `UnboundFixVariable` check at load time — a clear error, in place of the
/// old behaviour of splicing the literal text `?_` into a file.
pub fn fix_from_view(
    view: &ExpressionView,
    source: &str,
) -> Result<Template, paredit_core_syntax::selector::error::RewriteError> {
    Template::from_view(view, source)
}

/// The atom-token classifier `defrule` reads patterns with: the selector's own
/// [`classify_atom`], with `_` and `?_` intercepted first. Every other
/// spelling — `?name`, `?name:kind`, `...`, `?name...`, reader prefixes — is
/// the selector's tokenizer verbatim, so a spelling added there is available
/// to `defrule` with no change here.
fn atom_token(text: &str) -> Result<AtomToken, PatternError> {
    match text {
        "_" => Ok(AtomToken::Literal),
        "?_" => Ok(AtomToken::Wildcard {
            capture: None,
            kind: CaptureKind::Any,
        }),
        _ => classify_atom(text),
    }
}

/// [`paredit_core_syntax::selector::pattern`]'s own `convert`, with two
/// changes: it calls [`atom_token`] rather than [`classify_atom`] directly,
/// and a list's per-child loop treats a non-trailing bare `...` as the
/// literal atom rather than a mid-list rest (see the module documentation).
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
            match atom_token(symbol)? {
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
            let last_index = view.children.len().checked_sub(1);

            for (index, child) in view.children.iter().enumerate() {
                let token = (child.kind == ExpressionKind::Atom
                    && child.reader_prefixes.is_empty())
                .then(|| {
                    let text = child.text.as_deref().unwrap_or_default();
                    atom_token(&text[child.symbol_offset.min(text.len())..])
                })
                .transpose()?;

                if let Some(AtomToken::Rest(marker)) = token {
                    let is_tail = last_index == Some(index);
                    if marker.capture.is_none() && !is_tail {
                        // The old grammar only ever special-cased a *trailing*
                        // bare `...`; anywhere earlier it was the literal atom
                        // `...`. Preserved so an existing rule keeps matching
                        // exactly what it matched before.
                        push(&mut before, &mut after, &rest, literal_dots());
                        continue;
                    }
                    ellipsis_count += 1;
                    if let Some(name) = &marker.capture {
                        record_kind(kinds, name, CaptureKind::Any)?;
                    }
                    rest = Some(marker);
                    continue;
                }

                let converted = convert(child, depth + 1, kinds)?;
                push(&mut before, &mut after, &rest, converted);
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

fn literal_dots() -> Pattern {
    Pattern::Atom {
        text: "...".to_owned(),
        prefixes: Vec::new(),
    }
}

fn push(before: &mut Vec<Pattern>, after: &mut Vec<Pattern>, rest: &Option<Rest>, item: Pattern) {
    if rest.is_some() {
        after.push(item);
    } else {
        before.push(item);
    }
}

/// Whether a pattern still uses the narrower, non-trailing `...` spelling —
/// [`paredit inspect check --paredit-config`]'s migration signal.
///
/// Detected on the already-converted [`Pattern`] rather than re-walking the
/// source view: `convert`'s literal-atom fallback for this exact case is
/// the only way a bare, unprefixed `Pattern::Atom` with the text `...` can
/// appear in a pattern this module produced, so finding one *is* finding a
/// rule still written in the old style.
#[must_use]
pub fn uses_non_trailing_ellipsis(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Atom { text, prefixes } => text == "..." && prefixes.is_empty(),
        Pattern::Wildcard { .. } => false,
        Pattern::List { before, after, .. } => {
            before.iter().any(uses_non_trailing_ellipsis)
                || after.iter().any(uses_non_trailing_ellipsis)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::selector::rewrite::{RewriteAllowances, apply_plan, plan_rewrite};
    use paredit_core_syntax::selector::{match_all, matcher::PatternMatch};
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
        from_view(&parse(input).1).expect("a readable pattern")
    }

    fn matches(pattern_source: &str, code: &str) -> Vec<PatternMatch> {
        let tree = SyntaxTree::parse_with_dialect(code, Dialect::CommonLisp).expect("parse");
        match_all(&tree, &pattern(pattern_source), Dialect::CommonLisp)
    }

    fn matched_once(pattern_source: &str, code: &str) -> bool {
        matches(pattern_source, code).len() == 1
    }

    #[test]
    fn a_literal_pattern_matches_itself() {
        assert!(matched_once("(print x)", "(print x)"));
        assert!(!matched_once("(print x)", "(print y)"));
    }

    #[test]
    fn a_symbol_literal_matches_case_and_package_insensitively() {
        assert!(matched_once("(print ?x)", "(PRINT y)"));
        assert!(matched_once("(print ?x)", "(cl:print y)"));
    }

    #[test]
    fn a_variable_binds_the_form_it_matched() {
        let found = matches("(print ?x)", "(print (compute 1))");
        assert_eq!(
            found[0].capture("x").expect("capture x").text,
            "(compute 1)"
        );
    }

    #[test]
    fn a_repeated_variable_requires_the_same_form() {
        assert!(matched_once("(setf ?p ?p)", "(setf x x)"));
        assert!(!matched_once("(setf ?p ?p)", "(setf x y)"));
        // The reader upcases both, so these are the same place.
        assert!(matched_once("(setf ?p ?p)", "(setf X x)"));
    }

    #[test]
    fn bare_underscore_is_still_the_literal_symbol() {
        // The selector's own grammar reads a bare `_` as an anonymous
        // wildcard; `defrule` keeps its old, narrower meaning.
        assert!(matched_once("(f _)", "(f _)"));
        assert!(!matched_once("(f _)", "(f anything)"));
    }

    #[test]
    fn a_wildcard_binds_nothing_so_two_need_not_agree() {
        assert!(matched_once("(setf ?_ ?_)", "(setf x y)"));
        let found = matches("(print ?_)", "(print x)");
        assert!(found[0].captures.is_empty());
    }

    #[test]
    fn an_ellipsis_matches_the_rest_of_a_list() {
        assert!(matched_once("(defentity ?name ...)", "(defentity user)"));
        assert!(matched_once(
            "(defentity ?name ...)",
            "(defentity user :table \"t\" :key id)"
        ));
        // But not a list too short for the fixed part.
        assert!(matches("(defentity ?name ...)", "(defentity)").is_empty());
    }

    #[test]
    fn without_an_ellipsis_the_length_must_match_exactly() {
        assert!(matches("(print ?x)", "(print x y)").is_empty());
        assert!(matches("(print ?x ?y)", "(print x)").is_empty());
    }

    #[test]
    fn a_nested_pattern_matches_structurally() {
        let found = matches("(when (null ?x) ?body)", "(when (null items) (bail))");
        assert_eq!(found[0].capture("x").expect("x").text, "items");
        assert_eq!(found[0].capture("body").expect("body").text, "(bail)");
    }

    #[test]
    fn a_delimiter_is_part_of_the_shape() {
        let tree = SyntaxTree::parse_with_dialect("[a b]", Dialect::Clojure).expect("parse");
        let paren = pattern("(a b)");
        assert!(match_all(&tree, &paren, Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_string_literal_is_matched_exactly() {
        assert!(matched_once(r#"(f "a")"#, r#"(f "a")"#));
        assert!(!matched_once(r#"(f "a")"#, r#"(f "A")"#));
    }

    #[test]
    fn a_non_trailing_ellipsis_is_still_the_literal_atom() {
        // Old-style rule: `...` in the middle was never special.
        assert!(matched_once("(?a ... ?b)", "(x ... y)"));
        assert!(matches("(?a ... ?b)", "(x anything y)").is_empty());
        assert!(uses_non_trailing_ellipsis(&pattern("(?a ... ?b)")));
        assert!(!uses_non_trailing_ellipsis(&pattern("(?a ?b ...)")));
    }

    #[test]
    fn a_named_rest_is_the_new_mid_list_capability() {
        // Only reachable by the new, explicit spelling: the old grammar gave
        // `?name...` no rest meaning at all.
        let found = matches("(?a ?mid... ?b)", "(x 1 2 3 y)");
        assert_eq!(found[0].capture("mid").expect("mid").text, "1 2 3");
    }

    #[test]
    fn a_typed_capture_is_available_now_too() {
        assert!(matched_once("(f ?a:number)", "(f 1)"));
        assert!(matches("(f ?a:number)", "(f x)").is_empty());
    }

    #[test]
    fn rendering_substitutes_the_bound_source_verbatim() {
        let tree =
            SyntaxTree::parse_with_dialect("(print (compute  1))", Dialect::CommonLisp).unwrap();
        let found = match_all(&tree, &pattern("(print ?x)"), Dialect::CommonLisp);
        let template_view = parse(r#"(format t "~a~%" ?x)"#).1;
        let template = Template::from_view(&template_view, r#"(format t "~a~%" ?x)"#).unwrap();
        assert_eq!(
            template.render(tree.source(), &found[0].captures),
            r#"(format t "~a~%" (compute  1))"#
        );
    }

    #[test]
    fn a_fix_gains_quote_hazard_detection_for_free() {
        // The one behaviour that could not exist under the old bespoke
        // matcher at all: adopting the selector's `rewrite` module means a
        // `:fix` now refuses to rewrite inside quoted data.
        let tree =
            SyntaxTree::parse_with_dialect("'(a (if x y nil) b)", Dialect::CommonLisp).unwrap();
        let query = pattern("(if ?t ?a nil)");
        let template_view = parse("(when ?t ?a)").1;
        let template = Template::from_view(&template_view, "(when ?t ?a)").unwrap();
        let plan = plan_rewrite(
            &tree,
            &query,
            &template,
            Dialect::CommonLisp,
            RewriteAllowances::default(),
        );
        assert!(plan.is_empty());
        assert_eq!(apply_plan(tree.source(), &plan), "'(a (if x y nil) b)");
    }
}
