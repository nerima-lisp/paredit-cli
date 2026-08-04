//! `fennel-redundant-do` detection: a `(do …)` in the tail of a form that
//! already has an implicit `do`.
//!
//! # Primary source
//!
//! `fennel-ls`'s `redundant-do`, enabled by default since 0.2.0
//! (`src/fennel-ls/lint.fnl:318-351`):
//!
//! ```text
//! (if (and (. implicit-do-forms (tostring (. ast 1)))
//!          (list? last-body)
//!          (sym? (. last-body 1) :do))
//!     {:ast last-body :message "redundant do"})
//! ```
//!
//! where `implicit-do-forms` is every entry of `(fennel.syntax)` carrying
//! `body-form? = true` (`lint.fnl:37-38`).
//!
//! # Why the head list here is shorter than `fennel-ls`'s
//!
//! `(fennel.syntax)` reports **23** body forms in Fennel 1.6.1:
//!
//! ```text
//! accumulate case case-try collect comment do doto each eval-compiler
//! faccumulate fcollect fn for icollect lambda let macro match match-try
//! when while with-open λ
//! ```
//!
//! but `body-form?` does not mean "accepts more than one body expression", and
//! that is the property this lint actually needs. Nine of the twenty-three
//! reject a second body expression, which makes a trailing `(do …)` in them
//! **load-bearing**. Measured with `fennel --compile` on 1.6.1:
//!
//! ```text
//! (icollect [_ x (ipairs [1])] (print :p) x)
//!   => Compile error: expected exactly one body expression.
//!      Wrap multiple expressions in do
//! (icollect [_ x (ipairs [1])] (do (print :p) x))
//!   => compiles
//! ```
//!
//! `accumulate`, `faccumulate`, `fcollect` and `icollect` all give that error
//! verbatim; `case`, `case-try`, `match` and `match-try` give "expected even
//! number of pattern/body pairs" / "expected every pattern to have a body",
//! because their tail is one clause body and not a sequence of statements.
//! `fennel-ls`'s lint fires on all nine, and its message — "redundant do" —
//! advises deleting a form whose deletion does not compile. That is a false
//! positive in the upstream lint, and this rule declines to reproduce it.
//!
//! `doto` is dropped for a different reason: its trailing forms are calls
//! applied to the object, not statements, so `(doto x (do a b))` means
//! `(do x a b)` and is not a redundant wrapper at all. `comment` is dropped
//! because its contents are inert — reporting on the shape of discarded text
//! is noise.
//!
//! What is left is [`HEADS`]: the forms where a trailing `(do …)` really can be
//! unwrapped, each verified to accept multiple body expressions.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::head_symbol;

pub const DIALECTS: [Dialect; 1] = [Dialect::Fennel];

/// The body forms that genuinely accept more than one trailing expression.
///
/// Each was checked by compiling a two-expression body with `fennel --compile`
/// on 1.6.1; see the module docs for the nine that failed and are absent here.
/// `λ` is `lambda`'s Unicode spelling and is a distinct head in the index —
/// `head_key` does not fold it — so it needs its own entry.
pub const HEADS: [&str; 13] = [
    "collect",
    "do",
    "each",
    "eval-compiler",
    "fn",
    "for",
    "lambda",
    "let",
    "macro",
    "when",
    "while",
    "with-open",
    "λ",
];

/// One redundant `(do …)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedundantDo {
    /// The inner `(do …)`, which is what the finding points at.
    pub span: ByteSpan,
    /// The enclosing form — the node the engine dispatched on, and so the span
    /// the rule's quote guard is asked about. See `fennel_bad_unpack`'s
    /// `form_span` for why the two spans can disagree.
    pub form_span: ByteSpan,
    /// The enclosing form's head, so the message can name it.
    pub outer: String,
}

/// The index of the first element of `head`'s body.
///
/// `do` and `eval-compiler` start their body immediately; every other head
/// here takes exactly one leading element first — a binding vector, a
/// parameter list, a condition, a name — and the body follows it.
///
/// This matters only at the boundary: without it `(let [] (do …))` and
/// `(fn [] (do …))` would be indistinguishable from `(do (do …))`, and a form
/// whose *only* element is the `(do …)` would be misread. A `(do …)` sitting
/// in the binding position of `(let (do …) …)` is malformed code, not a
/// redundant wrapper.
const fn body_start(head: &str) -> usize {
    // `matches!` over `&str` is not const-evaluable, so this is a byte compare.
    if str_eq(head, "do") || str_eq(head, "eval-compiler") {
        1
    } else {
        2
    }
}

const fn str_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Examines one form.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<RedundantDo> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let outer = head_symbol(view)?;
    if !HEADS.contains(&outer) {
        return None;
    }
    let last = view.children.last()?;
    // The last child must be *in* the body, not the sole binding vector or
    // parameter list of a form whose body is empty.
    let last_index = view.children.len() - 1;
    if last_index < body_start(outer) {
        return None;
    }
    if head_symbol(last)? != "do" {
        return None;
    }
    Some(RedundantDo {
        span: last.span,
        form_span: view.span,
        outer: outer.to_owned(),
    })
}

/// Every redundant `(do …)` in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<RedundantDo> {
    let root = tree.root_view();
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// Every implicit-do form in the file. The denominator.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| HEADS.contains(&head)) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outers(source: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Fennel).expect("parse");
        collect(Dialect::Fennel, &tree)
            .into_iter()
            .map(|item| item.outer)
            .collect()
    }

    #[test]
    fn every_declared_head_is_reported_when_it_wraps_a_do() {
        for head in HEADS {
            let leading = if body_start(head) == 1 { "" } else { "[] " };
            let source = format!("({head} {leading}(do (f) (g)))");
            assert_eq!(outers(&source), vec![head], "{head} was not reported");
        }
    }

    #[test]
    fn the_nine_single_body_forms_are_not_reported() {
        // Each of these needs the `do` to compile; deleting it is an error.
        // `fennel --compile` on 1.6.1 refuses the unwrapped form for all nine.
        for head in [
            "accumulate",
            "faccumulate",
            "fcollect",
            "icollect",
            "case",
            "case-try",
            "match",
            "match-try",
        ] {
            let source = format!("({head} x _ (do (f) (g)))");
            assert!(
                outers(&source).is_empty(),
                "{head} was reported, but its `do` is load-bearing"
            );
        }
        assert!(outers("(doto x (do (f) (g)))").is_empty());
        assert!(outers("(comment (do (f) (g)))").is_empty());
    }

    #[test]
    fn only_the_tail_position_counts() {
        // A `do` in the middle of a body is a sequencing choice the author
        // made and unwrapping it would change nothing; `fennel-ls` looks only
        // at the last body form and so does this.
        assert!(outers("(fn [] (do (f) (g)) (h))").is_empty());
    }

    #[test]
    fn a_form_with_no_body_is_not_misread_as_wrapping_its_binding_vector() {
        // `(let (do …))` is malformed — a `do` where the binding vector goes.
        // Without `body_start` this reads as "the last child is a `do`".
        assert!(outers("(let (do (f)))").is_empty());
        assert!(outers("(fn (do (f)))").is_empty());
        // The control: with a binding vector present it is a real body.
        assert_eq!(outers("(let [] (do (f)))"), vec!["let"]);
    }

    #[test]
    fn a_bare_do_still_needs_a_body_element() {
        assert!(outers("(do)").is_empty());
        assert_eq!(outers("(do (do (f)))"), vec!["do"]);
    }

    #[test]
    fn a_body_that_is_not_a_do_is_left_alone() {
        assert!(outers("(when x (f) (g))").is_empty());
        assert!(outers("(fn [] (let [a 1] a))").is_empty());
    }

    #[test]
    fn the_unicode_lambda_is_its_own_head() {
        // `head_key` returns the head verbatim for Fennel, so `λ` and `lambda`
        // are different index keys and both must be declared.
        assert_eq!(outers("(λ [] (do (f) (g)))"), vec!["λ"]);
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        for dialect in [Dialect::Janet, Dialect::CommonLisp, Dialect::Clojure] {
            let tree =
                SyntaxTree::parse_with_dialect("(when x (do (f) (g)))", dialect).expect("parse");
            assert!(collect(dialect, &tree).is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn the_candidate_count_counts_every_implicit_do_form() {
        let tree = SyntaxTree::parse_with_dialect(
            "(when a (do (f))) (when b (f)) (fn [] (g))",
            Dialect::Fennel,
        )
        .expect("parse");
        // Three implicit-do forms; the inner `(do (f))` is a fourth.
        assert_eq!(candidate_count(Dialect::Fennel, &tree), 4);
        assert_eq!(collect(Dialect::Fennel, &tree).len(), 1);
    }
}
