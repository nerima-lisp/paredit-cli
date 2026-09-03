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
//! Three hazards do not announce themselves, because all three leave source
//! that still parses:
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
//! - **Quoted data.** `'(a (if x y nil) b)` is a *list literal*. It has the
//!   shape `(if ?t ?a nil)` matches, and rewriting it to `'(a (when x y) b)`
//!   changes the program's data rather than its code — two different lists,
//!   both of which read. A match strictly inside a quote is skipped by
//!   default. A match whose *own* node carries the quote is not: writing the
//!   prefix in the pattern is naming it.
//!
//! Both are reported as [`SkipReason`]s rather than folded into a total, so a
//! caller can say *why* a match it expected to see did not appear.

use std::collections::HashSet;

use crate::dialect::Dialect;
use crate::sexpr::{
    ByteSpan, ExpressionKind, ExpressionPath, ExpressionView, ReaderPrefix, SyntaxTree,
};

use super::error::RewriteError;
use super::matcher::{Capture, PatternMatch, match_all, symbol_matches};
use super::pattern::{AtomToken, Pattern, classify_atom};

/// One `?name` placeholder in a template, located in the template's own text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hole {
    /// The placeholder's span within [`Template::body`], covering the `?name`
    /// token alone — not any reader prefix the template wrote in front of it.
    span: ByteSpan,
    /// The capture name, with any `:kind` suffix and trailing `...` removed.
    name: String,
    /// Whether the template wrote reader prefixes on this placeholder, as in
    /// `#'?fn`.
    ///
    /// It decides whether the captured text keeps its *own* prefixes. A bare
    /// `?fn` matches `#'foo` as readily as `foo` and binds the whole thing,
    /// prefix included — so `--rewrite "(g #'?fn)"` over `(f #'foo)` spliced
    /// `#'foo` after the template's `#'` and produced `#'#'foo`. Running it
    /// again produced `#'#'#'foo`; nothing converges and nothing complains,
    /// because every step reparses.
    prefixed: bool,
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

    /// Converts an already-parsed form into a template, without reading text.
    ///
    /// The counterpart to [`super::pattern::Pattern::from_view`]: `defrule`'s
    /// `:fix` clause is a sub-form of a rule file the reader has already
    /// parsed, and `source` is that file's whole text, which is where `view`'s
    /// span is measured from.
    pub fn from_view(view: &ExpressionView, source: &str) -> Result<Self, RewriteError> {
        let origin = view.span.start().get();
        let body = source[origin..view.span.end().get()].to_owned();
        let mut holes = Vec::new();
        collect_holes(view, origin, &mut holes)?;
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
    ///
    /// `pub`: [`plan_rewrite`] is the whole-document planner most callers
    /// want, but `defrule` reports one finding per match rather than a single
    /// plan, and still wants this same hazard-free rendering for the finding's
    /// fix text once it has confirmed (via `plan_rewrite`'s `skipped` list)
    /// that the match is not one of the hazards this module refuses.
    #[must_use]
    pub fn render(&self, source: &str, captures: &[Capture]) -> String {
        let mut out = String::with_capacity(self.body.len());
        let mut cursor = 0usize;
        for hole in &self.holes {
            let capture = captures.iter().find(|capture| capture.name == hole.name);
            let bound = capture
                .and_then(|capture| capture.span)
                .map(|span| &source[span.start().get()..span.end().get()])
                .unwrap_or_default();
            // The template already wrote the prefixes it wants. Keeping the
            // capture's own on top of them is how `#'?fn` doubles; dropping
            // them is what `#'?fn` plainly means — "that form, function-quoted".
            //
            // Only when the template wrote prefixes. A bare `?x` must splice
            // `'a` as `'a`: stripping there would turn `(f 'a)` into `(g a)`
            // and change when the argument is evaluated.
            let bound = if hole.prefixed {
                strip_reader_prefixes(bound)
            } else {
                bound
            };

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
    /// The match sits inside quoted data, where a rewrite changes a literal
    /// rather than a program.
    Quoted,
}

impl SkipReason {
    /// The wire spelling, shared by the text and JSON renderings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overlapping => "overlapping",
            Self::CommentLoss => "comment-loss",
            Self::Quoted => "quoted",
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
            Self::Quoted => {
                "the match is inside quoted data, where a rewrite changes a \
                 literal rather than a program; pass --include-quoted to \
                 rewrite it anyway"
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

/// What a caller is willing to rewrite anyway.
///
/// A struct rather than two `bool` parameters, because `plan_rewrite(.., true,
/// false)` at a call site says nothing about which guard was waived, and these
/// are exactly the two decisions a reviewer needs to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RewriteAllowances {
    /// Rewrite a match even when it deletes a comment no capture carries.
    pub comment_loss: bool,
    /// Rewrite a match inside quoted data, changing a literal.
    pub quoted: bool,
}

/// Plans every rewrite `pattern` and `template` make over `tree`.
///
/// `allow` opts out of the guards described in the module documentation. Both
/// default to off at every call site in this repository; a caller turning one
/// on is choosing the loss it names.
#[must_use]
pub fn plan_rewrite(
    tree: &SyntaxTree,
    pattern: &Pattern,
    template: &Template,
    dialect: Dialect,
    allow: RewriteAllowances,
) -> RewritePlan {
    let source = tree.source();
    let used: Vec<String> = template.capture_names();
    let mut plan = RewritePlan::default();
    // Matches arrive sorted by (start, end), so an enclosing match is always
    // seen before the ones nested inside it. Spans in a tree are nested or
    // disjoint, never partially overlapping, which is what makes a single
    // high-water mark a complete overlap test.
    let mut applied_end = 0usize;
    // Computed once per document rather than per match: the walk is the whole
    // tree either way, and a pattern that matches a thousand forms would
    // otherwise walk it a thousand times.
    let quoted = if allow.quoted {
        HashSet::new()
    } else {
        quoted_spans(tree, dialect)
    };
    let comments: Vec<ByteSpan> = if allow.comment_loss {
        Vec::new()
    } else {
        tree.comments().map(|comment| comment.span()).collect()
    };

    for found in match_all(tree, pattern, dialect) {
        if found.span.start().get() < applied_end {
            plan.skipped.push(SkippedMatch {
                path: found.path,
                span: found.span,
                reason: SkipReason::Overlapping,
            });
            continue;
        }
        if quoted.contains(&found.span) {
            plan.skipped.push(SkippedMatch {
                path: found.path,
                span: found.span,
                reason: SkipReason::Quoted,
            });
            continue;
        }
        if !allow.comment_loss && drops_a_comment(&comments, &found, &used) {
            plan.skipped.push(SkippedMatch {
                path: found.path,
                span: found.span,
                reason: SkipReason::CommentLoss,
            });
            continue;
        }

        let before = source[found.span.start().get()..found.span.end().get()].to_owned();
        let after = template.render(source, &found.captures);
        if before == after {
            // Deliberately *before* `applied_end` moves. A match that renders
            // byte-identically discards nothing, so it shadows no nested match
            // — and treating it as an applied replacement suppressed every
            // inner match as `Overlapping` forever, under a `detail` that says
            // "run the command again to reach this one". It never converged.
            plan.unchanged += 1;
            continue;
        }
        applied_end = found.span.end().get();
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
/// One forward pass, copying the gaps between replacements and the
/// replacements themselves into a fresh buffer. The obvious implementation —
/// `replace_range` right to left over a copy of the source — is quadratic,
/// because every splice memmoves the whole tail behind it: 400 000
/// replacements over a 4 MB file took 34 seconds, and the tool's own 64 MB
/// input ceiling put that at hours. The plan's spans are disjoint and in
/// source order by construction, which is what makes a single pass possible.
#[must_use]
pub fn apply_plan(source: &str, plan: &RewritePlan) -> String {
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for replacement in &plan.replacements {
        let (start, end) = (replacement.span.start().get(), replacement.span.end().get());
        // Defensive: a caller could hand over a hand-built plan whose spans
        // are not ordered. Skipping is better than panicking on a slice that
        // runs backwards.
        if start < cursor {
            continue;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&replacement.after);
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    out
}

/// How deeply a node sits inside quoted data.
///
/// Two counters, not one flag. A plain `'` is absolute — a `,` under it does
/// not escape, because there is no backquote for it to belong to — while a
/// `` ` `` nests, and a `,` cancels exactly one level of it. Collapsing the two
/// made a `,` at quasiquote depth 2 look like an escape to code when it is
/// still template data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    /// Inside a `'` or `(quote …)`, from which nothing escapes.
    hard: bool,
    /// Enclosing quasiquotes not yet cancelled by an unquote.
    quasiquote_depth: u32,
}

impl QuoteState {
    const CODE: Self = Self {
        hard: false,
        quasiquote_depth: 0,
    };

    /// Whether a node in this state is data rather than code.
    const fn is_data(self) -> bool {
        self.hard || self.quasiquote_depth > 0
    }
}

/// Every node that sits inside quoted data, by span.
///
/// One walk carrying the context down, rather than a per-node prefix check.
/// Inspecting only a node's own prefixes answers "is this form quoted" and
/// loses the answer one level deeper, which is how `'(a (+ 1 2))` gets folded
/// to `'(a 3)` — the arithmetic is data and the node holding it carries no
/// prefix of its own.
///
/// `dialect` is here because the *matcher* folds Common Lisp case and package
/// qualifiers, so it finds a pattern inside `(QUOTE …)` and `(cl:quote …)`.
/// A guard that compared head symbols with `==` found neither, and let the
/// rewrite land in a data literal.
fn quoted_spans(tree: &SyntaxTree, dialect: Dialect) -> HashSet<ByteSpan> {
    let mut quoted = HashSet::new();
    // Bound rather than chained: `root_view` returns an owned tree whose
    // `Drop` would otherwise run while the walk still borrows into it.
    let root = tree.root_view();
    let mut pending: Vec<(&ExpressionView, QuoteState)> = root
        .children
        .iter()
        .map(|child| (child, QuoteState::CODE))
        .collect();

    while let Some((view, state)) = pending.pop() {
        let below = descend(view, state, dialect);
        // A node that *opens* code is code, however deep inside a quasiquote
        // it sits: `,(f x)` is evaluated. Marking it data because its
        // ancestors are would refuse the one form a caller writing `,(f ?x)`
        // was pointing at.
        if state.is_data() && below.is_data() {
            quoted.insert(view.span);
        }
        pending.extend(view.children.iter().map(|child| (child, below)));
    }
    quoted
}

/// The state `view`'s children sit in.
fn descend(view: &ExpressionView, state: QuoteState, dialect: Dialect) -> QuoteState {
    let names = |spellings: &[&str]| {
        head_symbol(view).is_some_and(|head| {
            spellings
                .iter()
                .any(|spelling| symbol_matches(head, spelling, dialect))
        })
    };

    if view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing
        )
    }) || names(&["unquote", "unquote-splicing"])
    {
        // A `,` under a plain `'` cancels nothing: there is no backquote it
        // belongs to, and the whole subtree is still the quoted datum.
        return if state.hard {
            state
        } else {
            QuoteState {
                quasiquote_depth: state.quasiquote_depth.saturating_sub(1),
                ..state
            }
        };
    }
    if view
        .reader_prefixes
        .iter()
        .any(|prefix| matches!(prefix, ReaderPrefix::Quasiquote))
        || names(&["quasiquote", "backquote"])
    {
        return QuoteState {
            quasiquote_depth: state.quasiquote_depth.saturating_add(1),
            ..state
        };
    }
    if view
        .reader_prefixes
        .iter()
        .any(|prefix| matches!(prefix, ReaderPrefix::Quote))
        || names(&["quote"])
    {
        return QuoteState {
            hard: true,
            ..state
        };
    }
    state
}

/// A paren list's head symbol, for the spelled-out `(quote …)` forms.
fn head_symbol(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List {
        return None;
    }
    let head = view.children.first()?;
    if head.kind != ExpressionKind::Atom {
        return None;
    }
    let text = head.text.as_deref()?;
    Some(&text[head.symbol_offset.min(text.len())..])
}

/// Whether rewriting `found` would delete a comment.
///
/// A comment inside a capture the template substitutes is copied along with
/// the capture's verbatim text and survives. Everything else inside the match
/// span is discarded, so a comment there is lost.
fn drops_a_comment(comments: &[ByteSpan], found: &PatternMatch, used: &[String]) -> bool {
    let carried: Vec<ByteSpan> = found
        .captures
        .iter()
        .filter(|capture| used.contains(&capture.name))
        .filter_map(|capture| capture.span)
        .collect();

    // The comments arrive in source order, so the ones inside this match are a
    // contiguous run. Scanning all of them per match made a file with many
    // comments and many matches quadratic — and paid that cost even when every
    // match was going to be skipped anyway.
    let first = comments.partition_point(|span| span.start() < found.span.start());
    comments[first..]
        .iter()
        .take_while(|span| span.end() <= found.span.end())
        .any(|span| {
            !carried
                .iter()
                .any(|kept| span.start() >= kept.start() && span.end() <= kept.end())
        })
}

/// `text` with any leading reader prefixes and the trivia after them removed.
///
/// Deliberately only the five spellings that **cannot begin an atom**. A bare
/// `#` is a reader prefix in `#(1 2 3)` and is *not* one in `#\a`, `#xff` or
/// `#b1010`, where the reader keeps the dispatch character on the atom itself
/// (see [`super::pattern`]'s note on `is_number_literal`). Stripping it
/// textually would turn a captured `#\a` into `\a`. Clojure's `#?` and `#{`
/// are left alone for the same reason: the gain is a rarer template than the
/// risk is worth.
///
/// Every leading prefix goes, not just one: `'?x` means "that form, quoted",
/// and a capture that arrived as `''a` was already two quotes deep.
fn strip_reader_prefixes(text: &str) -> &str {
    // Longest first: `,@` must be tried before `,`.
    const PREFIXES: [&str; 5] = [",@", "#'", "'", "`", ","];
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let Some(stripped) = PREFIXES
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix))
        else {
            return trimmed;
        };
        rest = stripped;
    }
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
            prefixed: !view.reader_prefixes.is_empty(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(source: &str, query: &str, rewrite: &str) -> (SyntaxTree, RewritePlan) {
        plan_allowing(source, query, rewrite, RewriteAllowances::default())
    }

    fn plan_allowing(
        source: &str,
        query: &str,
        rewrite: &str,
        allow: RewriteAllowances,
    ) -> (SyntaxTree, RewritePlan) {
        let dialect = Dialect::CommonLisp;
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse source");
        let pattern = Pattern::parse(query, dialect).expect("parse pattern");
        let template = Template::parse(rewrite, dialect).expect("parse template");
        template.check_against(&pattern).expect("template checks");
        let plan = plan_rewrite(&tree, &pattern, &template, dialect, allow);
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
    fn preserves_escaped_question_mark_symbols_in_elisp_rewrites() {
        let dialect = Dialect::EmacsLisp;
        let source = r"(old value)";
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse source");
        let pattern = Pattern::parse("(old ?value)", dialect).expect("parse pattern");
        let template = Template::parse(r"(new '\?class ?value)", dialect).expect("parse template");
        template.check_against(&pattern).expect("template checks");
        let plan = plan_rewrite(
            &tree,
            &pattern,
            &template,
            dialect,
            RewriteAllowances::default(),
        );
        assert_eq!(apply_plan(tree.source(), &plan), r"(new '\?class value)");
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
        let allow = RewriteAllowances {
            comment_loss: true,
            ..RewriteAllowances::default()
        };
        let (tree, plan) = plan_allowing(source, "(if ?t ?a nil)", "(when ?t ?a)", allow);
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

    /// `'(a (if x y nil) b)` is a list literal. Rewriting the `if` inside it
    /// produces a *different list*, and the result reads perfectly — so
    /// nothing downstream can catch it. One level deeper than the top-level
    /// quote a naive prefix check would notice.
    #[test]
    fn a_match_inside_quoted_data_is_refused() {
        let (_, plan) = plan("'(a (if x y nil) b)", "(if ?t ?a nil)", "(when ?t ?a)");
        assert!(plan.is_empty());
        assert_eq!(plan.skipped_for(SkipReason::Quoted), 1);
    }

    #[test]
    fn the_spelled_out_quote_form_counts_as_quoted_too() {
        let (_, plan) = plan("(quote (a (if x y nil)))", "(if ?t ?a nil)", "(when ?t ?a)");
        assert_eq!(plan.skipped_for(SkipReason::Quoted), 1);
    }

    /// An unquote inside a quasiquote is code again. Refusing to rewrite it
    /// would be a different wrong answer from rewriting the data around it.
    ///
    /// The match is one level under the `,` rather than on it, because the
    /// matcher is prefix-strict: `(if ?t ?a nil)` does not match
    /// `,(if x y nil)` at all, which is a separate correct behaviour.
    #[test]
    fn an_unquoted_form_inside_a_quasiquote_is_still_code() {
        let (tree, plan) = plan("`(a ,(f (if x y nil)))", "(if ?t ?a nil)", "(when ?t ?a)");
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(apply_plan(tree.source(), &plan), "`(a ,(f (when x y)))");
    }

    /// The quasiquote around it does not make an unquoted form data, so a
    /// pattern that names the `,` explicitly still reaches its form.
    #[test]
    fn a_pattern_naming_the_unquote_itself_reaches_it() {
        let (tree, plan) = plan("`(a ,(if x y nil))", ",(if ?t ?a nil)", ",(when ?t ?a)");
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(apply_plan(tree.source(), &plan), "`(a ,(when x y))");
    }

    /// A pattern that writes the prefix itself has named the quote, so the
    /// node carrying it is not "inside" anything.
    #[test]
    fn a_match_that_carries_the_quote_itself_is_not_refused() {
        let (tree, plan) = plan("(f '(a b))", "'(a b)", "'(c d)");
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(apply_plan(tree.source(), &plan), "(f '(c d))");
    }

    #[test]
    fn include_quoted_is_what_lets_a_data_rewrite_through() {
        let allow = RewriteAllowances {
            quoted: true,
            ..RewriteAllowances::default()
        };
        let (tree, plan) = plan_allowing(
            "'(a (if x y nil) b)",
            "(if ?t ?a nil)",
            "(when ?t ?a)",
            allow,
        );
        assert_eq!(apply_plan(tree.source(), &plan), "'(a (when x y) b)");
    }

    /// A bare `?fn` matches `#'foo` and binds it *with* the prefix, so a
    /// template writing its own `#'` used to emit `#'#'foo` — and again on the
    /// next run, without bound. Every step reparsed, so nothing caught it.
    #[test]
    fn a_prefixed_template_hole_does_not_double_a_prefix_the_capture_carries() {
        assert_eq!(rewritten("(f #'foo)", "(f ?fn)", "(g #'?fn)"), "(g #'foo)");
    }

    #[test]
    fn a_prefixed_template_hole_is_idempotent_across_runs() {
        let mut current = "(mapcar #'foo xs)".to_owned();
        for _ in 0..3 {
            current = rewritten(&current, "(mapcar ?fn ?xs)", "(mapcar #'?fn ?xs)");
        }
        assert_eq!(current, "(mapcar #'foo xs)");
    }

    /// A character literal is an *atom* whose text begins with `#`, not a
    /// prefixed node, so
    /// the prefix strip must not touch it. Stripping textually would leave
    /// `\a`.
    #[test]
    fn a_character_literal_survives_a_prefixed_hole() {
        assert_eq!(rewritten(r"(f #\a)", "(f ?x)", "(h #'?x)"), r"(h #'#\a)");
    }

    /// The strip happens only when the *template* wrote prefixes. A bare hole
    /// must splice `'a` as `'a`, or `(f 'a)` becomes `(g a)` and the argument
    /// starts being evaluated.
    #[test]
    fn a_bare_template_hole_keeps_the_captures_own_prefix() {
        assert_eq!(rewritten("(f 'a)", "(f ?x)", "(g ?x)"), "(g 'a)");
    }

    /// The matcher folds Common Lisp case and package qualifiers, so it finds
    /// the pattern inside `(QUOTE …)`. A guard comparing head symbols with
    /// `==` did not, and the rewrite landed in a data literal.
    #[test]
    fn the_quote_guard_folds_case_the_way_the_matcher_does() {
        for source in [
            "(QUOTE (a (if x y nil)))",
            "(Quote (a (if x y nil)))",
            "(cl:quote (a (if x y nil)))",
        ] {
            let (_, plan) = plan(source, "(if ?t ?a nil)", "(when ?t ?a)");
            assert!(plan.is_empty(), "{source} was rewritten inside quoted data");
            assert_eq!(plan.skipped_for(SkipReason::Quoted), 1, "{source}");
        }
    }

    /// A `,` at quasiquote depth 2 cancels one level and leaves the form
    /// still inside the outer template. Treating any `,` as an escape to code
    /// rewrote a literal.
    #[test]
    fn an_unquote_nested_two_quasiquotes_deep_is_still_data() {
        let (_, plan) = plan(
            "`(gen `(inner ,(f (if x y nil))))",
            "(if ?t ?a nil)",
            "(when ?t ?a)",
        );
        assert!(plan.is_empty());
        assert_eq!(plan.skipped_for(SkipReason::Quoted), 1);
    }

    /// A `,` under a plain `'` cancels nothing — there is no backquote for it
    /// to belong to, and the whole subtree is still the quoted datum.
    #[test]
    fn an_unquote_under_a_plain_quote_does_not_escape_to_code() {
        // The match sits one level *under* the `,`, because the matcher is
        // prefix-strict: `(if ?t ?a nil)` does not match `,(if x y nil)` at
        // all. What is being tested is that the `,` did not turn the subtree
        // back into code.
        let (_, plan) = plan("'(a ,(f (if x y nil)))", "(if ?t ?a nil)", "(when ?t ?a)");
        assert!(plan.is_empty());
        assert_eq!(plan.skipped_for(SkipReason::Quoted), 1);
    }

    /// An outer match that renders byte-identically discards nothing, so it
    /// shadows no nested match. Counting it as applied suppressed every inner
    /// match as `Overlapping` — for ever, since re-running reproduced it — and
    /// `SkipReason::Overlapping`'s advice is "run the command again".
    #[test]
    fn an_unchanged_outer_match_does_not_block_the_matches_inside_it() {
        // The outer `(f (f a b) (f a b))` renders to itself — its two
        // arguments are equal, so swapping them changes nothing. The two
        // inner matches are the real work, and they used to be lost.
        let (swapped_tree, swapped) = plan("(f (f a b) (f a b))", "(f ?x ?y)", "(f ?y ?x)");
        assert_eq!(swapped.unchanged, 1);
        assert_eq!(swapped.replacements.len(), 2);
        assert_eq!(swapped.skipped_for(SkipReason::Overlapping), 0);
        assert_eq!(
            apply_plan(swapped_tree.source(), &swapped),
            "(f (f b a) (f b a))"
        );

        // The genuinely-unchanged case: the outer render is identical, and the
        // two inner matches are still reachable.
        let (identical_tree, identical) = plan("(f (g a) (g a))", "(g ?x)", "(g ?x)");
        assert_eq!(identical.unchanged, 2);
        assert_eq!(identical.skipped_for(SkipReason::Overlapping), 0);
        assert_eq!(
            apply_plan(identical_tree.source(), &identical),
            "(f (g a) (g a))"
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
