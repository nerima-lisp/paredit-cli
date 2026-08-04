//! `lfe-clause-after-catch-all` detection: a pattern-matching clause that can
//! never run because an earlier clause in the same form matches everything.
//!
//! # This is what the compiler says, not a judgement call
//!
//! Verified against LFE 2.2.0 on Erlang/OTP 27.3.4.15. Compiling
//!
//! ```text
//! (defun a (x)  (case x ('one 1) (_ 'fallback) ('two 2)))
//! (defun b (x)  (case x ('one 1) (other other) ('two 2)))
//! (defun c (('one) 1) ((_) 'fallback) (('two) 2))
//! (defun d (x)  (case x ('one 1) (n (when (is_integer n)) 'int) ('two 2)))
//! ```
//!
//! produced exactly three warnings, and not a fourth:
//!
//! ```text
//! p3.lfe:5:  Warning: this clause cannot match because a previous clause at line 5 always matches
//! p3.lfe:12: Warning: this clause cannot match because a previous clause at line 12 always matches
//! p3.lfe:19: Warning: this clause for c/1 cannot match because a previous clause at line 19 always matches
//! ```
//!
//! `d` is silent because its catch-all carries a guard, so it does not always
//! match. That is the negative control, and it is why [`is_catch_all`] refuses
//! a clause with a `when`.
//!
//! # Two premises that had to be measured rather than reasoned about
//!
//! **A bare variable is a fresh binding, not a comparison.** In Erlang, a
//! pattern variable that is already bound compares against it, so it would not
//! be a catch-all. LFE does *not* inherit that. Compiling
//!
//! ```text
//! (defun a (x k) (case x ('one 1) (k 'equal-to-k) ('two 2)))
//! ```
//!
//! — where `k` comes from the function head — still warned. So a bare variable
//! always matches in LFE regardless of what is bound around it, which is what
//! lets this rule work at all: `binding_table()` is empty for LFE, and if the
//! Erlang reading had been the right one, no sound rule would have been
//! possible.
//!
//! **A repeated variable constrains.** Compiling
//!
//! ```text
//! (defun b ((x x) 'same) ((_ _) 'different))
//! ```
//!
//! produced **no** warning, so `(x x)` does not always match — it requires the
//! two arguments to be equal. [`is_catch_all`] therefore requires the named
//! variables in an argument list to be distinct. Dropping that check would
//! report the second clause here, which the compiler says is reachable.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::{
    atom_text, clause_guard, head_symbol, is_paren_list, is_symb_list, is_variable_atom,
};

/// LFE only. Every other dialect in this workspace spells its clauses
/// differently, and `case`/`receive` mean something else entirely in Common
/// Lisp and Clojure.
pub const DIALECTS: [Dialect; 1] = [Dialect::Lfe];

/// The clause-bearing forms this rule understands, and where their clauses
/// start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseForm {
    /// `(case Expr Clause…)` — clauses from index 2, one pattern each.
    Case,
    /// `(receive Clause… [(after N Body…)])` — clauses from index 1, one
    /// pattern each.
    Receive,
    /// `(match-lambda Clause…)` — clauses from index 1, an argument *list*
    /// each.
    MatchLambda,
    /// `(defun Name Clause…)` in its matching form — clauses from index 2, an
    /// argument list each.
    Defun,
}

impl ClauseForm {
    /// The head spelling that selects this form.
    #[must_use]
    pub const fn head(self) -> &'static str {
        match self {
            Self::Case => "case",
            Self::Receive => "receive",
            Self::MatchLambda => "match-lambda",
            Self::Defun => "defun",
        }
    }

    /// Whether a clause's pattern is a single pattern or a list of them.
    const fn pattern_is_arg_list(self) -> bool {
        matches!(self, Self::MatchLambda | Self::Defun)
    }

    /// Where the clause list starts within the form's children.
    const fn first_clause_index(self) -> usize {
        match self {
            Self::Case | Self::Defun => 2,
            Self::Receive | Self::MatchLambda => 1,
        }
    }
}

/// Every head this rule anchors on.
pub const CLAUSE_FORMS: [ClauseForm; 4] = [
    ClauseForm::Case,
    ClauseForm::Receive,
    ClauseForm::MatchLambda,
    ClauseForm::Defun,
];

/// The form `view` is, if it is one this rule understands.
#[must_use]
pub fn clause_form(view: &ExpressionView) -> Option<ClauseForm> {
    let head = head_symbol(view)?;
    let form = CLAUSE_FORMS.into_iter().find(|form| form.head() == head)?;
    // `(defun name (a b) body)` is the traditional single-clause form and has
    // no clause list at all. LFE decides this with `lfe_lib:is_symb_list`, and
    // `is_symb_list` here replicates that exactly; see its documentation.
    if form == ClauseForm::Defun {
        let args = view.children.get(2)?;
        if is_symb_list(args) {
            return None;
        }
    }
    Some(form)
}

/// The clauses of `view`, skipping everything that is not one.
///
/// Three things share the clause list and are not clauses:
///
/// - `receive`'s trailing `(after N Body…)`, which is a timeout rather than a
///   pattern.
/// - `defun`'s `(spec …)` metadata and its documentation string, which
///   `lfe_macro.erl`'s `exp_meta/2` strips before the clauses are read.
///
/// Anything that is not a paren list is not a clause either, which covers the
/// documentation string without having to recognize a string literal.
#[must_use]
pub fn clauses_of(view: &ExpressionView, form: ClauseForm) -> Vec<&ExpressionView> {
    view.children
        .iter()
        .skip(form.first_clause_index())
        .filter(|clause| is_paren_list(clause))
        .filter(|clause| !matches!(head_symbol(clause), Some("after" | "spec")))
        .collect()
}

/// Whether a clause matches every possible input.
///
/// A clause with a guard never qualifies: the guard can fail, so the clause
/// does not always match. The compiler agrees — the `d` probe above is silent.
#[must_use]
pub fn is_catch_all(clause: &ExpressionView, form: ClauseForm) -> bool {
    if clause_guard(clause, 0).is_some() {
        return false;
    }
    let Some(pattern) = clause.children.first() else {
        return false;
    };
    if !form.pattern_is_arg_list() {
        return is_variable_atom(pattern);
    }
    if !is_paren_list(pattern) {
        return false;
    }
    if !pattern.children.iter().all(is_variable_atom) {
        return false;
    }
    // A repeated variable constrains the arguments to be equal, so the clause
    // does *not* always match — measured, `((x x) 'same)` produces no
    // dead-clause warning for the clause after it. `_` is anonymous and never
    // binds, so repeats of it do not constrain.
    let mut named: Vec<&str> = pattern
        .children
        .iter()
        .filter_map(atom_text)
        .filter(|text| *text != "_")
        .collect();
    let total = named.len();
    named.sort_unstable();
    named.dedup();
    named.len() == total
}

/// One clause that can never run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadClause {
    /// The unreachable clause, which is what gets reported.
    pub span: ByteSpan,
    /// The earlier clause that always matches.
    pub catch_all_span: ByteSpan,
    pub form: ClauseForm,
}

/// Every unreachable clause in one clause-bearing form.
///
/// Reports the *later* clause rather than the catch-all, matching what the
/// compiler says: "this clause cannot match because a previous clause always
/// matches". The catch-all itself is legitimate; it is only in the wrong
/// place.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Vec<DeadClause> {
    if !DIALECTS.contains(&dialect) {
        return Vec::new();
    }
    let Some(form) = clause_form(view) else {
        return Vec::new();
    };
    let clauses = clauses_of(view, form);
    let Some(first_catch_all) = clauses.iter().position(|clause| is_catch_all(clause, form)) else {
        return Vec::new();
    };
    let catch_all_span = clauses[first_catch_all].span;
    clauses
        .into_iter()
        .skip(first_catch_all + 1)
        .map(|clause| DeadClause {
            span: clause.span,
            catch_all_span,
            form,
        })
        .collect()
}

/// How many clauses this rule adjudicated in one form: the denominator.
///
/// Counting only dead clauses would make the denominator equal the numerator,
/// so a clean corpus would report "0 findings over 0 candidates" — the
/// false-clean a denominator exists to rule out. A form has to have at least
/// two clauses for any of them to be dead, so a single-clause `case` is not a
/// candidate for anything.
#[must_use]
pub fn candidate_count_in_form(dialect: Dialect, view: &ExpressionView) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let Some(form) = clause_form(view) else {
        return 0;
    };
    let clauses = clauses_of(view, form);
    if clauses.len() < 2 {
        return 0;
    }
    clauses.len()
}

/// Every unreachable clause in a whole document.
///
/// Only used by this package's own corpus tests and cost measurements; the
/// engine drives the rule one form at a time through the head index.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<DeadClause> {
    let mut found = walk(dialect, tree, examine);
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// The document-wide denominator, for the same reason.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    walk(dialect, tree, |dialect, view| {
        vec![candidate_count_in_form(dialect, view)]
    })
    .into_iter()
    .sum()
}

fn walk<T>(
    dialect: Dialect,
    tree: &SyntaxTree,
    per_form: impl Fn(Dialect, &ExpressionView) -> Vec<T>,
) -> Vec<T> {
    if !DIALECTS.contains(&dialect) {
        return Vec::new();
    }
    let root = tree.root_view();
    let mut out = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        out.extend(per_form(dialect, view));
        stack.extend(view.children.iter());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dead(source: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        collect(Dialect::Lfe, &tree)
            .into_iter()
            .map(|item| item.span.slice(source).to_owned())
            .collect()
    }

    // -- the three shapes lfec warned about --------------------------------

    #[test]
    fn a_clause_after_an_underscore_catch_all_is_dead() {
        assert_eq!(
            dead("(defun a (x) (case x ('one 1) (_ 'fallback) ('two 2)))"),
            vec!["('two 2)"]
        );
    }

    /// LFE shadows rather than compares, so a bare variable always matches.
    #[test]
    fn a_clause_after_a_bare_variable_catch_all_is_dead() {
        assert_eq!(
            dead("(defun b (x) (case x ('one 1) (other other) ('two 2)))"),
            vec!["('two 2)"]
        );
    }

    #[test]
    fn a_defun_clause_after_a_catch_all_is_dead() {
        assert_eq!(
            dead("(defun c (('one) 1) ((_) 'fallback) (('two) 2))"),
            vec!["(('two) 2)"]
        );
    }

    // -- the negative controls lfec was silent about -----------------------

    /// The guarded catch-all. `lfec` produced no warning for this, and neither
    /// may the rule.
    #[test]
    fn a_guarded_clause_is_not_a_catch_all() {
        assert!(
            dead("(defun d (x) (case x ('one 1) (n (when (is_integer n)) 'int) ('two 2)))")
                .is_empty()
        );
    }

    /// `((x x) …)` constrains its two arguments to be equal; `lfec` produced
    /// no warning for the clause after it.
    #[test]
    fn a_repeated_variable_constrains_and_is_not_a_catch_all() {
        assert!(dead("(defun b ((x x) 'same) ((_ _) 'different))").is_empty());
    }

    /// But distinct variables really do match anything, and repeated `_` does
    /// not constrain because `_` never binds.
    #[test]
    fn distinct_variables_and_repeated_underscores_are_catch_alls() {
        assert_eq!(
            dead("(defun f ((a b) 'any) (('x 'y) 'never))"),
            vec!["(('x 'y) 'never)"]
        );
        assert_eq!(
            dead("(defun f ((_ _) 'any) (('x 'y) 'never))"),
            vec!["(('x 'y) 'never)"]
        );
    }

    #[test]
    fn a_catch_all_in_last_position_is_correct_code() {
        assert!(dead("(defun a (x) (case x ('one 1) ('two 2) (_ 'fallback)))").is_empty());
        assert!(dead("(defun c (('one) 1) (('two) 2) ((_) 'fallback))").is_empty());
    }

    // -- the traditional defun form has no clauses at all ------------------

    /// `(defun f (a b) body)` is a plain function. Reading its argument list
    /// as a clause would report the body as a dead clause on every ordinary
    /// function in the corpus.
    #[test]
    fn a_traditional_defun_is_not_a_clause_list() {
        assert!(dead("(defun f (a b) (+ a b))").is_empty());
        assert!(dead("(defun f () 'nothing)").is_empty());
        assert!(dead("(defun f (x) (list x x x))").is_empty());
    }

    /// LFE's own rule is `is_symb_list`, and `'one` is not a symbol — it reads
    /// as `(quote one)`. So an argument list containing a quoted atom makes
    /// this a *matching* defun, and the reader's `Quote` prefix is what tells
    /// the two apart.
    #[test]
    fn a_quoted_atom_in_the_arg_position_makes_it_a_matching_defun() {
        assert!(clause_form(&parse("(defun f ('one) 1)")).is_some());
        assert!(clause_form(&parse("(defun f (a b) 1)")).is_none());
        // A pattern argument list is likewise not a symbol list.
        assert!(clause_form(&parse("(defun f ((tuple a b) c) 1)")).is_some());
    }

    fn parse(source: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        tree.root_view().children.first().expect("form").clone()
    }

    // -- receive and match-lambda ------------------------------------------

    /// `clause_form` reads a head without first testing the delimiter, so the
    /// paren-only test lives inside [`head_symbol`] itself — and this is the
    /// test that reaches it.
    ///
    /// LFE spells `defsyntax` rule patterns with brackets, so a bracket list
    /// whose first element happens to be `case` or `defun` is pattern syntax,
    /// not a form. Reading a head off one would manufacture clause lists out
    /// of macro patterns.
    ///
    /// Mutation-testing found this: relaxing `head_symbol` to accept any list
    /// killed no test until this existed, because every *other* caller guards
    /// with `is_paren_list` first.
    #[test]
    fn a_bracket_list_is_not_read_as_a_clause_form() {
        assert!(dead("(defun f () [case x ('one 1) (_ 'any) ('two 2)])").is_empty());
        assert!(dead("[case x ('one 1) (_ 'any) ('two 2)]").is_empty());
        assert!(dead("[match-lambda ((_) 'any) (('two) 2)]").is_empty());
        let tree =
            SyntaxTree::parse_with_dialect("[case x ('one 1) (_ 'any) ('two 2)]", Dialect::Lfe)
                .expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 0);
    }

    #[test]
    fn a_receive_clause_after_a_catch_all_is_dead() {
        assert_eq!(
            dead("(defun loop () (receive ('ping 'pong) (_ 'any) ('quit 'bye)))"),
            vec!["('quit 'bye)"]
        );
    }

    /// `(after N Body…)` is a timeout, not a pattern clause. Treating `after`
    /// as a bare-variable catch-all would be wrong twice over: it is not a
    /// clause, and it is always last anyway.
    #[test]
    fn a_receive_after_clause_is_not_a_pattern_clause() {
        assert!(dead("(defun loop () (receive ('ping 'pong) (after 1000 'timeout)))").is_empty());
        // And a catch-all before the `after` does not make the timeout dead.
        assert!(dead("(defun loop () (receive (_ 'any) (after 1000 'timeout)))").is_empty());
    }

    #[test]
    fn a_match_lambda_clause_after_a_catch_all_is_dead() {
        assert_eq!(
            dead("(match-lambda (('one) 1) ((_) 'any) (('two) 2))"),
            vec!["(('two) 2)"]
        );
    }

    /// `(spec …)` is metadata `lfe_macro`'s `exp_meta/2` strips, not a clause.
    #[test]
    fn defun_metadata_is_not_a_clause() {
        assert!(dead("(defun f (spec ((integer) integer)) (('one) 1))").is_empty());
    }

    // -- several dead clauses ----------------------------------------------

    #[test]
    fn every_clause_after_the_catch_all_is_reported() {
        assert_eq!(
            dead("(defun a (x) (case x (_ 'any) ('two 2) ('three 3)))"),
            vec!["('two 2)", "('three 3)"]
        );
    }

    #[test]
    fn the_finding_names_the_clause_that_always_matches() {
        let source = "(defun a (x) (case x ('one 1) (_ 'fallback) ('two 2)))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        let found = collect(Dialect::Lfe, &tree);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].catch_all_span.slice(source), "(_ 'fallback)");
        assert_eq!(found[0].form, ClauseForm::Case);
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        for dialect in [Dialect::CommonLisp, Dialect::Clojure, Dialect::Scheme] {
            let tree = SyntaxTree::parse_with_dialect(
                "(defun a (x) (case x ('one 1) (_ 'f) ('two 2)))",
                dialect,
            )
            .expect("parse");
            assert!(collect(dialect, &tree).is_empty());
        }
    }

    /// `examine` carries its own dialect gate, and it has to be tested
    /// *directly*: `collect` gates as well, and the engine gates before
    /// dispatch, so a test going through either would pass with this one
    /// removed. Mutation-testing found exactly that — deleting `examine`'s
    /// gate killed nothing until this test existed.
    ///
    /// The redundancy is deliberate. `examine` is `pub`, so a future caller
    /// reaching it without going through `collect` must not get findings for a
    /// dialect this package does not model.
    #[test]
    fn examine_gates_on_the_dialect_by_itself() {
        let source = "(case x ('one 1) (_ 'f) ('two 2))";
        for dialect in [Dialect::CommonLisp, Dialect::Clojure, Dialect::Scheme] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let form = tree.root_view().children.first().expect("form").clone();
            assert!(
                examine(dialect, &form).is_empty(),
                "{dialect:?} must be rejected by `examine` itself"
            );
        }
        // And the control: LFE, the same source, does produce a finding.
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        let form = tree.root_view().children.first().expect("form").clone();
        assert_eq!(examine(Dialect::Lfe, &form).len(), 1);
    }

    // -- the denominator ---------------------------------------------------

    /// The denominator must be able to exceed the numerator. Correct code has
    /// clauses and no findings.
    #[test]
    fn correct_code_has_candidates_and_no_findings() {
        let source = "(defun a (x) (case x ('one 1) ('two 2) (_ 'fallback)))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 3);
        assert!(collect(Dialect::Lfe, &tree).is_empty());
    }

    /// A single-clause form cannot have a dead clause, so it is not a
    /// candidate — otherwise every ordinary `case` with one branch would
    /// inflate the denominator and make a clean sweep look better than it is.
    #[test]
    fn a_single_clause_form_is_not_a_candidate() {
        let tree = SyntaxTree::parse_with_dialect("(defun a (x) (case x (_ 'only)))", Dialect::Lfe)
            .expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 0);
    }

    #[test]
    fn a_traditional_defun_contributes_no_candidates() {
        let tree =
            SyntaxTree::parse_with_dialect("(defun f (a b) (+ a b))", Dialect::Lfe).expect("parse");
        assert_eq!(candidate_count(Dialect::Lfe, &tree), 0);
    }
}
