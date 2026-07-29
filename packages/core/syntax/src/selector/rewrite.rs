//! The write side of the pattern language: a `--query` pattern plus a
//! `--rewrite` template.
//!
//! [`super::pattern`] answers *which forms have this shape*. This module
//! answers *and what should they become*, which is the other half of a
//! codemod and the thing that turns a report into a transformation.
//!
//! # Rendering is splicing, never re-printing
//!
//! A template is rendered by copying its own source text and swapping each
//! `?name` placeholder for the **verbatim source slice** the capture bound.
//! Nothing is re-serialized: a captured string keeps its escapes, a captured
//! float keeps `1.0d0`, a captured form keeps its internal comments and its
//! line breaks. Printing a parsed value back out is exactly how a rewrite
//! turns `1.0d0` into `1` and a literal newline into the letter `n`, so this
//! module never has a value in hand to print.
//!
//! # What is deliberately refused
//!
//! Two hazards do not announce themselves, because both leave source that
//! still parses:
//!
//! - **Overlapping matches.** The matcher is pre-order and reports a nested
//!   `(let …)` inside an outer one. Rewriting both would splice a replacement
//!   into text that another replacement had already discarded. The plan keeps
//!   the outermost of any overlapping run and counts the rest as skipped —
//!   counted, never silently dropped.
//! - **Comment loss.** A comment sitting inside a matched form but outside
//!   every capture the template uses is deleted by the splice. Since comments
//!   live outside the node tree, the result still parses and the loss is
//!   invisible. Such a match is skipped by default; a caller who means it says
//!   so.
//!
//! Both are reported as [`SkipReason`]s rather than folded into a total, so a
//! caller can say *why* a match it expected to see did not appear.

use crate::dialect::Dialect;
use crate::sexpr::{ByteSpan, ExpressionKind, ExpressionPath, ExpressionView, SyntaxTree};

use super::error::RewriteError;
use super::matcher::{Capture, PatternMatch, match_all};
use super::pattern::{AtomToken, Pattern, classify_atom};

/// One `?name` placeholder in a template, located in the template's own text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hole {
    /// The placeholder's span within [`Template::body`].
    span: ByteSpan,
    /// The capture name, with any `:kind` suffix and trailing `...` removed.
    name: String,
}

/// A parsed `--rewrite` template.
///
/// Holds the template's own text rather than a tree: rendering is a splice, so
/// the text *is* the representation, and keeping it means a template's
/// formatting survives into the rewritten source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// The single form's source text, trimmed of the whitespace around it.
    body: String,
    /// Placeholders in source order, so rendering is one left-to-right pass.
    holes: Vec<Hole>,
}

impl Template {
    /// Parses template text with `dialect`'s reader.
    ///
    /// The same reader the pattern uses, so the two agree on what an atom is:
    /// `?x` inside a string literal is text in both, and `#'?fn` is a
    /// function-quoted placeholder in both.
    pub fn parse(text: &str, dialect: Dialect) -> Result<Self, RewriteError> {
        if text.trim().is_empty() {
            return Err(RewriteError::Empty);
        }
        let tree = SyntaxTree::parse_with_dialect(text, dialect).map_err(|source| {
            RewriteError::Malformed {
                detail: source.to_string(),
            }
        })?;
        let root = tree.root_view();
        let [form] = root.children.as_slice() else {
            return Err(RewriteError::NotOneForm {
                count: root.children.len(),
            });
        };

        let origin = form.span.start().get();
        let body = text[origin..form.span.end().get()].to_owned();
        let mut holes = Vec::new();
        collect_holes(form, origin, &mut holes)?;
        holes.sort_by_key(|hole| hole.span.start().get());
        Ok(Self { body, holes })
    }

    /// Every capture name this template substitutes, in first-appearance
    /// order with duplicates removed.
    #[must_use]
    pub fn capture_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for hole in &self.holes {
            if !names.contains(&hole.name) {
                names.push(hole.name.clone());
            }
        }
        names
    }

    /// The template's source text, for echoing back in a plan.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.body
    }

    /// Checks that every placeholder names a capture `pattern` binds.
    ///
    /// Run once before matching rather than per match: a template naming a
    /// capture the pattern does not bind is a mistake in the *command line*,
    /// and reporting it after a thousand files have been read makes the caller
    /// hunt for which file was at fault when no file was.
    pub fn check_against(&self, pattern: &Pattern) -> Result<(), RewriteError> {
        let bound = pattern.capture_names();
        for name in self.capture_names() {
            if !bound.contains(&name) {
                return Err(RewriteError::UnboundCapture {
                    name,
                    bound: if bound.is_empty() {
                        "nothing".to_owned()
                    } else {
                        bound.join(", ")
                    },
                });
            }
        }
        Ok(())
    }

    /// Renders this template for one match, splicing `source` slices in.
    ///
    /// A capture that bound no form — an empty `?rest...` — renders as
    /// nothing, and takes one run of preceding spaces with it, so
    /// `(f ?rest...)` over `(g)` yields `(f)` rather than `(f )`.
    fn render(&self, source: &str, captures: &[Capture]) -> String {
        let mut out = String::with_capacity(self.body.len());
        let mut cursor = 0usize;
        for hole in &self.holes {
            let bound = captures
                .iter()
                .find(|capture| capture.name == hole.name)
                .and_then(|capture| capture.span)
                .map(|span| &source[span.start().get()..span.end().get()])
                .unwrap_or_default();

            let mut start = hole.span.start().get();
            if bound.is_empty() {
                let bytes = self.body.as_bytes();
                while start > cursor && bytes[start - 1] == b' ' {
                    start -= 1;
                }
            }
            out.push_str(&self.body[cursor..start]);
            out.push_str(bound);
            cursor = hole.span.end().get();
        }
        out.push_str(&self.body[cursor..]);
        out
    }
}

/// Why a match the pattern found produced no replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// An enclosing match was rewritten instead.
    Overlapping,
    /// The rewrite would delete a comment that no capture carries over.
    CommentLoss,
}

impl SkipReason {
    /// The wire spelling, shared by the text and JSON renderings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overlapping => "overlapping",
            Self::CommentLoss => "comment-loss",
        }
    }

    /// One line explaining what the caller can do about it.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Overlapping => {
                "an enclosing match was rewritten; run the command again to reach this one"
            }
            Self::CommentLoss => {
                "a comment inside the match is carried by no capture; \
                 pass --allow-comment-loss to rewrite it anyway"
            }
        }
    }
}

/// One planned rewrite of one form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub path: ExpressionPath,
    pub span: ByteSpan,
    /// The source text being replaced, verbatim.
    pub before: String,
    /// The rendered template.
    pub after: String,
}

/// One match the plan declined to rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedMatch {
    pub path: ExpressionPath,
    pub span: ByteSpan,
    pub reason: SkipReason,
}

/// Every rewrite one pattern and template make over one document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RewritePlan {
    pub replacements: Vec<Replacement>,
    pub skipped: Vec<SkippedMatch>,
    /// Matches whose rendering is byte-identical to what is already there.
    ///
    /// Counted apart from the replacements so an idempotent second run
    /// reports "already in this shape" rather than "nothing matched", which
    /// are different answers to different questions.
    pub unchanged: usize,
}

impl RewritePlan {
    /// Whether applying this plan would change the document.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.replacements.is_empty()
    }

    /// How many skipped matches carry `reason`.
    #[must_use]
    pub fn skipped_for(&self, reason: SkipReason) -> usize {
        self.skipped
            .iter()
            .filter(|skipped| skipped.reason == reason)
            .count()
    }
}

/// Plans every rewrite `pattern` and `template` make over `tree`.
///
/// `allow_comment_loss` opts out of the comment guard described in the module
/// documentation. It defaults to off at every call site in this repository;
/// a caller turning it on is choosing to lose the comments.
#[must_use]
pub fn plan_rewrite(
    tree: &SyntaxTree,
    pattern: &Pattern,
    template: &Template,
    dialect: Dialect,
    allow_comment_loss: bool,
) -> RewritePlan {
    let source = tree.source();
    let used: Vec<String> = template.capture_names();
    let mut plan = RewritePlan::default();
    // Matches arrive sorted by (start, end), so an enclosing match is always
    // seen before the ones nested inside it. Spans in a tree are nested or
    // disjoint, never partially overlapping, which is what makes a single
    // high-water mark a complete overlap test.
    let mut applied_end = 0usize;

    for found in match_all(tree, pattern, dialect) {
        if found.span.start().get() < applied_end {
            plan.skipped.push(SkippedMatch {
                path: found.path,
                span: found.span,
                reason: SkipReason::Overlapping,
            });
            continue;
        }
        if !allow_comment_loss && drops_a_comment(tree, &found, &used) {
            plan.skipped.push(SkippedMatch {
                path: found.path,
                span: found.span,
                reason: SkipReason::CommentLoss,
            });
            continue;
        }

        let before = source[found.span.start().get()..found.span.end().get()].to_owned();
        let after = template.render(source, &found.captures);
        applied_end = found.span.end().get();
        if before == after {
            plan.unchanged += 1;
            continue;
        }
        plan.replacements.push(Replacement {
            path: found.path,
            span: found.span,
            before,
            after,
        });
    }

    plan
}

/// Applies a plan to the source it was planned against.
///
/// Right to left, so an earlier replacement's span stays valid while a later
/// one is spliced. The plan's spans are disjoint by construction.
#[must_use]
pub fn apply_plan(source: &str, plan: &RewritePlan) -> String {
    let mut out = source.to_owned();
    for replacement in plan.replacements.iter().rev() {
        let (start, end) = (replacement.span.start().get(), replacement.span.end().get());
        out.replace_range(start..end, &replacement.after);
    }
    out
}

/// Whether rewriting `found` would delete a comment.
///
/// A comment inside a capture the template substitutes is copied along with
/// the capture's verbatim text and survives. Everything else inside the match
/// span is discarded, so a comment there is lost.
fn drops_a_comment(tree: &SyntaxTree, found: &PatternMatch, used: &[String]) -> bool {
    let carried: Vec<ByteSpan> = found
        .captures
        .iter()
        .filter(|capture| used.contains(&capture.name))
        .filter_map(|capture| capture.span)
        .collect();

    tree.comments().any(|comment| {
        let span = comment.span();
        let inside_match = span.start() >= found.span.start() && span.end() <= found.span.end();
        inside_match
            && !carried
                .iter()
                .any(|kept| span.start() >= kept.start() && span.end() <= kept.end())
    })
}

/// Records every `?name` atom under `view`, with spans relative to `origin`.
fn collect_holes(
    view: &ExpressionView,
    origin: usize,
    holes: &mut Vec<Hole>,
) -> Result<(), RewriteError> {
    let mut pending = vec![view];
    while let Some(view) = pending.pop() {
        pending.extend(view.children.iter());
        if view.kind != ExpressionKind::Atom {
            continue;
        }
        let text = view.text.as_deref().unwrap_or_default();
        let symbol = &text[view.symbol_offset.min(text.len())..];
        // The pattern's own tokenizer, so a spelling that is a placeholder in
        // `--query` is a placeholder in `--rewrite` and never one in only one
        // of the two.
        let name = match classify_atom(symbol).map_err(RewriteError::Placeholder)? {
            AtomToken::Wildcard {
                capture: Some(name),
                ..
            } => name,
            AtomToken::Rest(rest) => match rest.capture {
                Some(name) => name,
                // A bare `...` has nothing to substitute: the template says
                // "some forms go here" without saying which.
                None => return Err(RewriteError::AnonymousRest),
            },
            // `_` is a wildcard when matching and an ordinary symbol when
            // writing, because there is no one form for it to stand for.
            AtomToken::Wildcard { capture: None, .. } | AtomToken::Literal => continue,
        };
        // `content_span` starts past the reader prefixes, so `#'?fn` swaps
        // only `?fn` and keeps the `#'` attached to the result.
        let span = view.content_span;
        holes.push(Hole {
            span: ByteSpan::new(
                crate::sexpr::ByteOffset::new(span.start().get() - origin),
                crate::sexpr::ByteOffset::new(span.end().get() - origin),
            ),
            name,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(source: &str, query: &str, rewrite: &str) -> (SyntaxTree, RewritePlan) {
        plan_allowing(source, query, rewrite, false)
    }

    fn plan_allowing(
        source: &str,
        query: &str,
        rewrite: &str,
        allow_comment_loss: bool,
    ) -> (SyntaxTree, RewritePlan) {
        let dialect = Dialect::CommonLisp;
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse source");
        let pattern = Pattern::parse(query, dialect).expect("parse pattern");
        let template = Template::parse(rewrite, dialect).expect("parse template");
        template.check_against(&pattern).expect("template checks");
        let plan = plan_rewrite(&tree, &pattern, &template, dialect, allow_comment_loss);
        (tree, plan)
    }

    fn rewritten(source: &str, query: &str, rewrite: &str) -> String {
        let (tree, plan) = plan(source, query, rewrite);
        apply_plan(tree.source(), &plan)
    }

    #[test]
    fn substitutes_captures_verbatim() {
        assert_eq!(
            rewritten("(if a b nil)", "(if ?t ?a nil)", "(when ?t ?a)"),
            "(when a b)"
        );
    }

    #[test]
    fn a_captured_float_keeps_its_common_lisp_exponent_marker() {
        // Re-printing a parsed value is how `1.0d0` becomes `1`. Splicing
        // source text cannot lose the marker, and this pins that.
        assert_eq!(rewritten("(f 1.0d0)", "(f ?x)", "(g ?x)"), "(g 1.0d0)");
    }

    #[test]
    fn a_captured_string_keeps_its_escapes_and_newlines() {
        let source = "(f \"a\\nb\tc\")";
        assert_eq!(rewritten(source, "(f ?x)", "(g ?x)"), "(g \"a\\nb\tc\")");
    }

    #[test]
    fn a_placeholder_inside_a_string_literal_is_text() {
        assert_eq!(
            rewritten("(f a)", "(f ?x)", "(g \"?x\" ?x)"),
            "(g \"?x\" a)"
        );
    }

    #[test]
    fn a_reader_prefix_on_a_placeholder_survives_substitution() {
        assert_eq!(rewritten("(f foo)", "(f ?fn)", "(g #'?fn)"), "(g #'foo)");
    }

    #[test]
    fn a_rest_capture_carries_every_form_it_swallowed() {
        assert_eq!(
            rewritten("(progn a b c)", "(progn ?body...)", "(let () ?body...)"),
            "(let () a b c)"
        );
    }

    #[test]
    fn an_empty_rest_takes_its_preceding_space_with_it() {
        assert_eq!(
            rewritten("(progn)", "(progn ?body...)", "(let () ?body...)"),
            "(let ())"
        );
    }

    #[test]
    fn a_multi_line_capture_keeps_its_own_line_breaks() {
        let source = "(if a\n    (progn\n      b)\n    nil)";
        assert_eq!(
            rewritten(source, "(if ?t ?a nil)", "(when ?t ?a)"),
            "(when a (progn\n      b))"
        );
    }

    #[test]
    fn the_outermost_of_two_nested_matches_wins_and_the_inner_is_counted() {
        let (_, plan) = plan("(f (f x))", "(f ?x)", "(g ?x)");
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(plan.skipped_for(SkipReason::Overlapping), 1);
    }

    #[test]
    fn a_comment_no_capture_carries_blocks_the_rewrite_by_default() {
        let source = "(if a\n    b ; keep me\n    nil)";
        let (_, plan) = plan(source, "(if ?t ?a nil)", "(when ?t ?a)");
        assert!(plan.is_empty());
        assert_eq!(plan.skipped_for(SkipReason::CommentLoss), 1);
    }

    #[test]
    fn a_comment_inside_a_substituted_capture_is_carried_over() {
        let source = "(if a\n    (progn ; keep me\n      b)\n    nil)";
        let (_, plan) = plan(source, "(if ?t ?a nil)", "(when ?t ?a)");
        assert_eq!(plan.replacements.len(), 1);
        assert!(plan.replacements[0].after.contains("; keep me"));
    }

    #[test]
    fn allow_comment_loss_rewrites_and_loses_the_comment() {
        let source = "(if a\n    b ; drop me\n    nil)";
        let (tree, plan) = plan_allowing(source, "(if ?t ?a nil)", "(when ?t ?a)", true);
        let after = apply_plan(tree.source(), &plan);
        assert_eq!(after, "(when a b)");
    }

    #[test]
    fn a_rewrite_that_changes_nothing_is_counted_apart_from_the_replacements() {
        let (_, plan) = plan("(when a b)", "(when ?t ?a)", "(when ?t ?a)");
        assert!(plan.is_empty());
        assert_eq!(plan.unchanged, 1);
    }

    #[test]
    fn a_template_naming_an_unbound_capture_is_refused_before_any_matching() {
        let dialect = Dialect::CommonLisp;
        let pattern = Pattern::parse("(f ?x)", dialect).expect("pattern");
        let template = Template::parse("(g ?y)", dialect).expect("template");
        let error = template.check_against(&pattern).expect_err("must refuse");
        assert!(matches!(error, RewriteError::UnboundCapture { .. }));
    }

    #[test]
    fn an_anonymous_rest_in_a_template_is_refused() {
        let error = Template::parse("(g ...)", Dialect::CommonLisp).expect_err("must refuse");
        assert!(matches!(error, RewriteError::AnonymousRest));
    }

    #[test]
    fn a_template_of_two_forms_is_refused() {
        let error = Template::parse("(g) (h)", Dialect::CommonLisp).expect_err("must refuse");
        assert!(matches!(error, RewriteError::NotOneForm { count: 2 }));
    }

    #[test]
    fn a_kind_suffix_in_a_template_names_the_same_capture_as_in_the_pattern() {
        assert_eq!(
            rewritten("(f (a))", "(f ?x:list)", "(g ?x:list)"),
            "(g (a))"
        );
    }

    #[test]
    fn several_matches_are_all_rewritten_in_one_pass() {
        assert_eq!(
            rewritten(
                "(if a b nil)\n(if c d nil)",
                "(if ?t ?a nil)",
                "(when ?t ?a)"
            ),
            "(when a b)\n(when c d)"
        );
    }
}
