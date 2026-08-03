//! A `loop` that accumulates `into` a variable nothing ever reads, so the loop
//! returns `nil` and the accumulated value is unreachable.
//!
//! CLHS 6.1.3 is explicit that naming an `into` variable takes the value *out*
//! of the loop's implicit result: "If `into` is used, … the loop does not
//! return the accumulation automatically." The variable's scope ends with the
//! loop, so if no `finally`, `do`, or conditional clause reads it, the value
//! cannot escape and the loop yields `nil`.
//!
//! # Measured under SBCL 2.6.0
//!
//! ```text
//! (loop for x in '(1 2 3) collect x into acc)    => NIL
//!   ; STYLE-WARNING: The variable ACC is assigned but never read.
//! (loop for x in '(1 2 3) sum x into total)      => NIL
//!   ; no warning at all
//! ```
//!
//! The second line is why this rule earns its place. For a list accumulator
//! SBCL can see the variable is never read and says so — a style warning most
//! builds do not fail on, but a diagnostic. For a *numeric* accumulator it
//! structurally cannot: `sum … into total` expands to `total = total + x`, so
//! `total` **is** read, by its own accumulation. The compiler has nothing to
//! report and the loop silently returns `nil`.
//!
//! # Why this is not `loop-collect-into-immediately-returned`
//!
//! That rule (in `lint-form-shape`) fires on `collect … into acc` *plus*
//! `finally (return acc)` — where the accumulator is read exactly twice, and
//! the complaint is that the `into` is redundant. This rule fires only when it
//! is read exactly **once**, at the `into` itself. The two populations are
//! disjoint by construction, and a test pins that.
//!
//! # What this rule deliberately does not flag
//!
//! - **An accumulator read anywhere else in the loop**, at any depth and in any
//!   clause — `finally`, `do`, `when`, another accumulation's operand. One
//!   further occurrence is enough to stay silent.
//! - **An accumulator named by two `into` clauses**, which is the legal way to
//!   accumulate from two branches. It reads as two occurrences and is not
//!   reported.
//! - **An accumulator whose name is shadowed or rebound**, since any occurrence
//!   at all suppresses the finding.
//! - **Anything in a form [`crate::loop_grammar`] declines to read**, or
//!   reached only as quoted data.
//!
//! Scope: Common Lisp only.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::loop_grammar::{LoopToken, is_accumulation_verb, read_loop_form};
use crate::shared::count_evaluated_reads;

/// One `into` accumulator that nothing reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadAccumulator {
    /// The span of the accumulator name.
    pub span: ByteSpan,
    pub name: String,
    /// The accumulation verb that fills it, lowercased.
    pub verb: String,
}

impl UnreadAccumulator {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "`{}` accumulates into `{}`, which nothing in the loop reads; naming an `into` \
             variable takes the value out of the loop's result, so this loop returns nil and \
             the accumulated value is unreachable",
            self.verb, self.name
        )
    }
}

/// How many times `name` is read anywhere in `view`.
///
/// Delegates to [`count_evaluated_reads`], which carries the two-counter quote
/// model. That is load-bearing rather than tidy: the commonest way a Common
/// Lisp macro reads a `loop` accumulator is `finally (return \`(progn
/// ,@acc))`, where the reference sits inside a quasiquote and escapes back to
/// code through an unquote. See that function's doc for the 41 false positives
/// the naive version produced over SBCL's own sources.
fn count_occurrences(view: &ExpressionView, name: &str) -> usize {
    count_evaluated_reads(view, name)
}

/// Every unread `into` accumulator in one `loop` form.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<UnreadAccumulator> {
    let Some(form) = read_loop_form(view) else {
        return Vec::new();
    };
    let mut found = Vec::new();

    for (index, token) in form.tokens.iter().enumerate() {
        // The grammar is `VERB form into name`. Anchor on `into` and read
        // backwards, so a verb whose operand is itself compound still matches.
        if token.keyword() != Some("into") {
            continue;
        }
        let verb = match form.tokens.get(index.wrapping_sub(2)).and_then(|token| {
            token
                .keyword()
                .filter(|word| is_accumulation_verb(word))
                .map(str::to_owned)
        }) {
            Some(verb) => verb,
            None => continue,
        };
        let Some(target) = form.tokens.get(index + 1) else {
            continue;
        };
        let Some(name) = target.operand_symbol() else {
            continue;
        };
        // The occurrence at the `into` itself is the one we just read. Anything
        // beyond it is a use, and one use is enough to stay silent.
        if count_occurrences(view, name) != 1 {
            continue;
        }
        found.push(UnreadAccumulator {
            span: target.view.span,
            name: name.to_owned(),
            verb,
        });
    }
    found
}

/// The number of `into` accumulation clauses in one `loop`, which is the
/// population this rule filters.
#[must_use]
pub fn candidate_count(view: &ExpressionView) -> usize {
    let Some(form) = read_loop_form(view) else {
        return 0;
    };
    let mut count = 0;
    for (index, token) in form.tokens.iter().enumerate() {
        if token.keyword() != Some("into") {
            continue;
        }
        let verb_is_accumulation = form
            .tokens
            .get(index.wrapping_sub(2))
            .and_then(LoopToken::keyword)
            .is_some_and(is_accumulation_verb);
        if verb_is_accumulation {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn found(input: &str) -> Vec<UnreadAccumulator> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        examine(&view)
    }

    /// SBCL: returns NIL, with only a style warning.
    #[test]
    fn flags_a_list_accumulator_nothing_reads() {
        let items = found("(loop for x in items collect x into acc)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].name, "acc");
        assert_eq!(items[0].verb, "collect");
    }

    /// The case that earns the rule: SBCL emits *no* diagnostic, because
    /// `total` is read by its own accumulation.
    #[test]
    fn flags_a_numeric_accumulator_nothing_reads() {
        let items = found("(loop for x in items sum x into total)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].name, "total");
        assert_eq!(items[0].verb, "sum");
    }

    #[test]
    fn flags_a_verb_whose_operand_is_compound() {
        let items = found("(loop for x in items collect (transform x) into acc)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].name, "acc");
    }

    // --- the negatives -----------------------------------------------------

    /// The control: one further occurrence, and the value escapes.
    #[test]
    fn does_not_flag_an_accumulator_returned_from_finally() {
        assert!(found("(loop for x in items collect x into acc finally (return acc))").is_empty());
    }

    #[test]
    fn does_not_flag_an_accumulator_read_in_a_body_clause() {
        assert!(
            found("(loop for x in items collect x into acc do (print (length acc)))").is_empty()
        );
        assert!(
            found("(loop for x in items collect x into acc when (null acc) do (warn \"empty\"))")
                .is_empty()
        );
    }

    /// Two branches accumulating into one variable is the legal idiom, and it
    /// reads as two occurrences.
    #[test]
    fn does_not_flag_an_accumulator_named_by_two_into_clauses() {
        assert!(
            found(
                "(loop for x in items when (evenp x) collect x into acc else collect 0 into acc)"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_implicit_accumulation() {
        assert!(found("(loop for x in items collect x)").is_empty());
    }

    /// `into` after something that is not an accumulation verb is not this
    /// rule's shape.
    #[test]
    fn does_not_flag_a_non_accumulation_into() {
        assert!(found("(loop for x in items do (merge-into x) collect x)").is_empty());
    }

    #[test]
    fn says_nothing_about_a_form_the_reader_declines() {
        assert!(found("(loop for k being the hash-keys of h collect k into acc)").is_empty());
        assert!(found("'(loop for x in items collect x into acc)").is_empty());
    }

    /// A quoted mention of the name is data, not a read, so it must not
    /// suppress the finding.
    #[test]
    fn a_quoted_mention_of_the_name_is_not_a_read() {
        let items = found("(loop for x in items collect x into acc do (print 'acc))");
        assert_eq!(items.len(), 1, "{items:?}");
    }

    /// The corpus regression, verbatim in shape from
    /// `sbcl/contrib/sb-gmp/gmp.lisp:202`. An accumulator read through
    /// `,@name` inside a `finally` template *is* read, and an earlier version
    /// of this rule reported 41 of these across SBCL's own sources — every one
    /// a false positive.
    #[test]
    fn an_accumulator_spliced_from_a_finally_template_is_read() {
        assert!(
            found(
                "(defmacro define-twoarg-mpz-funs (funs)\n  \
                 (loop for i in funs collect `(define-alien-routine ,i void) into defines\n  \
                 finally (return `(progn (declaim (inline ,@funs)) ,@defines))))"
            )
            .is_empty()
        );
    }

    /// The same shape with a plain `,` unquote rather than `,@`, and the
    /// second corpus shape — an accumulator spliced from a template nested
    /// inside a call, as at `sbcl/src/compiler/array-tran.lisp:2749`.
    #[test]
    fn an_accumulator_unquoted_from_a_nested_template_is_read() {
        assert!(
            found("(loop for x in items collect x into acc finally (return `(list ,acc)))")
                .is_empty()
        );
        assert!(
            found(
                "(loop for s in saetps\n  \
                 collect (typecode s) into tags\n  \
                 finally (return (specifier-type `(values (member ,@tags)))))"
            )
            .is_empty()
        );
    }

    /// The complement: a name mentioned only under a *hard* quote is the
    /// symbol, not the variable, so it does not suppress. This is the
    /// distinction a single depth counter cannot express, and the pair of
    /// tests is what keeps the fix from over-correcting into silence.
    #[test]
    fn a_hard_quoted_template_mention_does_not_suppress() {
        let items = found("(loop for x in items collect x into acc finally (print '(a acc b)))");
        assert_eq!(items.len(), 1, "{items:?}");
    }

    // --- disjointness from the shipped rule it sits next to ----------------

    /// `loop-collect-into-immediately-returned` fires on exactly two
    /// occurrences of the accumulator; this rule fires on exactly one. Pinned
    /// so neither can drift into the other's population.
    #[test]
    fn the_two_into_rules_have_disjoint_populations() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x in items collect x into acc finally (return acc))",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert_eq!(count_occurrences(&view, "acc"), 2);
        assert!(examine(&view).is_empty());
    }

    // --- the denominator ---------------------------------------------------

    #[test]
    fn an_into_clause_is_a_candidate_even_when_clean() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x in items collect x into acc finally (return acc))",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert_eq!(candidate_count(&view), 1);
        assert!(examine(&view).is_empty());
    }
}
