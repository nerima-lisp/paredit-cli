//! `racket-match-unreachable-clause`: a `match` clause that can never be
//! reached, because an earlier clause matches everything.
//!
//! # The premise, executed
//!
//! Racket's `match` performs no reachability analysis whatsoever, and `raco
//! make` compiles a dead clause without a word. Verified on Racket v9.2:
//!
//! ```text
//! $ cat dead.rkt
//! #lang racket/base
//! (require racket/match)
//! (define (f x)
//!   (match x
//!     [(? number?) 'num]
//!     [_ 'other]
//!     [(? string?) 'str]))
//! (displayln (f "hello"))
//!
//! $ raco make dead.rkt      # no output, exit 0
//! $ racket dead.rkt
//! other
//! ```
//!
//! `(f "hello")` returns `'other`: the `(? string?)` clause is dead. Nothing in
//! the toolchain says so, which is what makes this worth a rule rather than a
//! duplicate of the compiler.
//!
//! # What counts as a catch-all
//!
//! Two pattern shapes match every value:
//!
//! - `_`, the wildcard.
//! - **any bare identifier**, which binds the value and matches anything.
//!
//! The second is the interesting one, because it includes `else`. `match` gives
//! `else` no special meaning at all — it is an ordinary identifier pattern.
//! Verified:
//!
//! ```text
//! (match 5 [else 'else-matched] [5 'five])  ; => 'else-matched
//! ```
//!
//! A programmer arriving from `cond` writes `[else …]` and gets a catch-all by
//! accident rather than by the rule they had in mind; when a clause follows it,
//! that clause is dead exactly as if they had written `[x …]`.
//!
//! # What is deliberately not a catch-all
//!
//! - **A guarded clause.** `[x #:when (> x 100) 'big]` binds `x` and then
//!   *tests* it, so the clause can fail and fall through. Verified: `(match 5
//!   [x #:when (> x 100) 'big] [5 'five])` returns `'five`. This is the rule's
//!   sharpest suppression — without it every guarded dispatch table in Racket
//!   is a false positive.
//! - **A quoted symbol.** `['a …]` is a literal, not a binding. Verified:
//!   `(match 'b ['a 'was-a] ['b 'was-b])` returns `'was-b`.
//! - **Any literal**: a number, string, character, boolean, or keyword.
//! - **Any compound pattern** — `(? pred)`, `(list …)`, `(cons a d)`, `(struct
//!   …)`, `(and …)`, `(or …)` — none of which is an atom at all.
//! - **The ellipsis `...` and `___`**, which are repetition operators rather
//!   than patterns in their own right.
//!
//! Every one of those is a deliberate false negative where it is genuinely a
//! catch-all in disguise (`(? (lambda (_) #t))` is one), which is the direction
//! this package errs in throughout.
//!
//! Scope: Racket only. Scheme's `match` is a library with clause syntax that
//! varies by implementation, and `paredit-feature-lint-scheme-idiom` anchors on
//! no `match` head at all.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{is_inert_at, is_racket_list, racket_atom, racket_head};

/// The dialects this rule models. Read by both the rule's `dialect_scope` and
/// the report's `dialect_modelled` flag, so the engine's view and the report's
/// claim cannot drift.
pub const DIALECTS: [Dialect; 1] = [Dialect::Racket];

/// The heads this rule anchors on, shared with its `HeadFilter`.
///
/// `match-let`, `match-define` and `match-let*` are deliberately absent: their
/// operands are *binding* forms, not a clause sequence, so there is nothing to
/// be unreachable.
pub const HEADS: [&str; 3] = ["match", "match-lambda", "match-lambda*"];

/// Where the clause sequence starts, per head.
///
/// `(match target clause …)` has a target to skip; the `match-lambda` family
/// takes its argument implicitly and so begins at the first operand.
const fn clause_start(head_index: usize) -> usize {
    if head_index == 0 { 2 } else { 1 }
}

/// The repetition operators, which are not patterns.
const ELLIPSES: [&str; 3] = ["...", "___", "..."];

#[derive(Debug, Clone)]
pub struct MatchUnreachableClauseItem {
    /// The span of the first clause that can never be reached.
    pub span: ByteSpan,
    /// The span of the catch-all clause that shadows it.
    pub catch_all_span: ByteSpan,
    /// The catch-all pattern's own text, so the message can name it.
    pub catch_all_pattern: String,
    /// How many clauses follow the catch-all — all of them are dead.
    pub unreachable_clause_count: usize,
}

impl Finding for MatchUnreachableClauseItem {
    fn kind(&self) -> &'static str {
        "racket-match-unreachable-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("catch_all_pattern={}", self.catch_all_pattern),
            format!("unreachable_clause_count={}", self.unreachable_clause_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("catch_all_pattern", json!(self.catch_all_pattern)),
            (
                "unreachable_clause_count",
                json!(self.unreachable_clause_count),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "this match clause can never run: the earlier `{}` pattern matches every value, \
             and match performs no reachability check ({} clause(s) after it are dead)",
            self.catch_all_pattern, self.unreachable_clause_count
        )
    }
}

/// Whether `text` is an identifier that *binds* — and therefore matches
/// anything — rather than a literal that matches only itself.
///
/// The reader has already folded `'a`, `#t`, `"s"`, `#\c` and `#:kw` into atoms
/// whose text begins with a distinguishing character, so this is a decision
/// about the first byte plus two named exceptions.
fn is_binding_identifier(text: &str) -> bool {
    if text.is_empty() || ELLIPSES.contains(&text) {
        return false;
    }
    // `_` is the wildcard, which is a catch-all by a different route but the
    // same verdict.
    if text == "_" {
        return true;
    }
    let Some(first) = text.chars().next() else {
        return false;
    };
    // A quoted or quasiquoted datum, a `#`-prefixed literal (`#t`, `#f`, `#\c`,
    // `#:kw`, `#"bytes"`), a string, a character, or a number: all literals.
    if matches!(first, '\'' | '`' | ',' | '#' | '"') {
        return false;
    }
    // A number. Deliberately loose — a token that opens like a number and does
    // not parse is a reader error, not something this rule should have an
    // opinion about. What it must never do is call `-` or `->list` a number.
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut characters = unsigned.chars();
    let numeric = match characters.next() {
        Some('.') => characters.next().is_some_and(|next| next.is_ascii_digit()),
        Some(character) => character.is_ascii_digit(),
        None => false,
    };
    !numeric
}

/// Whether a clause is guarded by `#:when` or `#:unless`, which makes it able
/// to fail however permissive its pattern is.
///
/// The guard sits immediately after the pattern. Scanning the whole clause
/// would be wrong: a `#:when` deeper inside is part of a nested `for` or
/// `match` in the body, not this clause's guard.
fn is_guarded(clause: &ExpressionView) -> bool {
    clause
        .children
        .get(1)
        .and_then(racket_atom)
        .is_some_and(|text| text == "#:when" || text == "#:unless")
}

/// Whether this clause matches every value and cannot fail.
fn is_catch_all(clause: &ExpressionView) -> Option<&str> {
    if !is_racket_list(clause) {
        return None;
    }
    if is_guarded(clause) {
        return None;
    }
    let pattern = clause.children.first()?;
    // A compound pattern is never an unconditional catch-all here; see the
    // module docs for the deliberate false negatives.
    //
    // Note that a *quoted* atom needs no separate reader-prefix test: the
    // reader keeps the prefix in the token text, so `'a` arrives here as the
    // text `'a` and [`is_binding_identifier`] rejects it on its first
    // character. An earlier version tested `reader_prefixes` as well and no
    // mutation could kill it, because the two tests answered the same question.
    // The invariant that makes the text test sufficient is pinned by
    // `the_reader_keeps_a_quote_prefix_in_the_token_text` below.
    let text = racket_atom(pattern)?;
    is_binding_identifier(text).then_some(text)
}

/// Examines one node, which the caller has already narrowed to one of this
/// rule's heads (or is about to have this function narrow for it).
pub fn examine_match(
    tree: &SyntaxTree,
    view: &ExpressionView,
    match_form_count: &mut usize,
    violations: &mut Vec<MatchUnreachableClauseItem>,
) {
    // Cheap and node-local first. `is_inert_at` walks ancestors and is the only
    // non-local cost in the rule, so it runs last and only for a node that
    // would otherwise be reported.
    let Some(head) = racket_head(view) else {
        return;
    };
    let Some(head_index) = HEADS.iter().position(|candidate| *candidate == head) else {
        return;
    };
    let start = clause_start(head_index);
    let Some(clauses) = view.children.get(start..) else {
        return;
    };
    // A `match` with fewer than two clauses can have nothing unreachable.
    if clauses.len() < 2 {
        // Still a match form that was looked at, so it counts toward the
        // denominator.
        *match_form_count += 1;
        return;
    }
    *match_form_count += 1;

    // The first catch-all shadows everything after it. Only clauses before the
    // last one can shadow anything.
    let Some((offset, pattern)) = clauses[..clauses.len() - 1]
        .iter()
        .enumerate()
        .find_map(|(offset, clause)| is_catch_all(clause).map(|pattern| (offset, pattern)))
    else {
        return;
    };
    let pattern = pattern.to_owned();
    let catch_all_span = clauses[offset].span;
    let dead = &clauses[offset + 1..];

    if is_inert_at(tree, view.span) {
        return;
    }

    violations.push(MatchUnreachableClauseItem {
        span: dead[0].span,
        catch_all_span,
        catch_all_pattern: pattern,
        unreachable_clause_count: dead.len(),
    });
}

/// Collects every unreachable `match` clause in one file, with the number of
/// `match`-family forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_match_unreachable_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MatchUnreachableClauseItem>> {
    let modelled = DIALECTS.contains(&dialect);
    let mut match_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        let root = tree.root_view();
        let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
        while let Some(view) = stack.pop() {
            examine_match(tree, view, &mut match_form_count, &mut violations);
            stack.extend(view.children.iter().rev());
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("match_form_count", json!(match_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MatchUnreachableClauseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse input");
        build_match_unreachable_clause_report(Path::new("main.rkt"), Dialect::Racket, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<MatchUnreachableClauseItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "match_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("match_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    /// [`is_catch_all`] decides "quoted datum or binding identifier?" from the
    /// token text alone. That is only sound because the reader keeps the quote
    /// **in** the text rather than stripping it into `reader_prefixes` alone.
    ///
    /// This test exists because a `reader_prefixes` guard beside the text test
    /// survived every mutation: the two were answering the same question, so the
    /// prefix guard was removed and the assumption it duplicated was pinned
    /// here instead. If the reader ever strips the prefix from the text, `'a`
    /// becomes a bare identifier, every quoted-symbol clause becomes a
    /// catch-all, and this fails first.
    #[test]
    fn the_reader_keeps_a_quote_prefix_in_the_token_text() {
        for (source, expected) in [
            ("(match x ['a 1])", "'a"),
            ("(match x [`a 1])", "`a"),
            ("(match x [,a 1])", ",a"),
        ] {
            let tree = SyntaxTree::parse_with_dialect(source, Dialect::Racket).expect("parse");
            let pattern = &tree.root_view().children[0].children[2].children[0];
            assert_eq!(
                pattern.text.as_deref(),
                Some(expected),
                "the reader must keep the prefix in the token text: {source}"
            );
            assert!(
                !is_binding_identifier(expected),
                "{expected} must not read as a binding identifier"
            );
        }
    }

    // -- positive ------------------------------------------------------------

    /// The executed premise, reduced.
    #[test]
    fn flags_a_clause_after_a_wildcard() {
        let source = "(match x [(? number?) 'num] [_ 'other] [(? string?) 'str])";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(slice(source, found[0].span), "[(? string?) 'str]");
        assert_eq!(slice(source, found[0].catch_all_span), "[_ 'other]");
        assert_eq!(found[0].catch_all_pattern, "_");
        assert_eq!(found[0].unreachable_clause_count, 1);
    }

    /// `else` is not special in `match`; it is an ordinary identifier pattern
    /// and binds. Verified on Racket v9.2.
    #[test]
    fn flags_a_clause_after_an_else_pattern() {
        let found = findings("(match x [1 'one] [else 'fallback] [2 'two])");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].catch_all_pattern, "else");
    }

    #[test]
    fn flags_a_clause_after_a_bare_binding_identifier() {
        let found = findings("(match x [n 'bound] [5 'five])");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].catch_all_pattern, "n");
    }

    #[test]
    fn counts_every_clause_after_the_catch_all_not_only_the_first() {
        let found = findings("(match x [_ 'any] [1 'one] [2 'two] [3 'three])");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].unreachable_clause_count, 3);
    }

    #[test]
    fn reports_the_first_catch_all_when_there_are_two() {
        let found = findings("(match x [a 'first] [b 'second] [1 'one])");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].catch_all_pattern, "a");
        assert_eq!(found[0].unreachable_clause_count, 2);
    }

    #[test]
    fn flags_the_match_lambda_family() {
        assert_eq!(findings("(match-lambda [_ 'any] [1 'one])").len(), 1);
        assert_eq!(findings("(match-lambda* [_ 'any] [1 'one])").len(), 1);
    }

    /// The `match-lambda` family has no target operand, so its clauses begin one
    /// position earlier. Reading them at the `match` offset would skip the
    /// catch-all entirely.
    #[test]
    fn the_match_lambda_family_reads_its_first_operand_as_a_clause() {
        // With `match`'s offset the `[_ 'any]` clause would be treated as the
        // target and the remaining single clause could shadow nothing.
        let found = findings("(match-lambda [_ 'any] [1 'one])");
        assert_eq!(found[0].catch_all_pattern, "_");
    }

    #[test]
    fn a_parenthesised_clause_is_read_like_a_bracketed_one() {
        assert_eq!(findings("(match x (_ 'any) (1 'one))").len(), 1);
    }

    // -- the guard, which is the sharpest suppression -------------------------

    /// `(match 5 [x #:when (> x 100) 'big] [5 'five])` returns `'five`, so the
    /// guarded clause is not a catch-all and the clause after it is live.
    #[test]
    fn does_not_flag_a_clause_after_a_when_guarded_pattern() {
        assert!(findings("(match x [n #:when (> n 100) 'big] [5 'five])").is_empty());
        assert!(findings("(match x [_ #:when (p) 'guarded] [5 'five])").is_empty());
    }

    #[test]
    fn does_not_flag_a_clause_after_an_unless_guarded_pattern() {
        assert!(findings("(match x [n #:unless (p n) 'no] [5 'five])").is_empty());
    }

    /// A `#:when` deeper in the clause body belongs to a nested form, not to
    /// this clause. Only position 1 is a guard.
    #[test]
    fn a_when_keyword_in_the_body_is_not_a_guard() {
        let found = findings("(match x [_ (for/list ([i 3] #:when (odd? i)) i)] [1 'one])");
        assert_eq!(found.len(), 1);
    }

    // -- literals are not catch-alls -----------------------------------------

    /// `(match 'b ['a 'was-a] ['b 'was-b])` returns `'was-b`.
    #[test]
    fn does_not_flag_a_clause_after_a_quoted_symbol() {
        assert!(findings("(match x ['a 'was-a] ['b 'was-b])").is_empty());
        assert!(findings("(match x [(quote a) 1] [(quote b) 2])").is_empty());
    }

    #[test]
    fn does_not_flag_a_clause_after_any_literal_pattern() {
        for pattern in [
            "5", "-5", "+5", "5.0", ".5", "-.5", "\"s\"", "#t", "#f", "#\\c", "#:kw",
        ] {
            assert!(
                findings(&format!("(match x [{pattern} 'hit] [other 'miss])")).is_empty(),
                "{pattern} is a literal, not a catch-all"
            );
        }
    }

    #[test]
    fn does_not_flag_a_clause_after_a_compound_pattern() {
        for pattern in [
            "(? number?)",
            "(list a b)",
            "(cons a d)",
            "(struct point (x y))",
            "(and a b)",
            "(or 1 2)",
            "(vector a)",
            "(hash-table (k v))",
        ] {
            assert!(
                findings(&format!("(match x [{pattern} 'hit] [1 'one])")).is_empty(),
                "{pattern} is compound, not a catch-all"
            );
        }
    }

    /// A negative *number* must not be mistaken for an identifier, and an
    /// identifier that merely opens with a sign must not be mistaken for a
    /// number.
    #[test]
    fn the_identifier_test_separates_signs_from_numbers() {
        assert!(is_binding_identifier("-"));
        assert!(is_binding_identifier("->list"));
        assert!(is_binding_identifier("+"));
        assert!(is_binding_identifier("x"));
        assert!(is_binding_identifier("_"));
        assert!(is_binding_identifier("else"));
        assert!(!is_binding_identifier("-5"));
        assert!(!is_binding_identifier("+5"));
        assert!(!is_binding_identifier("5"));
        assert!(!is_binding_identifier(".5"));
        assert!(!is_binding_identifier("-.5"));
        assert!(!is_binding_identifier("..."));
        assert!(!is_binding_identifier("___"));
        assert!(!is_binding_identifier(""));
    }

    // -- shapes with nothing to report ---------------------------------------

    #[test]
    fn does_not_flag_a_catch_all_in_the_last_position() {
        assert!(findings("(match x [1 'one] [_ 'other])").is_empty());
        assert!(findings("(match x [1 'one] [else 'other])").is_empty());
    }

    #[test]
    fn does_not_flag_a_match_with_a_single_clause() {
        assert_eq!(scanned("(match x [_ 'any])"), 1);
        assert!(findings("(match x [_ 'any])").is_empty());
    }

    #[test]
    fn does_not_flag_a_match_with_no_clauses() {
        assert_eq!(scanned("(match x)"), 1);
        assert!(findings("(match x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_match_with_no_catch_all_at_all() {
        assert!(findings("(match x [1 'one] [2 'two] [(? string?) 's])").is_empty());
    }

    /// The target of a `match` is not a clause. Reading it as one would make
    /// `(match x [1 'one])` look like a catch-all `x` followed by a dead clause.
    #[test]
    fn the_match_target_is_not_read_as_a_clause() {
        assert!(findings("(match x [1 'one])").is_empty());
        assert!(findings("(match target [1 'one] [2 'two])").is_empty());
    }

    // -- head discipline -----------------------------------------------------

    /// The head index ASCII-lowercases, so it offers `(MATCH …)` to this rule.
    /// Racket is case sensitive and that is a different name.
    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(MATCH x [_ 'any] [1 'one])"), 0);
        assert!(findings("(MATCH x [_ 'any] [1 'one])").is_empty());
    }

    #[test]
    fn does_not_flag_a_qualified_head_that_merely_ends_in_match() {
        assert_eq!(scanned("(racket:match x [_ 'any] [1 'one])"), 0);
    }

    #[test]
    fn does_not_flag_the_binding_forms_of_the_match_family() {
        for head in ["match-let", "match-let*", "match-define", "match-letrec"] {
            assert_eq!(
                scanned(&format!("({head} ([_ x]) body)")),
                0,
                "{head} binds, it has no clause sequence"
            );
        }
    }

    // -- data and template guards --------------------------------------------

    #[test]
    fn does_not_flag_a_quoted_match_shape() {
        assert!(findings("'(match x [_ 'any] [1 'one])").is_empty());
        assert!(findings("(quote (match x [_ 'any] [1 'one]))").is_empty());
        assert!(findings("`(a (match x [_ 'any] [1 'one]))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_match_inside_a_quasiquote() {
        assert_eq!(findings("`(a ,(match x [_ 'any] [1 'one]))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_match_call() {
        assert_eq!(scanned("#(match x [_ 'any] [1 'one])"), 0);
        assert!(findings("#(match x [_ 'any] [1 'one])").is_empty());
    }

    /// A `match` inside a macro template has pattern variables its caller
    /// fills in, so its clause sequence is not the one that will run.
    #[test]
    fn does_not_flag_a_match_inside_a_macro_template() {
        let source = "(define-syntax my-match \
                      (syntax-rules () ((_ e cl ...) (match e cl ... [_ 'fallback] [1 'one]))))";
        assert!(findings(source).is_empty());
    }

    #[test]
    fn a_match_spelled_inside_a_string_is_text_not_a_form() {
        assert!(findings("(displayln \"(match x [_ 'any] [1 'one])\")").is_empty());
    }

    // -- nesting and the denominator -----------------------------------------

    #[test]
    fn finds_a_nested_match() {
        let found = findings("(define (f x) (match x [_ (match x [a 1] [2 2])] [1 'one]))");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn the_summary_counts_every_match_scanned_not_only_the_flagged_ones() {
        let source = "(match a [_ 1] [2 2])\n(match b [1 1] [_ 2])\n(match c [1 1])\n";
        assert_eq!(scanned(source), 3);
        assert_eq!(findings(source).len(), 1);
    }

    // -- the wrong dialect ---------------------------------------------------

    /// The same bytes read as four dialects. Scheme shares Racket's whole
    /// surface syntax here, so it is the sharpest control available.
    ///
    /// Written with parentheses rather than the idiomatic brackets on purpose:
    /// the Common Lisp and Clojure readers reject `[`, so a bracketed fixture
    /// would fail to *parse* for the controls and prove nothing about scope.
    #[test]
    fn the_same_bytes_are_flagged_as_racket_and_unmodelled_elsewhere() {
        let source = "(match x (_ 1) (2 2))\n";

        assert_eq!(findings(source).len(), 1);

        for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report = build_match_unreachable_clause_report(Path::new("f.scm"), dialect, &tree)
                .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
            assert_eq!(report.summary, vec![("match_form_count", json!(0))]);
        }
    }

    // -- the envelope --------------------------------------------------------

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("#lang racket\n(match x\n  [_ 'any]\n  [1 'one])\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 4);
        assert_eq!(finding.kind(), "racket-match-unreachable-clause");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("catch_all_pattern", json!("_")),
                ("unreachable_clause_count", json!(1))
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "catch_all_pattern=_".to_owned(),
                "unreachable_clause_count=1".to_owned()
            ]
        );
        assert!(finding.message().contains("can never run"));
    }
}
