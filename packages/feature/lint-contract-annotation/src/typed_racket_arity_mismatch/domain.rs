//! Typed Racket arity-mismatch detection: a `(: name (-> A B C))` annotation
//! whose arrow arity disagrees with the parameter count of the `define`
//! directly below it.
//!
//! Purely syntactic counting. Nothing here type-checks anything.
//!
//! # What the arrow means
//!
//! The Typed Racket Reference defines `(-> dom ... rng)` as "the type of
//! functions from the (possibly-empty) sequence `dom ...` to the `rng` type":
//! the **last** type is the result and the others are the arguments. So
//! `(-> Integer String Boolean)` is a two-argument function, and the arity this
//! rule compares is `(length - 2)` — one for the `->` head, one for the range.
//!
//! Note the corollary, which is why this package has no
//! "annotation missing a return type" rule: `(-> Number)` is a **nullary
//! function returning `Number`**, not an arrow missing its range. The Typed
//! Racket Guide spells this out by giving `(case-> (-> Number) (-> Number
//! Number))` as the type of a function with one optional argument.
//!
//! # Why this anchors on `define` and not on `:`
//!
//! The lint engine's head index cannot be keyed on `:` — `NormalizedHead`
//! rejects any head containing a colon at compile time — so a rule cannot ask
//! to be shown `(: …)` forms. This one is shown `define` forms instead and
//! looks *up* one top-level sibling, which is where Typed Racket's own
//! documentation and every idiomatic file put the annotation.
//!
//! # Everything that is deliberately not counted
//!
//! Positional counting is only honest for the plain prefix arrow over plain
//! positional parameters. The rule declines, silently, whenever either side has
//! a shape that breaks it:
//!
//! On the **type** side —
//!
//! - `(-> Integer * Integer)`, a uniform rest argument: variadic, no fixed
//!   arity.
//! - `(-> A B ooo bound)`, a non-uniform rest argument spelled with a literal
//!   `...`.
//! - any `#:kw`, mandatory or optional: a keyword argument is two elements of
//!   the `dom` sequence but one parameter, and an optional one is parenthesized
//!   besides.
//! - a **proposition** or **object**, `(-> Any Boolean : String)`, whose `:`
//!   ends the `dom` sequence. Everything after it is latent-proposition syntax,
//!   so a positional count reads three arguments where there is one. The
//!   `#:+`/`#:-` spelling of the same thing falls out of the `#:` case above.
//! - the **infix** arrow, `(Integer -> Boolean)`, which the reference also
//!   accepts. Only an arrow in head position is counted.
//! - `->*`, `case->`, and an `All`/`∀` wrapper, none of which has `->` at the
//!   head of the type, so all three are skipped by construction rather than by
//!   a special case.
//! - `(->)`, which has no range at all.
//!
//! On the **`define`** side —
//!
//! - a curried definition, `(define ((f x) y) …)`, whose parameter list's head
//!   is itself a list.
//! - an optional or annotated parameter, `[y 5]` or `[x : Integer]`, which is a
//!   bracket form.
//! - a keyword parameter, `#:kw k`.
//! - a rest parameter, `(define (f x . rest) …)`.
//! - `(define f (lambda (x) …))`, which names no parameter list of its own.
//! - a `define` that is not a top-level form — one inside a `module`, a `let`,
//!   or a `parameterize` has no top-level sibling above it to read.
//!
//! Every one of those is a false negative, which is the direction this package
//! errs in throughout.
//!
//! Scope: Racket only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::policy::RuleDialectScope;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, is_paren_list};
use serde_json::{Value, json};

use crate::support::{
    is_bracket_list, is_unevaluated_at, preceding_top_level_index, top_level_form, top_level_span,
};

/// The one place this rule's dialect is decided.
///
/// There is no `RACKET_ONLY` constant on [`RuleDialectScope`], so the scope is
/// constructed explicitly here. This is the first built-in rule scoped to
/// Racket *alone*: `leftover-print-debug` and `self-recursive-tail-call` both
/// name `Dialect::Racket`, but each does so inside a broad list of eight or ten
/// dialects, and neither encodes anything Racket-specific. Both
/// `crate::typed_racket_arity_mismatch::rule::Rule::dialect_scope` and
/// [`build_typed_racket_arity_mismatch_report`]'s `dialect_modelled` flag read
/// this constant, so the engine's view and the report's claim cannot drift.
pub const SCOPE: RuleDialectScope = RuleDialectScope::new(&[Dialect::Racket]);

/// The prefix-arrow spellings the reference lists. `→` is documented as an
/// alias for `->`.
const ARROW_HEADS: [&str; 2] = ["->", "\u{2192}"];

#[derive(Debug, Clone)]
pub struct TypedRacketArityMismatchItem {
    /// The span of the `define` whose parameter count disagrees.
    pub span: ByteSpan,
    /// The span of the `(: …)` annotation above it, so a reader can see both
    /// halves of the disagreement.
    pub annotation_span: ByteSpan,
    pub name: String,
    /// Argument types in the arrow — its length minus the head and the range.
    pub annotated_arity: usize,
    /// Parameters in the `define`'s parameter list.
    pub defined_arity: usize,
}

impl Finding for TypedRacketArityMismatchItem {
    fn kind(&self) -> &'static str {
        "typed-racket-arity-mismatch"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("name={}", self.name),
            format!("annotated_arity={}", self.annotated_arity),
            format!("defined_arity={}", self.defined_arity),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("annotated_arity", json!(self.annotated_arity)),
            ("defined_arity", json!(self.defined_arity)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "type annotation for `{}` declares {} argument(s) but the define takes {}: in \
             (-> dom ... rng) the last type is the result, so the argument count is one less \
             than the arrow's length",
            self.name, self.annotated_arity, self.defined_arity
        )
    }
}

/// A cheap test on raw source: does this top-level form open with a `:` head?
///
/// Run before the annotation is materialized, so that a `define` following
/// another `define` — which is most of them — costs a slice comparison instead
/// of an `ExpressionView` for the whole preceding form.
///
/// Deliberately loose: it only has to be a superset of what
/// [`read_annotation`] accepts, because that function re-checks the head on the
/// parsed form. `(:` followed by whitespace excludes `(:keyword …)` and
/// `(::foo)` without excluding anything `read_annotation` would take.
fn opens_with_colon_head(text: &str) -> bool {
    let rest = text.trim_start();
    let Some(rest) = rest.strip_prefix('(') else {
        return false;
    };
    let rest = rest.trim_start();
    rest.strip_prefix(':')
        .is_some_and(|after| after.starts_with(char::is_whitespace))
}

/// The `(name, arrow-type)` of a `(: name (-> …))` annotation, when it is one
/// this rule can count.
fn read_annotation(view: &ExpressionView) -> Option<(&str, &ExpressionView)> {
    if !is_paren_list(view) || view.children.len() != 3 {
        return None;
    }
    // Racket is case-sensitive, so the head must be exactly `:`.
    if atom_text(&view.children[0]) != Some(":") {
        return None;
    }
    let name = atom_text(&view.children[1])?;
    let ty = &view.children[2];
    Some((name, ty))
}

/// The number of argument types in a plain prefix arrow, or `None` when the
/// type is any of the shapes positional counting cannot read.
fn arrow_arity(ty: &ExpressionView) -> Option<usize> {
    if !is_paren_list(ty) {
        return None;
    }
    // Only an arrow in *head* position is counted. The infix spelling
    // `(Integer -> Boolean)` is equally legal and is declined here.
    let head = atom_text(ty.children.first()?)?;
    if !ARROW_HEADS.contains(&head) {
        return None;
    }
    // `->` plus a range is the minimum; `(->)` has no range.
    let arity = ty.children.len().checked_sub(2)?;

    for child in &ty.children[1..] {
        if let Some(text) = atom_text(child) {
            // `*` is a uniform rest argument, `...` a non-uniform one, and
            // `#:kw` a keyword; none is one positional argument.
            //
            // A bare `:` opens the *proposition* (and, after a second `:`, the
            // object) of a dependent function type. Everything from it onward
            // is latent-proposition syntax, not a `dom` — so
            // `(-> Any Boolean : String)` is a **one**-argument function, and
            // counting positionally would call it three. Declining the whole
            // arrow is the only honest answer, because the `#:+`/`#:-` spelling
            // of the same thing is caught by the `#:` test above and the two
            // must agree.
            if text == "*" || text == "..." || text == ":" || text.starts_with("#:") {
                return None;
            }
            // A second arrow anywhere in the sequence means the infix form, or
            // a shape this rule has not been taught.
            if ARROW_HEADS.contains(&text) {
                return None;
            }
            continue;
        }
        // An optional keyword argument is a parenthesized `(#:kw Type)` pair.
        if child
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|first| first.starts_with("#:"))
        {
            return None;
        }
    }
    Some(arity)
}

/// The `(name, parameter-count)` of a `(define (name p …) …)`, or `None` for
/// every other `define` shape.
fn read_define(view: &ExpressionView) -> Option<(&str, usize)> {
    if atom_text(view.children.first()?) != Some("define") {
        return None;
    }
    let signature = view.children.get(1)?;
    if !is_paren_list(signature) {
        // `(define f (lambda …))` names no parameter list of its own.
        return None;
    }
    // A curried `(define ((f x) y) …)` has a list, not a name, at the head.
    let name = atom_text(signature.children.first()?)?;

    for parameter in &signature.children[1..] {
        // `[y 5]` optional, `[x : Integer]` annotated.
        if is_bracket_list(parameter) {
            return None;
        }
        let text = atom_text(parameter)?;
        // `#:kw k` keyword, `. rest` rest argument.
        if text.starts_with("#:") || text == "." {
            return None;
        }
    }
    Some((name, signature.children.len() - 1))
}

/// Examines one `define`, reading the annotation directly above it.
///
/// Takes the tree because the annotation is a *separate top-level form*; the
/// lookup is a binary search over the top level plus, only on a hit, one
/// materialized form. It is never a scan, and never `tree.root_view()`.
pub fn examine_define(
    tree: &SyntaxTree,
    view: &ExpressionView,
    annotated_define_count: &mut usize,
    violations: &mut Vec<TypedRacketArityMismatchItem>,
) {
    let Some((name, defined_arity)) = read_define(view) else {
        return;
    };
    // One binary search over the top level, then the cheap half first: a span
    // and a source slice, no `ExpressionView`. For a Racket file most `define`s
    // follow another `define`, and those stop here.
    let Some(previous) = preceding_top_level_index(tree, view.span) else {
        return;
    };
    let Some(previous_span) = top_level_span(tree, previous) else {
        return;
    };
    let Some(text) = tree
        .source()
        .get(previous_span.start().get()..previous_span.end().get())
    else {
        return;
    };
    if !opens_with_colon_head(text) {
        return;
    }

    // Only now is a form materialized, and only that one form.
    let Some(annotation) = top_level_form(tree, previous) else {
        return;
    };
    let Some((annotated_name, ty)) = read_annotation(&annotation) else {
        return;
    };
    // Racket is case-sensitive: `(: F …)` does not annotate `(define (f …) …)`.
    if annotated_name != name {
        return;
    }
    let Some(annotated_arity) = arrow_arity(ty) else {
        return;
    };
    *annotated_define_count += 1;
    if annotated_arity != defined_arity {
        violations.push(TypedRacketArityMismatchItem {
            span: view.span,
            annotation_span: annotation.span,
            name: name.to_owned(),
            annotated_arity,
            defined_arity,
        });
    }
}

/// Collects every arity disagreement in one file, with the number of countable
/// annotated definitions as the denominator beside them.
///
/// `dialect_modelled` is derived from [`SCOPE`], the same constant the engine
/// consults.
pub fn build_typed_racket_arity_mismatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TypedRacketArityMismatchItem>> {
    let modelled = SCOPE.includes(dialect);
    let mut annotated_define_count = 0;
    let mut violations = Vec::new();

    if modelled {
        // Only top-level forms can carry a top-level annotation above them, so
        // this never descends.
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            if is_unevaluated_at(tree, view.span) {
                continue;
            }
            examine_define(tree, &view, &mut annotated_define_count, &mut violations);
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("annotated_define_count", json!(annotated_define_count))],
    ))
}

/// The rule's own data guard, shared with the report so the two agree.
#[must_use]
pub fn is_data_at(tree: &SyntaxTree, span: ByteSpan) -> bool {
    is_unevaluated_at(tree, span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<TypedRacketArityMismatchItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse input");
        build_typed_racket_arity_mismatch_report(Path::new("main.rkt"), Dialect::Racket, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<TypedRacketArityMismatchItem> {
        report(input).findings
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_an_annotation_declaring_too_few_arguments() {
        let found = findings("(: f (-> Integer Boolean))\n(define (f x y) #t)\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "f");
        assert_eq!(found[0].annotated_arity, 1);
        assert_eq!(found[0].defined_arity, 2);
    }

    #[test]
    fn flags_an_annotation_declaring_too_many_arguments() {
        let found = findings("(: f (-> Integer String Boolean))\n(define (f x) #t)\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].annotated_arity, 2);
        assert_eq!(found[0].defined_arity, 1);
    }

    /// `(-> Number)` is a *nullary* function returning `Number` — the corollary
    /// that makes a "missing return type" rule impossible, and the arity this
    /// rule must compare against zero.
    #[test]
    fn a_one_element_arrow_is_a_nullary_function_returning_that_type() {
        assert!(findings("(: f (-> Integer))\n(define (f) 1)\n").is_empty());
        let found = findings("(: f (-> Integer))\n(define (f x) 1)\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].annotated_arity, 0);
        assert_eq!(found[0].defined_arity, 1);
    }

    #[test]
    fn a_comment_between_the_annotation_and_the_define_does_not_hide_it() {
        assert_eq!(
            findings("(: f (-> Integer Boolean))\n;; explains f\n(define (f x y) #t)\n").len(),
            1
        );
    }

    #[test]
    fn the_unicode_arrow_is_the_same_arrow() {
        assert_eq!(
            findings("(: f (\u{2192} Integer Boolean))\n(define (f x y) #t)\n").len(),
            1
        );
    }

    #[test]
    fn the_finding_carries_both_spans() {
        let source = "(: f (-> Integer Boolean))\n(define (f x y) #t)\n";
        let found = findings(source);
        let define = &source[found[0].span.start().get()..found[0].span.end().get()];
        let annotation =
            &source[found[0].annotation_span.start().get()..found[0].annotation_span.end().get()];
        assert_eq!(define, "(define (f x y) #t)");
        assert_eq!(annotation, "(: f (-> Integer Boolean))");
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_an_agreeing_annotation() {
        assert!(findings("(: f (-> Integer Boolean))\n(define (f x) #t)\n").is_empty());
        assert!(findings("(: g (-> Integer String Boolean))\n(define (g x y) #t)\n").is_empty());
        assert!(findings("(: h (-> Boolean))\n(define (h) #t)\n").is_empty());
    }

    /// Racket is case-sensitive, so `(: F …)` does not annotate `(define (f …))`.
    #[test]
    fn does_not_pair_names_that_differ_in_case() {
        assert!(findings("(: F (-> Integer Boolean))\n(define (f x y) #t)\n").is_empty());
    }

    #[test]
    fn does_not_pair_an_annotation_with_a_different_name() {
        assert!(findings("(: g (-> Integer Boolean))\n(define (f x y) #t)\n").is_empty());
    }

    #[test]
    fn does_not_flag_a_define_with_no_annotation_above_it() {
        assert!(findings("(define (f x y) #t)\n").is_empty());
        assert!(findings("(define (g) 1)\n(define (f x y) #t)\n").is_empty());
    }

    /// A non-adjacent annotation is a deliberate false negative: only the form
    /// directly above is read, because a scan would be quadratic.
    #[test]
    fn does_not_read_an_annotation_that_is_not_directly_above() {
        assert!(
            findings("(: f (-> Integer Boolean))\n(define (g) 1)\n(define (f x y) #t)\n")
                .is_empty()
        );
    }

    // -- every shape positional counting cannot read -------------------------

    #[test]
    fn declines_a_rest_argument_arrow() {
        assert!(findings("(: f (-> Integer * Integer))\n(define (f x y) 1)\n").is_empty());
        assert!(findings("(: f (-> A B ... C))\n(define (f x) 1)\n").is_empty());
    }

    #[test]
    fn declines_a_keyword_argument_arrow() {
        assert!(findings("(: f (-> Integer #:kw String Boolean))\n(define (f x) #t)\n").is_empty());
        assert!(
            findings("(: f (-> Integer (#:kw String) Boolean))\n(define (f x) #t)\n").is_empty()
        );
    }

    /// The reference also accepts the infix arrow. Only a head-position arrow
    /// is counted.
    #[test]
    fn declines_the_infix_arrow() {
        assert!(findings("(: f (Integer -> Boolean))\n(define (f x y) #t)\n").is_empty());
        // The reference's own `add2`. A positional count of the infix form
        // gives zero arguments against one parameter — the catastrophic case,
        // because every correct infix annotation would be reported.
        assert!(findings("(: add2 (Number -> Number))\n(define (add2 n) n)\n").is_empty());
    }

    /// A dependent function type: the `:` ends the `dom` sequence, so
    /// `(-> Any Boolean : String)` takes **one** argument and a positional
    /// count reads three. Reported by the Typed Racket reference's own
    /// `opt-proposition` grammar; before this was declined the rule fired on
    /// correct code.
    #[test]
    fn declines_an_arrow_carrying_a_proposition_or_object() {
        for ty in [
            "(-> Any Boolean : String)",
            "(-> Any Boolean : (U String Symbol))",
            "(-> Any Boolean : #:+ String #:- (! String))",
            // The object follows a second `:`.
            "(-> Any Boolean : String : (0 0))",
        ] {
            assert!(
                findings(&format!("(: f {ty})\n(define (f x) #t)\n")).is_empty(),
                "{ty} is a dependent function type, not four positional arguments"
            );
        }
    }

    /// The Typed Racket reference's own `is-zero?` example, which mixes a
    /// mandatory keyword, an optional keyword, and a nested arrow. A positional
    /// count reads four arguments against five parameters and reports a
    /// mismatch on code that type-checks.
    #[test]
    fn declines_the_references_own_keyword_example() {
        assert!(
            findings(
                "(: is-zero? (-> Number #:equality (-> Number Number Any) [#:zero Number] Any))\n\
                 (define (is-zero? n #:equality equality #:zero [zero 0]) (equality n zero))\n"
            )
            .is_empty()
        );
    }

    /// The unicode `All`. Its inner arrow may also be spelled infix, with the
    /// parentheses around it omitted — neither reaches a count, because the
    /// head of the type is not an arrow.
    #[test]
    fn declines_every_universal_quantifier_spelling() {
        assert!(findings("(: f (\u{2200} (A) (-> A A)))\n(define (f x y) x)\n").is_empty());
        assert!(findings("(: f (All (A) (A -> A)))\n(define (f x y) x)\n").is_empty());
        assert!(
            findings("(: f (All (A ...) (-> A ... A Integer)))\n(define (f x) 1)\n").is_empty()
        );
    }

    /// Typed Racket's `define` accepts `maybe-tvars` before the header, so the
    /// header is not always at index 1. Reading index 1 as a parameter list
    /// would count `(A)` as one parameter.
    #[test]
    fn declines_a_define_whose_header_is_not_at_index_one() {
        assert!(findings("(: f (-> Integer Integer))\n(define #:forall (A) (f x) 1)\n").is_empty());
        assert!(
            findings("(: f (-> Integer Integer))\n(define #:\u{2200} (A) (f x) 1)\n").is_empty()
        );
    }

    /// A return-type annotation after the header leaves the header itself
    /// plain, but every such parameter is a bracket form, so the define is
    /// declined for that reason and never counted.
    #[test]
    fn declines_a_define_with_a_return_type_annotation() {
        assert!(
            findings("(: f (-> Integer Boolean))\n(define (f [x : Integer]) : Boolean #t)\n")
                .is_empty()
        );
    }

    /// `->*`, `case->` and `All` have something other than `->` at the head of
    /// the type, so they are skipped by construction.
    #[test]
    fn declines_the_other_arrow_constructors() {
        assert!(findings("(: f (->* (Integer) (String) Boolean))\n(define (f x) #t)\n").is_empty());
        assert!(
            findings("(: f (case-> (-> Integer) (-> Integer Integer)))\n(define (f x) 1)\n")
                .is_empty()
        );
        assert!(findings("(: f (All (A) (-> A A)))\n(define (f x y) x)\n").is_empty());
    }

    #[test]
    fn declines_an_arrow_with_no_range() {
        assert!(findings("(: f (->))\n(define (f x) 1)\n").is_empty());
    }

    /// A non-arrow annotation — a plain value type — is not a function type at
    /// all.
    #[test]
    fn declines_a_non_function_annotation() {
        assert!(findings("(: x Integer)\n(define (x) 1)\n").is_empty());
    }

    #[test]
    fn declines_a_curried_define() {
        assert!(findings("(: f (-> Integer Boolean))\n(define ((f x) y) #t)\n").is_empty());
    }

    #[test]
    fn declines_optional_and_annotated_parameters() {
        assert!(findings("(: f (-> Integer Boolean))\n(define (f x [y 5]) #t)\n").is_empty());
        assert!(
            findings("(: f (-> Integer Boolean))\n(define (f [x : Integer] [y : Integer]) #t)\n")
                .is_empty()
        );
    }

    #[test]
    fn declines_keyword_and_rest_parameters() {
        assert!(findings("(: f (-> Integer Boolean))\n(define (f x #:kw k) #t)\n").is_empty());
        assert!(findings("(: f (-> Integer Boolean))\n(define (f x . rest) #t)\n").is_empty());
    }

    #[test]
    fn declines_a_define_bound_to_a_lambda() {
        assert!(findings("(: f (-> Integer Boolean))\n(define f (lambda (x y) #t))\n").is_empty());
    }

    /// A nested `define` has no top-level sibling of its own, so the annotation
    /// above its enclosing form must not be paired with it.
    #[test]
    fn declines_a_nested_define() {
        assert!(
            findings("(: f (-> Integer Boolean))\n(module m racket (define (f x y) #t))\n")
                .is_empty()
        );
    }

    // -- the five quote shapes, in Racket's spelling (`,` is the unquote) -----

    #[test]
    fn a_quoted_define_is_data_and_is_not_flagged() {
        assert!(findings("(: f (-> Integer Boolean))\n'(define (f x y) #t)\n").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings("(: f (-> Integer Boolean))\n(quote (define (f x y) #t))\n").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(findings("(: f (-> Integer Boolean))\n'(a ,(define (f x y) #t))\n").is_empty());
    }

    #[test]
    fn a_quasiquote_without_an_unquote_is_data() {
        assert!(findings("(: f (-> Integer Boolean))\n`(define (f x y) #t)\n").is_empty());
    }

    /// Under an unquote the `define` is code again — but it is no longer a
    /// top-level form, so it has no annotation above it to disagree with. The
    /// point of the test is that the walk reaches it as code rather than
    /// stopping.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_code_again_but_is_no_longer_top_level() {
        assert!(findings("(: f (-> Integer Boolean))\n`(a ,(define (f x y) #t))\n").is_empty());
        let tree = SyntaxTree::parse_with_dialect(
            "(: f (-> Integer Boolean))\n`(a ,(define (f x y) #t))\n",
            Dialect::Racket,
        )
        .expect("parse");
        let template = &tree.root_view().children[1];
        let define = &template.children[1];
        assert!(!is_unevaluated_at(&tree, define.span));
    }

    // -- a string literal ----------------------------------------------------

    #[test]
    fn a_define_spelled_inside_a_string_is_text_not_a_form() {
        assert!(
            findings("(: f (-> Integer Boolean))\n(displayln \"(define (f x y) #t)\")\n")
                .is_empty()
        );
    }

    // -- the wrong dialect ---------------------------------------------------

    /// The same bytes, read as four dialects. `#t` is avoided so that every
    /// reader accepts the source and the *only* difference between the runs is
    /// the dialect scope — Scheme in particular shares Racket's whole surface
    /// syntax here, so it is the sharpest control available.
    #[test]
    fn the_same_bytes_are_flagged_as_racket_and_unmodelled_elsewhere() {
        let source = "(: f (-> Integer Boolean))\n(define (f x y) 1)\n";

        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Racket).expect("parse");
        let racket =
            build_typed_racket_arity_mismatch_report(Path::new("f.rkt"), Dialect::Racket, &tree)
                .expect("build report");
        assert!(racket.dialect_modelled);
        assert_eq!(racket.findings.len(), 1);

        for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report =
                build_typed_racket_arity_mismatch_report(Path::new("f.scm"), dialect, &tree)
                    .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
            assert_eq!(report.summary, vec![("annotated_define_count", json!(0))]);
        }
    }

    #[test]
    fn a_racket_file_is_reported_as_modelled() {
        assert!(report("(define (f x) #t)").dialect_modelled);
    }

    // -- the envelope --------------------------------------------------------

    #[test]
    fn the_summary_counts_every_countable_annotated_define() {
        let report = report(
            "(: a (-> Integer Boolean))\n(define (a x) #t)\n\
             (: b (-> Integer Boolean))\n(define (b x y) #t)\n\
             (define (c x) #t)\n",
        );
        assert_eq!(report.summary, vec![("annotated_define_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    /// A declined shape is not counted either: the denominator is what the rule
    /// could actually check, not what it saw.
    #[test]
    fn a_declined_shape_is_not_counted_in_the_denominator() {
        let report = report("(: f (-> Integer * Integer))\n(define (f x y) 1)\n");
        assert_eq!(report.summary, vec![("annotated_define_count", json!(0))]);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report =
            report("#lang typed/racket\n(: f (-> Integer Boolean))\n(define (f x y) #t)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "typed-racket-arity-mismatch");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("name", json!("f")),
                ("annotated_arity", json!(1)),
                ("defined_arity", json!(2))
            ]
        );
        assert!(finding.message().contains("the last type is the result"));
    }

    // -- the cheap pre-filter ------------------------------------------------

    #[test]
    fn the_source_prefilter_accepts_exactly_a_colon_head() {
        assert!(opens_with_colon_head("(: f Integer)"));
        assert!(opens_with_colon_head("  (:\n  f Integer)"));
        assert!(!opens_with_colon_head("(define (f x) 1)"));
        assert!(!opens_with_colon_head("(:keyword 1)"));
        assert!(!opens_with_colon_head("(:: f Integer)"));
        assert!(!opens_with_colon_head("f"));
        assert!(!opens_with_colon_head(""));
    }
}
