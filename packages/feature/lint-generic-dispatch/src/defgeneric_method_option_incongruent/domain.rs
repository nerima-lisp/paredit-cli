//! `defgeneric-method-option-incongruent`: a `(:method …)` option whose lambda
//! list the `defgeneric` right above it will not accept.
//!
//! # What CLHS 7.6.4 requires, and what SBCL 2.6.0 actually does
//!
//! Every row below was run. The expression is given so it can be re-run, and
//! the verdict is SBCL 2.6.0's, quoted from the transcript. The first eight use
//! a separate `defmethod` because that is the clearest way to isolate one
//! difference at a time; the last row is this rule's own subject and shows that
//! a `defgeneric`'s own `(:method …)` is held to exactly the same rule.
//!
//! | written | SBCL 2.6.0 |
//! |---|---|
//! | `(defgeneric g1 (a))` + `(defmethod g1 ((a t) (b t)) …)` | `SIMPLE-PROGRAM-ERROR` — "the method has more required arguments than the generic function" |
//! | `(defgeneric k3 (a b))` + `(defmethod k3 ((a t)) a)` | `SIMPLE-PROGRAM-ERROR` — "fewer required arguments" |
//! | `(defgeneric g2 (a &optional b))` + `(defmethod g2 ((a t)) a)` | `SIMPLE-PROGRAM-ERROR` — "fewer optional arguments" |
//! | `(defgeneric k2 (a))` + `(defmethod k2 ((a t) &optional o) …)` | `SIMPLE-PROGRAM-ERROR` — "more optional arguments" |
//! | `(defgeneric k1 (a))` + `(defmethod k1 ((a t) &key opt) …)` | `SIMPLE-PROGRAM-ERROR` — "differ in whether they accept &REST or &KEY arguments" |
//! | `(defgeneric g4 (a &key b))` + `(defmethod g4 ((a t) &key) a)` | `SIMPLE-PROGRAM-ERROR` — "does not accept each of the &KEY arguments (:B)" |
//! | `(defgeneric g3 (a &key))` + `(defmethod g3 ((a t) &key extra) …)` | **accepted** — a method may add keywords |
//! | `(defgeneric g5 (a &rest r))` + `(defmethod g5 ((a t) &key k) …)` | **accepted** — `&rest` and `&key` are one question |
//! | `(defgeneric k4 (a &key b))` + `(defmethod k4 ((a t) &rest r) …)` | **accepted** — a bare `&rest` accepts every keyword |
//! | `(defgeneric k5 (a &key b))` + `(defmethod k5 ((a t) &key &allow-other-keys) a)` | **accepted** |
//! | `(defgeneric k6 (a) (:method ((a t) (b t)) …))` | `SIMPLE-PROGRAM-ERROR` — "the method has more required arguments than the generic function" |
//!
//! So [`incongruence_between`] reports exactly four things, in that order, and
//! nothing else.
//!
//! # Why this reads only the `defgeneric`'s own options
//!
//! The obvious larger rule is to correlate a `defgeneric` with every
//! `defmethod` in the file that names it. That was written, tested and
//! **measured**, and then dropped. It is not a soundness problem — the
//! file-scope objection does not apply, because such a rule reports on a
//! *co-occurrence* rather than on an absence, and a `defgeneric` in another
//! file yields a missed finding rather than a false one.
//!
//! It is a cost problem. The correlation is O(generics x top-level forms) and
//! nothing available bounds it:
//!
//! - `RuleContext::scratch_cache` is a **single** slot already owned by
//!   `paredit-feature-lint-repl-debug`, and a second type in it panics, so the
//!   pairing cannot be built once per file and shared;
//! - `RuleContext::binding_table` is per-file and cached, but it models lexical
//!   scopes rather than top-level definition names, and building it costs a
//!   whole semantic pass;
//! - reading each top-level form's head and name straight out of the source
//!   text via `SyntaxTree::root_child_span`, with no allocation at all,
//!   measured **slower** than the shipped `select_path(&Path::root_child(i))`
//!   scan it was written to beat — 4.37 s against 2.60 s at 2000 protocols,
//!   with 8x doubling ratios of 97 and 115 where linear is 8.
//!
//! Two rules in `paredit-feature-lint-object-system` already have that shape and
//! are on record as dominating lint time at ~480 definitions. A third was not
//! worth adding for a check whose omission costs missed findings and nothing
//! else, so the cross-form half is not shipped. The numbers are in
//! `cost_tests`, where `cost-control-select-path-scan` keeps the comparison
//! runnable.
//!
//! What remains is local to the matched form, costs one pass over its own
//! children, and still catches a file that will not load.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

use crate::support::{
    LambdaList, has_reader_conditional, is_method_option, lambda_list_of, method_option_parts,
};

/// One `(:method …)` option whose lambda list the generic function will not
/// accept.
#[derive(Debug, Clone)]
pub struct Incongruence {
    /// The span of the offending `(:method …)` option.
    pub span: ByteSpan,
    /// The generic function's name, as the `defgeneric` writes it.
    pub generic: String,
    /// What CLOS will object to, phrased the way SBCL phrases it.
    pub reason: String,
}

impl Incongruence {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "this (:method ...) option's lambda list is not congruent with the lambda list of \
             (defgeneric {}) above it: {}, so CLOS signals an error when the method is added \
             rather than adding it",
            self.generic, self.reason
        )
    }
}

/// The first way `method` fails CLHS 7.6.4 against `generic`, if it fails.
///
/// The order is SBCL's own: it reports the required-argument mismatch before
/// looking at anything else, so a message that names one difference names the
/// same one SBCL would.
#[must_use]
pub fn incongruence_between(generic: &LambdaList, method: &LambdaList) -> Option<String> {
    if generic.required != method.required {
        return Some(format!(
            "the generic function takes {} required argument{} and the method takes {}",
            generic.required,
            if generic.required == 1 { "" } else { "s" },
            method.required
        ));
    }
    if generic.optional != method.optional {
        return Some(format!(
            "the generic function takes {} &optional argument{} and the method takes {}",
            generic.optional,
            if generic.optional == 1 { "" } else { "s" },
            method.optional
        ));
    }
    if generic.accepts_trailing() != method.accepts_trailing() {
        let (with, without) = if generic.accepts_trailing() {
            ("the generic function", "the method")
        } else {
            ("the method", "the generic function")
        };
        return Some(format!(
            "{with} accepts &rest or &key arguments and {without} does not"
        ));
    }
    // A method that accepts every keyword cannot be missing one. Verified:
    // SBCL accepts both `&rest` alone and `&key &allow-other-keys` against a
    // generic that names `:b`.
    if method.accepts_any_keyword() {
        return None;
    }
    let missing: Vec<&str> = generic
        .keywords
        .iter()
        .filter(|keyword| !method.keywords.contains(keyword))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(format!(
        "the method does not accept the keyword argument{} {} that the generic function names",
        if missing.len() == 1 { "" } else { "s" },
        missing.join(", ")
    ))
}

/// Examines one matched `defgeneric`.
///
/// # Cost
///
/// One pass over the form's own children and nothing else. No other node in the
/// file is read, nothing is allocated per candidate beyond the findings
/// themselves, and `root_view()` is never reached — the rule calls
/// `is_unevaluated_at` once, and only after a finding is already in hand.
#[must_use]
pub fn examine_defgeneric(view: &ExpressionView, source: &str) -> Vec<Incongruence> {
    let mut found = Vec::new();
    if !list_head(view).is_some_and(|head| symbol_is(head, "defgeneric")) {
        return found;
    }
    // A folded `#+`/`#-` atom shifts every later index, so neither the lambda
    // list at index 2 nor the options after it are where they are counted, and
    // a `(:method …)` option may be hidden inside the atom entirely.
    if has_reader_conditional(view) {
        return found;
    }
    // (defgeneric name lambda-list option*)
    let Some(lambda_list) = view.children.get(2).filter(|child| is_paren_list(child)) else {
        return found;
    };
    let Some(generic) = lambda_list_of(lambda_list) else {
        return found;
    };
    let name = written_name(view, source).unwrap_or_else(|| "the generic function".to_owned());

    for option in view.children.iter().skip(3).filter(|c| is_method_option(c)) {
        let Some(parts) = method_option_parts(option) else {
            continue;
        };
        let Some(method) = lambda_list_of(parts.lambda_list) else {
            continue;
        };
        if let Some(reason) = incongruence_between(&generic, &method) {
            found.push(Incongruence {
                span: option.span,
                generic: name.clone(),
                reason,
            });
        }
    }
    found
}

/// The name a `defgeneric` defines, as written.
///
/// A `(setf width)` name keeps its parentheses; it is only ever put into a
/// message, never compared, so it is reproduced rather than normalized.
fn written_name(view: &ExpressionView, source: &str) -> Option<String> {
    let name = view.children.get(1)?;
    let text = source.get(name.span.start().get()..name.span.end().get())?;
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(source: &str) -> Vec<Incongruence> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        tree.root_view()
            .children
            .iter()
            .flat_map(|child| examine_defgeneric(child, source))
            .collect()
    }

    fn reasons(source: &str) -> Vec<String> {
        findings(source)
            .into_iter()
            .map(|item| item.reason)
            .collect()
    }

    // -- the four things it reports, one SBCL transcript row each -------------

    #[test]
    fn flags_a_method_option_with_more_required_arguments() {
        let found = findings("(defgeneric k6 (a)\n  (:method ((a t) (b t)) (list a b)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "k6");
        assert!(found[0].reason.contains("1 required argument"), "{found:?}");
        assert!(found[0].message().contains("signals an error"));
    }

    #[test]
    fn flags_a_method_option_with_fewer_required_arguments() {
        assert_eq!(
            reasons("(defgeneric k3 (a b) (:method ((a t)) a))").len(),
            1
        );
    }

    #[test]
    fn flags_a_method_option_whose_optional_count_differs() {
        assert_eq!(
            reasons("(defgeneric g2 (a &optional b) (:method ((a t)) a))").len(),
            1
        );
        assert_eq!(
            reasons("(defgeneric k2 (a) (:method ((a t) &optional o) o))").len(),
            1
        );
    }

    #[test]
    fn flags_a_method_option_that_accepts_keywords_where_the_generic_does_not() {
        let found = reasons("(defgeneric k1 (a) (:method ((a t) &key opt) opt))");
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("&rest or &key"), "{found:?}");
    }

    #[test]
    fn flags_a_method_option_that_omits_a_keyword_the_generic_names() {
        let found = reasons("(defgeneric g4 (a &key b) (:method ((a t) &key) a))");
        assert_eq!(found.len(), 1);
        assert!(found[0].contains(":b"), "{found:?}");
    }

    // -- the near misses SBCL accepts -----------------------------------------

    #[test]
    fn accepts_a_method_option_that_adds_a_keyword() {
        assert!(reasons("(defgeneric g3 (a &key) (:method ((a t) &key extra) extra))").is_empty());
    }

    #[test]
    fn accepts_a_key_option_on_a_rest_generic_and_the_reverse() {
        assert!(reasons("(defgeneric g5 (a &rest r) (:method ((a t) &key k) k))").is_empty());
        assert!(reasons("(defgeneric g6 (a &key k) (:method ((a t) &rest r) r))").is_empty());
    }

    #[test]
    fn accepts_a_method_option_that_declares_allow_other_keys() {
        assert!(
            reasons("(defgeneric k5 (a &key b) (:method ((a t) &key &allow-other-keys) a))")
                .is_empty()
        );
    }

    #[test]
    fn accepts_an_ordinary_congruent_default_method() {
        assert!(
            reasons(
                "(defgeneric draw (shape stream)\n\
                 \x20 (:documentation \"Draws SHAPE.\")\n\
                 \x20 (:method ((s shape) stream) (format stream \"~a\" s))\n\
                 \x20 (:method :around ((s shape) stream) (call-next-method)))"
            )
            .is_empty()
        );
    }

    /// A qualifier displaces the lambda list by one child. A rule that read a
    /// fixed index would compare the *qualifier* to the generic's lambda list.
    #[test]
    fn a_qualified_method_option_is_read_at_the_right_index() {
        assert!(reasons("(defgeneric d (a b) (:method :before ((a t) (b t)) a))").is_empty());
        assert_eq!(
            reasons("(defgeneric d (a b) (:method :before ((a t)) a))").len(),
            1,
            "and a genuinely incongruent qualified option is still caught"
        );
    }

    #[test]
    fn a_setf_generic_names_itself_in_the_message() {
        let found = findings("(defgeneric (setf width) (v s) (:method ((v t)) v))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "(setf width)");
    }

    #[test]
    fn flags_every_incongruent_option_of_one_generic() {
        assert_eq!(
            reasons(
                "(defgeneric g (a)\n\
                 \x20 (:method ((a t) (b t)) a)\n\
                 \x20 (:method ((a t)) a)\n\
                 \x20 (:method ((a t) &optional c) a))"
            )
            .len(),
            2
        );
    }

    // -- what it declines to look at ------------------------------------------

    /// A separate `defmethod` is not this rule's subject. Correlating one with
    /// its `defgeneric` is O(generics x forms) and was dropped on measurement;
    /// see the module documentation.
    #[test]
    fn a_separate_defmethod_is_not_correlated() {
        assert!(reasons("(defgeneric draw (a))\n(defmethod draw ((a t) (b t)) a)").is_empty());
    }

    #[test]
    fn a_documentation_option_is_never_read_as_a_method() {
        assert!(
            reasons("(defgeneric g (a)\n  (:documentation \"Draws A.\")\n  (:method ((a t)) a))")
                .is_empty()
        );
        // ...and an option list whose head is not `:method` cannot be read as
        // one either, however method-shaped it looks.
        assert!(reasons("(defgeneric g (a) (:report ((a t) (b t)) a))").is_empty());
    }

    /// A reader conditional **inside a `(:method …)` option's body** hides
    /// nothing this rule reads: the lambda list is still a list and still
    /// incongruent under either reading of the conditional. Declining it would
    /// be a lost finding, which is why `method_option_parts` carries no
    /// reader-conditional guard where `method_parts` does.
    #[test]
    fn a_reader_conditional_in_a_method_options_body_hides_nothing() {
        assert_eq!(
            reasons("(defgeneric g (a) (:method ((a t) (b t)) #+sbcl (foo)))").len(),
            1
        );
    }

    /// ...but one where the *lambda list* should be leaves no list there at
    /// all, so the shape check declines it without a guard.
    #[test]
    fn a_folded_lambda_list_is_declined_by_shape() {
        assert!(reasons("(defgeneric g (a) (:method #+sbcl ((a t) (b t)) a))").is_empty());
    }

    #[test]
    fn a_reader_conditional_makes_the_form_opaque() {
        assert!(
            reasons("(defgeneric g (a) #+sbcl (:documentation \"d\") (:method ((a t) (b t)) a))")
                .is_empty(),
            "the option indices are shifted and an option may be hidden in the folded atom"
        );
        assert!(
            reasons("(defgeneric g #+sbcl (a) (:method ((a t) (b t)) a))").is_empty(),
            "the lambda list is not at index 2"
        );
    }

    #[test]
    fn a_defgeneric_with_no_lambda_list_is_declined() {
        assert!(reasons("(defgeneric g)").is_empty());
        assert!(reasons("(defgeneric g nil (:method ((a t)) a))").is_empty());
    }

    #[test]
    fn a_method_option_with_no_lambda_list_is_declined() {
        assert!(reasons("(defgeneric g (a) (:method))").is_empty());
        assert!(reasons("(defgeneric g (a) (:method :around))").is_empty());
    }
}
