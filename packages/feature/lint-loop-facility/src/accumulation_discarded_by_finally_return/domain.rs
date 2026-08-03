//! A `loop` that accumulates into its **implicit** result and then returns
//! something else from `finally`, so every value it accumulated is discarded.
//!
//! CLHS 6.1.1.3 gives an extended `loop` at most one implicit result, produced
//! by the accumulation clauses that name no `into` variable. CLHS 6.1.2.3 makes
//! `finally`'s `(return …)` return from the loop's implicit `nil` block, which
//! pre-empts that result. Since an implicit accumulation has no name, a
//! `finally (return …)` can never return it — so the two together always mean
//! the accumulated value is built, consed, and thrown away.
//!
//! # Measured under SBCL 2.6.0
//!
//! ```text
//! (loop for x in '(1 2 3) collect x finally (return :other))   => :OTHER
//! ```
//!
//! The list `(1 2 3)` is fully built on the way there. SBCL emits no warning:
//! the accumulation is "used" as far as the compiler is concerned, because the
//! collection machinery reads its own head and tail variables.
//!
//! # Why this is not one of the four rules that already read `loop`
//!
//! `lint-form-shape`'s `loop-collect-into-immediately-returned` is about the
//! opposite shape — `collect … into acc` *plus* `finally (return acc)`, where
//! the value **is** returned and the `into` is merely redundant. It requires
//! `into_count == 1` and requires the returned symbol to be that accumulator,
//! so it cannot fire here.
//!
//! `lint-iteration-flow`'s `loop-unreachable-finally-clause` explicitly refuses
//! this reading, and names it as a separate complaint in its own module doc:
//! "the `collect` is still evaluated on every iteration — the accumulated list
//! is merely discarded. That is a different complaint, and calling it
//! unreachable would be false." This is that different complaint.
//!
//! # What this rule deliberately does not flag
//!
//! - **An accumulation that names an `into` target.** Then the value has a name
//!   and the `finally` may well be returning something computed from it.
//! - **A `named` loop.** `loop named outer` establishes the block `outer`
//!   rather than `nil`, so a bare `(return …)` inside one does not return from
//!   the loop at all — under SBCL it does not even compile ("return for unknown
//!   block: NIL"). Whatever such a form means, it is not this.
//! - **`finally (return-from …)`**, a deliberate non-local exit to a named
//!   block further out.
//! - **A conditional return**, `finally (when p (return v))`. That discards the
//!   accumulation only on some paths, and the unconditional case is the one
//!   worth a rule. Only a `(return …)` that is a *direct* form of the `finally`
//!   clause counts — the same convention `loop-unreachable-finally-clause`
//!   uses, which keeps the two disjoint.
//! - **Anything in a form [`crate::loop_grammar`] declines to read**, or
//!   reached only as quoted data.
//!
//! Scope: Common Lisp only.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::loop_grammar::{is_accumulation_verb, read_loop_form};
use crate::shared::{is_call_to, symbol_word};

/// One `loop` whose implicit accumulation a `finally (return …)` discards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardedAccumulation {
    /// The span of the accumulation clause keyword whose result is lost.
    pub span: ByteSpan,
    /// That clause's verb, lowercased.
    pub verb: String,
}

impl DiscardedAccumulation {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "this `{}` accumulates into the loop's implicit result, which the `finally \
             (return …)` then discards; the accumulated value is built on every iteration and \
             never read",
            self.verb
        )
    }
}

/// Whether one `finally` clause form is an unconditional `(return …)`.
///
/// `return-from` is excluded on purpose: it is a deliberate exit to a named
/// block further out, not a claim about this loop's value.
fn is_unconditional_return(view: &ExpressionView) -> bool {
    is_call_to(view, "return")
}

/// Every discarded implicit accumulation in one `loop` form.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<DiscardedAccumulation> {
    let Some(form) = read_loop_form(view) else {
        return Vec::new();
    };

    let mut implicit: Vec<(usize, String)> = Vec::new();
    let mut returns = false;
    let mut in_finally = false;

    for (index, token) in form.tokens.iter().enumerate() {
        match token.keyword() {
            Some("named") => {
                // A named loop rebinds the block a bare `return` targets, so
                // none of the reasoning below holds.
                return Vec::new();
            }
            Some("finally") => in_finally = true,
            Some(word) if is_accumulation_verb(word) => {
                in_finally = false;
                // `VERB form [into var]`: the clause is implicit unless the
                // token two along is the `into` keyword.
                let named = form
                    .tokens
                    .get(index + 2)
                    .is_some_and(|token| token.keyword() == Some("into"));
                if !named {
                    implicit.push((index, word.to_owned()));
                }
            }
            Some(_) => in_finally = false,
            None => {
                if in_finally && is_unconditional_return(token.view) {
                    returns = true;
                }
            }
        }
    }

    if !returns {
        return Vec::new();
    }
    implicit
        .into_iter()
        .map(|(index, verb)| DiscardedAccumulation {
            span: form.tokens[index].view.span,
            verb,
        })
        .collect()
}

/// The number of `loop` forms carrying an implicit accumulation, which is the
/// population this rule filters. A zero-finding sweep over zero candidates is a
/// false clean.
#[must_use]
pub fn candidate_count(view: &ExpressionView) -> usize {
    let Some(form) = read_loop_form(view) else {
        return 0;
    };
    // Written as two positive early exits rather than `!x.is_some_and(p)`,
    // which the Nix toolchain's newer clippy rejects in favour of
    // `x.is_none_or(!p)`; the destructuring form sidesteps the choice.
    usize::from(form.tokens.iter().enumerate().any(|(index, token)| {
        let Some(word) = token.keyword() else {
            return false;
        };
        if !is_accumulation_verb(word) {
            return false;
        }
        form.tokens
            .get(index + 2)
            .is_none_or(|next| next.keyword() != Some("into"))
    }))
}

/// Whether a symbol names the accumulation verb set, exposed for the corpus
/// harness's own denominator.
#[must_use]
pub fn verb_of(view: &ExpressionView) -> Option<String> {
    symbol_word(view).filter(|word| is_accumulation_verb(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn found(input: &str) -> Vec<DiscardedAccumulation> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        examine(&view)
    }

    /// SBCL: returns `:OTHER`, having built `(1 2 3)` on the way.
    #[test]
    fn flags_an_implicit_collect_discarded_by_finally_return() {
        let items = found("(loop for x in items collect x finally (return :other))");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].verb, "collect");
    }

    #[test]
    fn flags_every_accumulation_verb_in_the_implicit_position() {
        for verb in [
            "collect",
            "collecting",
            "append",
            "nconc",
            "sum",
            "count",
            "maximize",
            "minimize",
        ] {
            let items = found(&format!(
                "(loop for x in items {verb} x finally (return :other))"
            ));
            assert_eq!(items.len(), 1, "{verb} was not reported: {items:?}");
            assert_eq!(items[0].verb, verb);
        }
    }

    // --- the negatives -----------------------------------------------------

    /// The control that makes the rule about the discard rather than about
    /// `finally`: no `return`, so the implicit result is what the loop yields.
    #[test]
    fn does_not_flag_a_finally_that_does_not_return() {
        assert!(found("(loop for x in items collect x finally (report))").is_empty());
    }

    #[test]
    fn does_not_flag_an_accumulation_with_an_into_target() {
        assert!(
            found("(loop for x in items collect x into acc finally (return (nreverse acc)))")
                .is_empty()
        );
    }

    /// `loop named outer` establishes block `outer`, not `nil`, so a bare
    /// `return` does not return from the loop — SBCL will not even compile it.
    #[test]
    fn does_not_flag_a_named_loop() {
        assert!(
            found("(loop named outer for x in items collect x finally (return :other))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_return_from() {
        assert!(found("(loop for x in items collect x finally (return-from f :other))").is_empty());
    }

    #[test]
    fn does_not_flag_a_conditional_return() {
        assert!(
            found("(loop for x in items collect x finally (when p (return :other)))").is_empty()
        );
    }

    /// A `return` written in a `do` body is not a `finally` return.
    #[test]
    fn does_not_flag_a_return_outside_finally() {
        assert!(
            found("(loop for x in items collect x do (when (p x) (return :early)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_with_no_accumulation() {
        assert!(found("(loop for x in items do (print x) finally (return :other))").is_empty());
    }

    #[test]
    fn says_nothing_about_a_form_the_reader_declines() {
        assert!(
            found("(loop for k being the hash-keys of h collect k finally (return 1))").is_empty()
        );
        assert!(found("'(loop for x in items collect x finally (return :other))").is_empty());
    }

    // --- the denominator ---------------------------------------------------

    #[test]
    fn an_implicit_accumulation_is_a_candidate_even_when_clean() {
        let tree =
            SyntaxTree::parse_with_dialect("(loop for x in items collect x)", Dialect::CommonLisp)
                .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert_eq!(candidate_count(&view), 1);
        assert!(examine(&view).is_empty());
    }

    #[test]
    fn an_into_accumulation_is_not_a_candidate() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x in items collect x into acc finally (return acc))",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert_eq!(candidate_count(&view), 0);
    }
}
