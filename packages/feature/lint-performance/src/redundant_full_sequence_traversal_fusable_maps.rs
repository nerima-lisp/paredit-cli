//! `redundant-full-sequence-traversal-fusable-maps`: mapping over the result of
//! a map.
//!
//! `(mapcar #'f (mapcar #'g xs))` walks `xs` once to build a whole intermediate
//! list, then walks that list to build another and throws the first away.
//! `(mapcar (lambda (x) (f (g x))) xs)` is one pass and one allocation.
//!
//! # Why the *nested* spelling and not two siblings
//!
//! The request this rule answers was written as "two sequential `mapcar` calls
//! over the same sequence". That reading was declined, and the reason is worth
//! writing down because it is a cost decision, not a taste one.
//!
//! Two *sibling* calls — `(let ((as (mapcar #'f xs)) (bs (mapcar #'g xs))) …)` —
//! can only be correlated from something that can see both, and a rule anchored
//! on `mapcar` sees neither the other call nor the binding form: [`RuleContext`]
//! carries no parent pointer. Recovering one would mean walking to the enclosing
//! body from *every* `mapcar` in the file, which makes a file of N maps cost
//! N×N. Two rules in this repository have shipped with exactly that shape and
//! together accounted for 98% of a lint run's time.
//!
//! Worse, the sibling reading cannot be made precise from syntax alone: fusing
//! two siblings is only sound when the first's result is not used elsewhere,
//! which is a dataflow question. The nested reading needs neither — the inner
//! list is, by construction, consumed by exactly one caller and is unreachable
//! afterwards — so it is both cheaper and more certain. Preferring the false
//! negative is the policy here.
//!
//! # What is deliberately not reported
//!
//! - A multi-sequence call. `(mapcar #'f (mapcar #'+ as bs))` zips two lists,
//!   and the composition that would replace it is not a composition of one
//!   function.
//! - A mixed pair. `(mapcar #'f (map 'vector #'g xs))` is already wrong —
//!   `mapcar` takes lists — and `(mapcar #'f (map 'list #'g xs))` would need the
//!   result type read to know it is right. Both operators must be the same one.
//! - An inner `(map nil …)`, which returns `nil` rather than a sequence, so the
//!   outer map is not traversing what the inner produced.
//! - `mapcan`, `mapcon`, `maplist`. `mapcan` splices, so the outer map does not
//!   see one element per input element and the composition is a different shape.
//!
//! Report-only. The rewrite has to name the composed function — a `lambda`, an
//! `alexandria:compose`, or a `loop` — and it reorders the two functions' side
//! effects from "every `g`, then every `f`" to "`g` then `f`, per element". That
//! is a decision about the program.
//!
//! Scope: Common Lisp only.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head};

use crate::shared::{symbol_is, unqualified};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-full-sequence-traversal-fusable-maps",
    RuleCategory::Performance,
    Severity::Warning,
    "a map over the result of another map, which walks and allocates a whole intermediate sequence",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Each `mapcar` walks its sequence once and allocates a fresh one. Chaining two of them \
         builds an intermediate sequence that nothing else can reach, walks it, and discards it. \
         Composing the two functions makes one pass and one allocation.",
    )
    .with_example(
        "(mapcar #'first (mapcar #'parse-entry lines))",
        "(mapcar (lambda (line) (first (parse-entry line))) lines)",
    )
    .with_caveat(
        "Fusing reorders side effects from \"every inner call, then every outer call\" to one \
         pair per element, so this is a report rather than a fix. A multi-sequence call, a \
         `mapcan`, and an inner `(map nil …)` are all left alone: none of them produces one \
         element per input element.",
    ),
);

/// The two spellings this rule reads, and where each puts its one sequence.
///
/// A closed pair rather than a table, because the rule requires the outer and
/// the inner to be the *same* operator and comparing two variants says that in
/// one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapKind {
    /// `(mapcar function sequence)`.
    Mapcar,
    /// `(map result-type function sequence)`.
    Map,
}

impl MapKind {
    /// The child index of the one sequence a single-sequence call maps over.
    const fn sequence_index(self) -> usize {
        match self {
            Self::Mapcar => 2,
            Self::Map => 3,
        }
    }

    /// How many children a single-sequence call has, head included. A longer
    /// call maps over several sequences at once and is not this rule's shape.
    const fn single_sequence_arity(self) -> usize {
        self.sequence_index() + 1
    }
}

/// Reads one call as a single-sequence map, or `None`.
fn read_map(view: &ExpressionView) -> Option<MapKind> {
    let head = list_head(view)?;
    let kind = if symbol_is(head, "mapcar") {
        MapKind::Mapcar
    } else if symbol_is(head, "map") {
        MapKind::Map
    } else {
        return None;
    };
    (view.children.len() == kind.single_sequence_arity()).then_some(kind)
}

/// Whether a `map` call's result type is the literal `nil`, in which case it
/// returns `nil` rather than a sequence and the caller is not traversing what it
/// produced.
///
/// Reads `nil` and `'nil` alike; anything computed is not a literal `nil` and is
/// left as a sequence-producing call.
fn produces_no_sequence(view: &ExpressionView, kind: MapKind) -> bool {
    if kind != MapKind::Map {
        return false;
    }
    view.children
        .get(1)
        .and_then(atom_text)
        .is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// One fusable pair: an outer map whose sequence is another map's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusableMaps {
    /// The whole outer call, which is what is reported.
    pub span: ByteSpan,
    /// The inner call, whose result is the intermediate sequence.
    pub inner_span: ByteSpan,
    /// The operator both calls spell, for the message.
    pub operator: String,
}

/// Reads one map call and reports the map it consumes, if any.
///
/// Everything read is inside the matched node: the outer call's own children and
/// one of them. No ancestor, no sibling, no whole-file scan.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<FusableMaps> {
    let outer = read_map(view)?;
    let sequence = view.children.get(outer.sequence_index())?;
    // `'(mapcar …)` in the sequence position is a literal list, not a call.
    if !sequence.reader_prefixes.is_empty() {
        return None;
    }
    let inner = read_map(sequence)?;
    if inner != outer || produces_no_sequence(sequence, inner) {
        return None;
    }
    Some(FusableMaps {
        span: view.span,
        inner_span: sequence.span,
        operator: unqualified(list_head(view)?).to_ascii_lowercase(),
    })
}

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("mapcar"), NormalizedHead::new("map")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        // Only now, with a candidate in hand, is it worth asking whether the
        // form is code at all: the engine's dispatch walks into quoted data, and
        // `'(mapcar #'f (mapcar #'g xs))` is a list of symbols.
        if is_unevaluated_at(context.tree(), found.span) {
            return Ok(());
        }
        sink.report(
            found.span,
            format!(
                "this {0} walks the whole sequence the inner {0} just built, which exists only \
                 to be walked; composing the two functions makes one pass",
                found.operator
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::{messages, reported};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn form(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn operator(input: &str) -> Option<String> {
        examine(&form(input)).map(|found| found.operator)
    }

    /// The engine path, which is what the CLI runs.
    fn findings(source: &str) -> Vec<String> {
        reported(&META, &RULE, source)
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_mapcar_over_a_mapcar() {
        assert_eq!(
            operator("(mapcar #'f (mapcar #'g xs))"),
            Some("mapcar".to_owned())
        );
        assert_eq!(findings("(mapcar #'f (mapcar #'g xs))").len(), 1);
    }

    #[test]
    fn flags_a_map_over_a_map() {
        assert_eq!(
            operator("(map 'list #'f (map 'list #'g xs))"),
            Some("map".to_owned())
        );
    }

    #[test]
    fn flags_a_lambda_spelled_pair_the_same_way() {
        assert_eq!(
            operator("(mapcar (lambda (y) (h y)) (mapcar #'g xs))"),
            Some("mapcar".to_owned())
        );
    }

    #[test]
    fn the_package_qualified_spelling_is_read_the_same() {
        assert_eq!(
            operator("(cl:mapcar #'f (cl:mapcar #'g xs))"),
            Some("mapcar".to_owned())
        );
    }

    #[test]
    fn a_three_deep_chain_reports_each_consuming_call() {
        // Two outer calls each consume a map, so two findings, not one.
        assert_eq!(
            findings("(mapcar #'f (mapcar #'g (mapcar #'h xs)))").len(),
            2
        );
    }

    #[test]
    fn the_message_names_the_operator() {
        let said = messages(&META, &RULE, "(mapcar #'f (mapcar #'g xs))");
        assert_eq!(said.len(), 1);
        assert!(said[0].contains("mapcar"), "{said:?}");
    }

    #[test]
    fn the_reported_span_is_the_outer_call() {
        assert_eq!(
            findings("(progn (mapcar #'f (mapcar #'g xs)))"),
            vec!["(mapcar #'f (mapcar #'g xs))"]
        );
    }

    // -- near-miss negatives --------------------------------------------------

    #[test]
    fn does_not_flag_a_single_map() {
        assert_eq!(operator("(mapcar #'f xs)"), None);
    }

    #[test]
    fn does_not_flag_a_map_over_something_that_is_not_a_map() {
        assert_eq!(operator("(mapcar #'f (remove-if #'p xs))"), None);
        assert_eq!(operator("(mapcar #'f (sort xs #'<))"), None);
    }

    /// The guard that keeps a zip out: `(mapcar #'+ as bs)` produces one element
    /// per *pair*, and no single composed function replaces it.
    #[test]
    fn does_not_flag_a_multi_sequence_inner_call() {
        assert_eq!(operator("(mapcar #'f (mapcar #'+ as bs))"), None);
    }

    #[test]
    fn does_not_flag_a_multi_sequence_outer_call() {
        assert_eq!(operator("(mapcar #'+ (mapcar #'g xs) ys)"), None);
    }

    /// `mapcan` splices its results, so the outer call does not see one element
    /// per input element.
    #[test]
    fn does_not_flag_a_splicing_inner_call() {
        assert_eq!(operator("(mapcar #'f (mapcan #'g xs))"), None);
        assert_eq!(operator("(mapcar #'f (maplist #'g xs))"), None);
    }

    /// The two operators must be the same one: `mapcar` over a `map` needs the
    /// inner result type read before anything can be claimed.
    #[test]
    fn does_not_flag_a_mixed_pair() {
        assert_eq!(operator("(mapcar #'f (map 'list #'g xs))"), None);
        assert_eq!(operator("(map 'list #'f (mapcar #'g xs))"), None);
    }

    /// `(map nil …)` returns `nil`, not a sequence.
    #[test]
    fn does_not_flag_an_inner_map_that_produces_no_sequence() {
        assert_eq!(operator("(map nil #'f (map nil #'g xs))"), None);
        assert_eq!(operator("(map 'list #'f (map nil #'g xs))"), None);
        // An *outer* `map nil` over a real inner sequence still fuses.
        assert_eq!(
            operator("(map nil #'f (map 'list #'g xs))"),
            Some("map".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_quoted_form_in_the_sequence_position() {
        assert_eq!(operator("(mapcar #'f '(mapcar #'g xs))"), None);
    }

    // -- quote-context negative -----------------------------------------------

    #[test]
    fn reports_nothing_inside_quoted_data() {
        assert!(findings("'(mapcar #'f (mapcar #'g xs))").is_empty());
        assert!(findings("(quote (mapcar #'f (mapcar #'g xs)))").is_empty());
        assert!(findings("`(mapcar #'f (mapcar #'g xs))").is_empty());
        assert!(findings("(defparameter *forms* '(mapcar #'f (mapcar #'g xs)))").is_empty());
    }

    #[test]
    fn reports_a_form_escaped_back_into_code_by_an_unquote() {
        assert_eq!(findings("`(a ,(mapcar #'f (mapcar #'g xs)))").len(), 1);
    }

    // -- string-literal negative ----------------------------------------------

    #[test]
    fn reports_nothing_spelled_only_inside_a_string() {
        assert!(findings("(format t \"(mapcar #'f (mapcar #'g xs))\")").is_empty());
        assert!(findings("(defun doc () \"(mapcar #'f (mapcar #'g xs))\")").is_empty());
    }
}
