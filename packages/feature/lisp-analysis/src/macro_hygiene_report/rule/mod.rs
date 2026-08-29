//! The lint-rule face of `inspect macro-hygiene`: one rule per risk.
//!
//! [`crate::macro_hygiene_report::domain`] backs both these rules and the
//! standalone report, and detects five risks. The report can only be read;
//! a lint rule can be denied, failed on, suppressed and baselined, so each
//! risk is exposed as its own rule rather than one rule reporting all five —
//! a project that wants `macro-deep-quasiquote-nesting` to fail the build
//! should not have to accept `elisp-macro-missing-declare` with it.
//!
//! All five are report-only. An automatic gensym rewrite was implemented and
//! then reverted: review found it could corrupt a working macro through
//! unquoting inside a nested quasiquote, shadowing the macro's own lambda-list
//! parameter, rewriting quoted literal data, and emitting Common-Lisp-shaped
//! `let`/`,` syntax into the seven other dialects these rules cover.
//!
//! All five are `Severity::Warning`, and deliberately so: as
//! [`crate::macro_hygiene_report::usecase`] puts it, a hygiene risk is a fact
//! about the file rather than a defect by definition. A capturing template is
//! a bug only in a project that has decided it is one, and that project says
//! so through `lint.deny`.
//!
//! Each rule re-runs the whole hygiene analysis over the same node and keeps
//! the one risk it is about. That redundancy is accepted rather than
//! engineered away: sharing one analysis across five rules needs a cache keyed
//! on the node, and it was measured not to pay for itself on real code.
//!
//! Measured on a release build, A/B against `origin/main`, with a same-binary
//! null control establishing a ±3% noise floor:
//!
//! - Emacs 30.2's lisp tree (1655 files, 0.67% of forms a macro definition):
//!   the five rules are **0.39% of total rule time**, and the run is **+0.63%
//!   end to end (median)** — inside the noise floor.
//! - A corpus of nothing but `defmacro` forms: the four Common-Lisp-applicable
//!   rules are **75% of all 183 rules' time**, for **+77% wall clock**. That
//!   is the worst case, and it is what the fivefold redundancy costs.
//! - Sweeping macro density 0/2/5/10/100% gives ≈0/2/5/10/77% overhead — a
//!   clean dose-response, with the crossover into measurable cost at about
//!   **5% macro density**. Real Lisp sits an order of magnitude below that.
//! - `HEADS` is what contains it: at 0% macro density the delta is −2.6%
//!   (noise), so widening the head list and adding four registry entries costs
//!   nothing on the non-macro forms that are almost all of any file.
//!
//! Cache the analysis if a corpus ever lands above ~5% macro density; until
//! then the cache would buy a fraction of a percent and cost a node-keyed
//! lifetime.

pub mod deep_quasiquote;
pub mod missing_declare;
pub mod multiple_evaluation;
pub mod parameter_reordering;
pub mod variable_capture;

use paredit_core_lint_engine::engine::RuleContext;
use paredit_core_lint_engine::model::{HeadFilter, NormalizedHead};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_syntax::definition::is_macro_expander_definition;
use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::view_query::list_head;

use crate::macro_hygiene_report::domain::{
    HYGIENE_MODELLED_DIALECTS, HygieneFinding, hygiene_findings_in,
};

/// Every macro-definition head [`is_macro_expander_definition`] recognises
/// across the modelled dialects. A pre-filter only: [`hygiene_findings`] still
/// confirms the exact dialect/head pairing before doing anything, since e.g.
/// `macro` means something in Fennel and nothing in the other seven, and
/// `defsetf` something in Common Lisp and nothing in Emacs Lisp.
///
/// `HeadFilter::Heads` is a hard dispatch-time filter — a form whose head is
/// missing here never reaches a rule at all — so this list must stay a
/// superset of what the predicate accepts, or the rules silently inspect
/// fewer forms than the report does.
const HEADS: [NormalizedHead; 9] = [
    // Common Lisp.
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
    NormalizedHead::new("define-setf-expander"),
    NormalizedHead::new("defsetf"),
    // Emacs Lisp, beyond the `defmacro` it shares with Common Lisp.
    NormalizedHead::new("cl-defmacro"),
    NormalizedHead::new("cl-define-compiler-macro"),
    NormalizedHead::new("define-inline"),
    // LFE, beyond its `defmacro`.
    NormalizedHead::new("defsyntax"),
    // Fennel, which spells its macro-expander form `macro`. Clojure, Janet,
    // Hy and Carp each use the `defmacro` above.
    NormalizedHead::new("macro"),
];

/// The dispatch filter every rule in this slice shares.
const fn head_filter() -> HeadFilter {
    HeadFilter::Heads(&HEADS)
}

/// The dialects whose macro system the analysis models. Shared by four of the
/// five rules; `elisp-macro-missing-declare` narrows it to Emacs Lisp, whose
/// `declare` convention the other seven do not have.
const fn dialect_scope() -> RuleDialectScope {
    RuleDialectScope::new(&HYGIENE_MODELLED_DIALECTS)
}

/// Every hygiene risk in `view`, or nothing when `view` is not a macro-
/// expander definition of this dialect after all.
fn hygiene_findings(context: &RuleContext<'_>, view: &ExpressionView) -> Vec<HygieneFinding> {
    let dialect = context.dialect();
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !is_macro_expander_definition(dialect, head) {
        return Vec::new();
    }
    let Some(name) = view.children.get(1).and_then(atom_symbol_text) else {
        return Vec::new();
    };
    hygiene_findings_in(dialect, view, name)
}

#[cfg(test)]
mod tests {
    use paredit_core_syntax::dialect::Dialect;

    use super::{HEADS, HYGIENE_MODELLED_DIALECTS, is_macro_expander_definition};

    /// Every `(dialect, head)` pair `is_macro_expander_definition` accepts
    /// across the modelled dialects, listed by hand.
    ///
    /// Hand-listed because the operator tables it consults expose no way to
    /// enumerate themselves — there is no `CommonLispOperator::ALL` to fold
    /// over. **A head added to the predicate must be added here and to
    /// [`HEADS`].**
    const ACCEPTED: &[(Dialect, &str)] = &[
        (Dialect::CommonLisp, "defmacro"),
        (Dialect::CommonLisp, "define-compiler-macro"),
        (Dialect::CommonLisp, "define-setf-expander"),
        (Dialect::CommonLisp, "defsetf"),
        (Dialect::EmacsLisp, "defmacro"),
        (Dialect::EmacsLisp, "cl-defmacro"),
        (Dialect::EmacsLisp, "cl-define-compiler-macro"),
        (Dialect::EmacsLisp, "define-inline"),
        (Dialect::Clojure, "defmacro"),
        (Dialect::Janet, "defmacro"),
        (Dialect::Hy, "defmacro"),
        (Dialect::Carp, "defmacro"),
        (Dialect::Lfe, "defmacro"),
        (Dialect::Lfe, "defsyntax"),
        (Dialect::Fennel, "macro"),
    ];

    /// `HeadFilter::Heads` is a hard dispatch gate, so a head the predicate
    /// accepts but [`HEADS`] omits makes the five rules see fewer forms than
    /// `inspect macro-hygiene` does — silently, with no error and no finding.
    ///
    /// That is exactly the bug FR-005 fixed: `define-compiler-macro`,
    /// `define-setf-expander` and `defsetf` were accepted by the predicate and
    /// missing from the filter, so the capture rule saw one of three macros in
    /// a Common Lisp file. The module documents the superset relation as a
    /// hard invariant; this is what enforces it.
    #[test]
    fn every_head_the_predicate_accepts_is_in_the_dispatch_filter() {
        for &(dialect, head) in ACCEPTED {
            assert!(
                is_macro_expander_definition(dialect, head),
                "{dialect:?}/{head} is listed as accepted but the predicate rejects it; \
                 update ACCEPTED"
            );
            assert!(
                HEADS.iter().any(|known| known.as_str() == head),
                "{dialect:?}/{head} is a macro-expander head the dispatch filter omits, so \
                 the lint rules never see it"
            );
        }
    }

    /// The other direction, and fully derived: a head in [`HEADS`] that no
    /// modelled dialect accepts is either a typo or the residue of a head the
    /// predicate dropped, and either way it widens the dispatch gate for
    /// nothing.
    #[test]
    fn every_head_in_the_dispatch_filter_is_a_macro_expander_somewhere() {
        for head in HEADS {
            assert!(
                HYGIENE_MODELLED_DIALECTS
                    .iter()
                    .any(|&dialect| is_macro_expander_definition(dialect, head.as_str())),
                "{} is in the dispatch filter but no modelled dialect treats it as a \
                 macro-expander definition",
                head.as_str()
            );
        }
    }

    /// Scheme and Racket give hygiene as a language guarantee, so the
    /// predicate refuses their macro-definition forms outright — the dispatch
    /// filter is never even consulted for them.
    #[test]
    fn the_hygienic_dialects_have_no_macro_expander_head_at_all() {
        for dialect in [Dialect::Scheme, Dialect::Racket] {
            for head in HEADS {
                assert!(
                    !is_macro_expander_definition(dialect, head.as_str()),
                    "{dialect:?} accepted {}",
                    head.as_str()
                );
            }
            for head in ["define-syntax", "define-syntax-rule", "syntax-rules"] {
                assert!(
                    !is_macro_expander_definition(dialect, head),
                    "{dialect:?} accepted {head}"
                );
            }
        }
    }
}
