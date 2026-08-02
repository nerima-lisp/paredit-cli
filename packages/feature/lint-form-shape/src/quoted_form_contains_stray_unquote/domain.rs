//! Common Lisp stray-unquote detection: a `,` or `,@` inside a hard-quoted
//! form with no backquote anywhere above it — the classic typo of writing `'`
//! where `` ` `` was meant.
//!
//! ```text
//! (defmacro my-when (test &body body)
//!   '(if ,test (progn ,@body)))       ; ' should be `
//! ```
//!
//! In Common Lisp this does not even read. SBCL rejects the file outright:
//!
//! ```text
//! $ sbcl --script f.lisp
//! READ error during LOAD:  Comma not inside a backquote.
//! ```
//!
//! CLHS 2.4.6.1 leaves the consequences of a comma outside a backquote
//! undefined, and every major implementation makes it a reader error. So this
//! is a defect that stops the file loading, reported with the form and the
//! comma named rather than as a line and column from the reader.
//!
//! # This rule's quote polarity is the inverse of every other rule's
//!
//! Everywhere else in this package, [`crate::support::is_unevaluated_at`]
//! answers "is this data?" and a `true` means *stay quiet*. Here the finding
//! **is** the data case, and the two counters must be read apart rather than
//! through [`QuoteState::is_data`]:
//!
//! | shape | `hard` | `quasi` | evaluated in CL? | this rule |
//! |---|---|---|---|---|
//! | `` `(f ,x) ``      | no  | 1 | yes | silent |
//! | `'(f ,x)`          | yes | 0 | **read error** | **fires** |
//! | `` `(list ',x) ``  | yes | 1 | yes | silent |
//! | `` `(f '(g ,x)) `` | yes | 1 | yes | silent |
//!
//! The last two rows are why `is_data()` alone is the wrong test: it is `true`
//! for both, and both are correct, extremely common macro code. Common Lisp's
//! backquote processing descends *through* a nested `quote`, so a comma under
//! `'` but still under `` ` `` is a live escape. Verified against SBCL:
//! `(defmacro m (v) `(list ',v '(tag ,v)))` macroexpands `(m 42)` to
//! `(LIST '42 '(TAG 42))` — both commas substituted.
//!
//! The condition is therefore `hard && !inside_quasiquote`, read from the state
//! **outside** the comma node, and [`crate::support::QuoteState`] exposes the
//! two separately for exactly this rule.
//!
//! # Disjoint from `redundant-quote`
//!
//! [`crate::redundant_quote`] is about a *correct* quote around a
//! self-evaluating literal (`'5`, `'"s"`, `':k`). It never looks at commas and
//! never reports a quoted list. This rule reports only quoted forms containing
//! a comma, and never a literal.
//!
//! # Reach, and the one shape `HeadFilter::Heads` cannot see
//!
//! The rule anchors on `defmacro`, `define-compiler-macro`, `macrolet` and
//! `quote`, walks each matched form's own subtree, and reports every stray
//! comma in it. That covers the typo where it actually happens — a macro
//! template — and the long-hand `(quote (a ,b))` spelling anywhere.
//!
//! It cannot see a bare `'(a ,b)` written outside all four. A `'`-prefixed
//! list's head is its *first element* (`a`), not `quote`, so no head set
//! reaches it, and `AllNodes`/`WholeTree` are excluded by this package's
//! `clean/forms/*` benchmark budget. That is a documented false negative, and
//! the narrow direction to be wrong in.
//!
//! # What this rule deliberately does not flag
//!
//! - **A comma inside a string** (`"a,b"`), which the reader keeps as one atom,
//!   so no node ever carries an unquote prefix for it.
//! - **A character literal `#\,`**, likewise one atom.
//! - **A bare `,x` in unquoted code**, which is a different reader error and
//!   not "a quoted form containing a stray unquote".
//! - **A matched form reached only as quoted data.**
//!
//! Scope: Common Lisp only. Clojure reads `,` as whitespace, and Emacs Lisp and
//! Scheme both read a stray `,x` into an ordinary `(unquote x)` datum rather
//! than signalling — so "this file will not load" is a Common Lisp claim.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    QuoteState, for_each_evaluated_subview, for_each_subview_with_outer_quote_state,
};

/// The heads this rule anchors on. See the module doc for why `'(a ,b)` written
/// outside all four is out of reach.
pub const QUOTE_CONTEXT_HEADS: [&str; 4] =
    ["defmacro", "define-compiler-macro", "macrolet", "quote"];

#[derive(Debug, Clone)]
pub struct QuotedFormContainsStrayUnquoteItem {
    /// The span of the matched enclosing form (the `defmacro`, or the
    /// `(quote …)`).
    pub span: ByteSpan,
    /// The span of the stray `,x` / `,@x` node itself.
    pub unquote_span: ByteSpan,
    /// Whether the stray comma was a splice.
    pub splicing: bool,
}

impl Finding for QuotedFormContainsStrayUnquoteItem {
    fn kind(&self) -> &'static str {
        "quoted-form-contains-stray-unquote"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![if self.splicing { ",@" } else { "," }.to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("splicing", json!(self.splicing)),
            (
                "unquote_span",
                json!({
                    "start": self.unquote_span.start().get(),
                    "end": self.unquote_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} inside a quoted form with no enclosing backquote; \
             Common Lisp signals \"Comma not inside a backquote\" at read time — \
             the ' was probably meant to be a `",
            if self.splicing { ",@" } else { "," }
        )
    }
}

/// The unquote prefix a node carries, if it carries one.
///
/// Read from the node's own prefixes only. A `','x` carries both a quote and an
/// unquote; the unquote is what matters, and the surrounding state answers the
/// rest.
fn unquote_prefix(view: &ExpressionView) -> Option<bool> {
    view.reader_prefixes.iter().find_map(|prefix| match prefix {
        ReaderPrefix::Unquote => Some(false),
        ReaderPrefix::UnquoteSplicing => Some(true),
        _ => None,
    })
}

/// Whether `outer` is the state in which a comma is stray: under a hard quote,
/// and under no backquote at all.
///
/// This is the whole rule, and it is deliberately **not**
/// [`QuoteState::is_data`] — see the module doc's table.
const fn is_stray_context(outer: QuoteState) -> bool {
    outer.is_hard_quoted() && !outer.is_inside_quasiquote()
}

/// Examines one node, which the caller has already established is evaluated
/// code with one of [`QUOTE_CONTEXT_HEADS`] as its head.
///
/// Cheapest predicate first: the head comparison rejects everything the head
/// index let through for another rule before the subtree walk starts, and the
/// walk itself allocates only its own stack — nothing per node.
pub fn examine(
    view: &ExpressionView,
    quote_context_count: &mut usize,
    violations: &mut Vec<QuotedFormContainsStrayUnquoteItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &QUOTE_CONTEXT_HEADS) {
        return;
    }
    *quote_context_count += 1;

    let span = view.span;
    for_each_subview_with_outer_quote_state(view, |node, outer| {
        let Some(splicing) = unquote_prefix(node) else {
            return;
        };
        if !is_stray_context(outer) {
            return;
        }
        violations.push(QuotedFormContainsStrayUnquoteItem {
            span,
            unquote_span: node.span,
            splicing,
        });
    });
}

/// Collects every stray unquote in one file, with the number of quote-context
/// forms scanned as the denominator beside them.
pub fn build_quoted_form_contains_stray_unquote_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<QuotedFormContainsStrayUnquoteItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("quote_context_count", json!(0))],
        ));
    }

    let mut quote_context_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        // Evaluated nodes only: a `defmacro` inside `'(…)` is a list of
        // symbols, and the commas under it are that list's commas.
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut quote_context_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("quote_context_count", json!(quote_context_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<QuotedFormContainsStrayUnquoteItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_quoted_form_contains_stray_unquote_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    // -- positive: the typo --------------------------------------------------

    #[test]
    fn flags_the_classic_quote_for_backquote_typo() {
        let violations =
            report("(defmacro my-when (test &body body) '(if ,test (progn ,@body)))").findings;
        assert_eq!(violations.len(), 2);
        assert!(!violations[0].splicing);
        assert!(violations[1].splicing);
    }

    #[test]
    fn the_unquote_span_covers_the_comma_form() {
        let source = "(defmacro m (x) '(f ,x))";
        let violations = report(source).findings;
        let span = violations[0].unquote_span;
        assert_eq!(&source[span.start().get()..span.end().get()], ",x");
    }

    #[test]
    fn flags_a_long_hand_quote_form() {
        assert_eq!(report("(quote (a ,b))").findings.len(), 1);
    }

    #[test]
    fn flags_a_stray_comma_nested_deep_inside_the_quoted_data() {
        assert_eq!(report("(defmacro m (x) '(a (b (c ,x))))").findings.len(), 1);
    }

    #[test]
    fn flags_inside_a_macrolet() {
        assert_eq!(
            report("(macrolet ((m (x) '(f ,x))) (m 1))").findings.len(),
            1
        );
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(report("(CL:DEFMACRO M (x) '(f ,x))").findings.len(), 1);
    }

    // -- the inverted polarity: correct macro code must stay silent -----------

    /// The correct spelling.
    #[test]
    fn does_not_flag_a_backquoted_template() {
        assert!(
            report("(defmacro my-when (test &body body) `(if ,test (progn ,@body)))")
                .findings
                .is_empty()
        );
    }

    /// SBCL: `(defmacro m (v) `(list ',v))` expands `(m 42)` to `(LIST '42)`.
    /// A single `is_data()` test would call this a typo.
    #[test]
    fn does_not_flag_the_quoted_unquote_idiom() {
        assert!(
            report("(defmacro m (v) `(let ((x ',v)) x))")
                .findings
                .is_empty()
        );
    }

    /// SBCL: `` `(list '(tag ,v)) `` expands to `(LIST '(TAG 42))`. Backquote
    /// processing descends through a nested `quote`, so this comma is live.
    #[test]
    fn does_not_flag_a_comma_under_a_quote_that_is_itself_under_a_backquote() {
        assert!(
            report("(defmacro m (v) `(list '(tag ,v)))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_comma_under_a_long_hand_quote_form_inside_a_backquote() {
        assert!(
            report("(defmacro m (v) `(list (quote (tag ,v))))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_nested_double_backquote() {
        assert!(
            report("(defmacro outer (x) `(defmacro inner () `(f ,,x)))")
                .findings
                .is_empty()
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form_with_no_comma() {
        assert!(report("(defmacro m () '(a b c))").findings.is_empty());
        assert!(report("(quote (a b c))").findings.is_empty());
    }

    /// A bare comma in unquoted code is a different reader error, and not "a
    /// quoted form containing a stray unquote".
    #[test]
    fn does_not_flag_a_bare_comma_in_unquoted_code() {
        assert!(report("(defmacro m (x) (f ,x))").findings.is_empty());
    }

    #[test]
    fn does_not_flag_a_macro_with_no_quoting_at_all() {
        assert!(report("(defmacro m (x) (list 'f x))").findings.is_empty());
    }

    // -- the two atoms that look like commas and are not --------------------

    #[test]
    fn does_not_flag_a_comma_inside_a_string() {
        assert!(
            report("(defmacro m () '(f \"a,b\" \",@c\"))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_comma_character_literal() {
        assert!(report("(defmacro m () '(f #\\, #\\a))").findings.is_empty());
    }

    // -- the five quote shapes, at this rule's own polarity ------------------

    /// Plain code with the typo fires.
    #[test]
    fn plain_code_fires() {
        assert!(fires("(defmacro m (x) '(f ,x))"));
    }

    /// `'(a X)` — a quoted form with no comma — is silent.
    #[test]
    fn a_quoted_form_without_a_comma_is_silent() {
        assert!(!fires("(defmacro m (x) '(f x))"));
    }

    /// `(quote (a X))` — likewise.
    #[test]
    fn a_long_hand_quote_without_a_comma_is_silent() {
        assert!(!fires("(quote (a b))"));
    }

    /// `'(a ,X)` — the inverted case. Every *other* rule in this package is
    /// silent here; this one is the reason the polarity had to be separable.
    #[test]
    fn a_comma_inside_a_hard_quote_fires_here_and_nowhere_else() {
        assert!(fires("(quote (a ,x))"));
    }

    /// `` `(a ,X) `` — fires for every other rule, silent for this one.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_silent_here() {
        assert!(!fires("(defmacro m (x) `(f ,x))"));
    }

    /// The matched form itself being data still silences it: a `defmacro`
    /// inside `'(…)` is a list of symbols.
    #[test]
    fn a_hard_quoted_matched_form_is_silent() {
        assert!(!fires("'(defmacro m (x) '(f ,x))"));
        assert!(report("'(defmacro m (x) '(f ,x))").findings.is_empty());
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(defmacro m (x) '(f ,x))", Dialect::Clojure)
            .expect("parse");
        let report = build_quoted_form_contains_stray_unquote_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_quote_context_scanned() {
        let report = report(
            "(defmacro a (x) '(f ,x))\n\
             (defmacro b (x) `(f ,x))\n\
             (quote (c d))\n",
        );
        assert_eq!(report.summary, vec![("quote_context_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_marker() {
        let report = report("(defmacro m (x)\n  '(f ,@x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 1);
        assert_eq!(finding.kind(), "quoted-form-contains-stray-unquote");
        assert_eq!(finding.text_columns(), vec![",@".to_owned()]);
        assert!(finding.message().contains("Comma not inside a backquote"));
    }
}
