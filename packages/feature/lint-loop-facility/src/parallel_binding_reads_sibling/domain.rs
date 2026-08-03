//! `loop` variable clauses joined by `and` bind **in parallel**, so a clause's
//! initial value cannot read a sibling's.
//!
//! CLHS 6.1.1.4 ("Expanding Loop Forms") and the `for-as-*` grammar in 6.1.2.1
//! give `and` this meaning: successive `for`/`as`/`with` clauses bind
//! sequentially, like `let*`, while clauses joined by `and` bind
//! simultaneously, like `let`. It is the `do` versus `do*` distinction wearing
//! `loop` syntax, and it is invisible at the call site — the two spellings
//! differ by three characters and produce different answers.
//!
//! # Measured under SBCL 2.6.0
//!
//! The silent case, which is the one worth a rule:
//!
//! ```text
//! (loop for a from 1 to 3 and b = (* a 10) collect (list a b))
//!   => ((1 10) (2 10) (3 20))      ; no warning at all — `b` lags one behind
//! (loop for a from 1 to 3 for b = (* a 10) collect (list a b))
//!   => ((1 10) (2 20) (3 30))      ; the sequential spelling, correct
//! ```
//!
//! And the loud cases, which the same shape also produces:
//!
//! ```text
//! (loop for a = 1 then (1+ a) and b = (* a 10) repeat 3 collect (list a b))
//!   => TYPE-ERROR: Value of A in (* A 10) is NIL, not a NUMBER
//! (loop with a = 1 and b = (* a 2) repeat 1 collect (list a b))
//!   => UNBOUND-VARIABLE: The variable A is unbound
//! (loop for a = '(1 2) and b in a collect (list a b))
//!   => UNBOUND-VARIABLE: The variable A is unbound
//! ```
//!
//! # The `then` exclusion, which is what keeps this rule honest
//!
//! A sibling reference in a `then` **step** form is not a defect; it is the
//! standard "previous element" idiom, and it *requires* `and`:
//!
//! ```text
//! (loop for x in '(1 2 3) and prev = nil then x collect (cons prev x))
//!   => ((NIL . 1) (1 . 2) (2 . 3))   ; correct — `prev` is the previous `x`
//! (loop for x in '(1 2 3) for prev = nil then x collect (cons prev x))
//!   => ((NIL . 1) (2 . 2) (3 . 3))   ; the sequential spelling is the wrong one
//! ```
//!
//! So the rule reports a reference from an **init** position only — the `=`
//! init form and the `in`/`on`/`across`/`from`/`to`/`by` operands, all of which
//! are evaluated once at loop setup, when every sibling in the group still
//! holds `nil` or is not bound at all. A `then` form is never reported.
//!
//! # What this rule deliberately does not flag
//!
//! - **Successive `for … for …` clauses**, which bind sequentially. Reading an
//!   earlier clause's variable there is correct and extremely common.
//! - **A self-reference**, `for a = 1 then (1+ a)`. A clause may always read
//!   its own variable.
//! - **A reference inside a form that binds names of its own** — a nested
//!   `let`, `lambda`, `destructuring-bind`, or `loop`. The inner binding may
//!   shadow the sibling, and proving it does not costs a scope analysis this
//!   layer does not have. See `loop_grammar::OPAQUE_BINDERS`.
//! - **A reference under a reader prefix.** `'a` and `#'a` do not read the
//!   variable `a`.
//! - **Anything in a form [`crate::loop_grammar`] declines to read** — an
//!   iteration path (`being the hash-keys …`) or a simple `loop`.
//! - **Anything reached only as quoted data**, at any depth, including a `loop`
//!   written in a `defmacro` template.
//!
//! Scope: Common Lisp only.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::loop_grammar::{read_loop_form, reads_variable};

/// One clause whose initial value reads a parallel sibling's variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiblingRead {
    /// The span of the init form that does the reading.
    pub span: ByteSpan,
    /// The variable the reading clause binds.
    pub reader: String,
    /// The sibling variable it reads.
    pub sibling: String,
}

impl SiblingRead {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "`{}` is bound in parallel with `{}` by `and`, so its initial value reads `{}` \
             before that clause has run; write successive `for` clauses instead of `and` to \
             bind them in sequence",
            self.reader, self.sibling, self.sibling
        )
    }
}

/// Every parallel-binding sibling read in one `loop` form.
///
/// Returns an empty vector both for a clean `loop` and for one this reader
/// declines to model; the caller's denominator, not this function, is what
/// tells the two apart.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<SiblingRead> {
    let Some(form) = read_loop_form(view) else {
        return Vec::new();
    };
    let mut found = Vec::new();

    for group in form.parallel_groups() {
        // A group of one is an ordinary variable clause and carries no risk.
        //
        // This is a **cost** guard, not a correctness one, and mutation-testing
        // says so: removing it fails no test, because a lone binding has no
        // sibling for the loop below to find. It stays because it is what keeps
        // the overwhelmingly common single-clause `loop` off the nested walk
        // below entirely — the corpus sweep measured 16598 modelled `loop`
        // forms against 132 groups of two or more, so this exits early for
        // better than 99% of them.
        if group.bindings.len() < 2 {
            continue;
        }
        for (position, binding) in group.bindings.iter().enumerate() {
            for other_position in 0..group.bindings.len() {
                // A clause reading its *own* variable in its *own* init form
                // is a different complaint — the variable is `nil` there — and
                // is deliberately out of scope. This rule is about reads
                // *across* a parallel group, which is what `and` makes
                // surprising; a self-read is equally wrong however the clause
                // was spelled, so it is not evidence about `and` at all.
                if other_position == position {
                    continue;
                }
                for sibling in &group.bindings[other_position].names {
                    // There is deliberately no "the reading clause binds this
                    // name too" guard here. An earlier version had one and
                    // mutation-testing found it killed no test; chasing that
                    // showed it was dead code rather than a missing test. Its
                    // only reachable input is two clauses of one parallel group
                    // binding the same name, and SBCL 2.6.0 rejects that at
                    // macroexpansion time — `(loop for (a b) in pairs and
                    // a = (1+ a) collect a)` fails with "Duplicated variable
                    // A". A guard whose only trigger cannot compile earns
                    // nothing.
                    //
                    // A clause reading *its own* variable is a different thing
                    // and is still never reported: `other_position ==
                    // position` is skipped above, and a lone clause never
                    // reaches here at all.
                    for &operand in &binding.init_operands {
                        let operand = form.tokens[operand].view;
                        if !reads_variable(operand, sibling) {
                            continue;
                        }
                        found.push(SiblingRead {
                            span: operand.span,
                            reader: binding
                                .names
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "the clause".to_owned()),
                            sibling: sibling.clone(),
                        });
                    }
                }
            }
        }
    }
    found
}

/// The number of `loop` forms this rule could say anything about, for the
/// denominator a corpus sweep needs. A zero-finding sweep over zero candidates
/// is a false clean.
#[must_use]
pub fn candidate_count(view: &ExpressionView) -> usize {
    read_loop_form(view).map_or(0, |form| {
        form.parallel_groups()
            .iter()
            .filter(|group| group.bindings.len() >= 2)
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn reads(input: &str) -> Vec<SiblingRead> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        examine(&view)
    }

    /// The silent case: SBCL gives `((1 10) (2 10) (3 20))` and no warning.
    #[test]
    fn flags_an_and_joined_init_that_reads_its_sibling() {
        let found = reads("(loop for a from 1 to 3 and b = (* a 10) collect (list a b))");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].reader, "b");
        assert_eq!(found[0].sibling, "a");
    }

    /// The `with` spelling, which SBCL rejects outright as an unbound variable.
    #[test]
    fn flags_an_and_joined_with_clause() {
        let found = reads("(loop with a = 1 and b = (* a 2) repeat 1 collect (list a b))");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].reader, "b");
    }

    /// A forward reference is the same defect: SBCL gives the same lagged
    /// `((1 10) (2 10) (3 20))`.
    #[test]
    fn flags_a_reference_to_a_later_sibling() {
        let found = reads("(loop for b = (* a 10) and a from 1 to 3 collect (list a b))");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].reader, "b");
        assert_eq!(found[0].sibling, "a");
    }

    /// An `in` operand is an init position too — SBCL: unbound variable A.
    #[test]
    fn flags_a_sibling_read_in_an_in_operand() {
        let found = reads("(loop for a = (make-list 2) and b in a collect (list a b))");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].sibling, "a");
    }

    // --- the negatives, each differing by exactly the thing the rule is about

    /// The sequential spelling. This is the control that makes the rule about
    /// `and` rather than about reading a loop variable.
    #[test]
    fn does_not_flag_successive_for_clauses() {
        assert!(reads("(loop for a from 1 to 3 for b = (* a 10) collect (list a b))").is_empty());
        assert!(reads("(loop with a = 1 with b = (* a 2) repeat 1 collect (list a b))").is_empty());
    }

    /// The "previous element" idiom, which requires `and` and is correct.
    /// SBCL: `((NIL . 1) (1 . 2) (2 . 3))`.
    #[test]
    fn does_not_flag_a_sibling_read_in_a_then_step_form() {
        assert!(
            reads("(loop for x in items and prev = nil then x collect (cons prev x))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_self_reference() {
        assert!(reads("(loop for a = 1 then (1+ a) repeat 3 collect a)").is_empty());
    }

    /// A clause reading its own variable in its own *init* form, inside a
    /// parallel group. This is out of scope on purpose: it is equally wrong
    /// however the clause was spelled, so it says nothing about `and`, which is
    /// what this rule is about. Pinned because mutation-testing showed the
    /// `other_position == position` guard was otherwise killing no test.
    #[test]
    fn does_not_flag_a_clause_reading_its_own_variable_in_its_init() {
        assert!(reads("(loop for a = (1+ a) and b = 1 repeat 1 collect (list a b))").is_empty());
    }

    #[test]
    fn does_not_flag_an_and_group_with_no_cross_reference() {
        assert!(
            reads("(loop for a from 1 to 3 and b from 10 to 30 by 10 collect (list a b))")
                .is_empty()
        );
    }

    /// `and` between main clauses joins body code, not bindings.
    #[test]
    fn does_not_flag_an_and_between_main_clauses() {
        assert!(reads("(loop for x in items collect x and count x)").is_empty());
        assert!(reads("(loop for x in items do (a x) and do (b x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_read_that_a_nested_binder_could_shadow() {
        assert!(
            reads("(loop for a from 1 to 3 and b = (let ((a 5)) (* a 10)) collect (list a b))")
                .is_empty()
        );
        assert!(
            reads("(loop for a from 1 to 3 and b = (lambda (a) a) collect (list a b))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_quoted_or_sharp_quoted_sibling_name() {
        assert!(reads("(loop for a from 1 to 3 and b = 'a collect (list a b))").is_empty());
        assert!(reads("(loop for a from 1 to 3 and b = #'a collect (list a b))").is_empty());
    }

    /// A variable spelled like a clause keyword must not break the reading.
    #[test]
    fn a_variable_named_like_a_keyword_is_still_read_correctly() {
        let found = reads("(loop for count from 1 to 3 and b = (* count 10) collect b)");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].sibling, "count");
    }

    #[test]
    fn says_nothing_about_a_form_the_reader_declines() {
        assert!(reads("(loop for k being the hash-keys of table collect k)").is_empty());
        assert!(reads("(loop (process))").is_empty());
        assert!(reads("'(loop for a from 1 to 3 and b = (* a 10) collect b)").is_empty());
    }

    /// A destructuring pattern binds every symbol in it, and each is a sibling.
    #[test]
    fn flags_a_read_of_a_destructured_sibling_name() {
        let found = reads("(loop for (k . v) in alist and n = (length v) collect (list k n))");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].sibling, "v");
    }

    // --- the denominator ---------------------------------------------------

    #[test]
    fn the_candidate_count_counts_parallel_groups_not_findings() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for a from 1 to 3 and b from 1 to 3 collect (list a b))",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        // One candidate group, zero findings: a clean result over a real
        // denominator, which is what a corpus sweep must be able to see.
        assert_eq!(candidate_count(&view), 1);
        assert!(examine(&view).is_empty());
    }

    #[test]
    fn a_loop_with_no_and_group_is_not_a_candidate() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for a from 1 to 3 for b = (* a 10) collect (list a b))",
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
