//! A `contains?` whose collection can never contain the key it is asked
//! about.
//!
//! ```clojure
//! (contains? (keys m) :id)   ; throws: contains? not supported on ISeq
//! (contains? [:a :b] :a)     ; false, always: a vector's keys are its indexes
//! ```
//!
//! # The premise, read off `clojure/clojure`
//!
//! `contains?` is `(. clojure.lang.RT (contains coll key))` (`core.clj`:1502-
//! 1510), and `RT.contains` (`RT.java`:824-848) is a closed dispatch:
//!
//! ```java
//! if(coll == null)                         return F;
//! else if(coll instanceof Associative)     return ((Associative) coll).containsKey(key) ? T : F;
//! else if(coll instanceof IPersistentSet)  …
//! else if(coll instanceof Map)             …
//! else if(coll instanceof Set)             …
//! else if(key instanceof Number && (coll instanceof String || coll.getClass().isArray())) …
//! else if(coll instanceof ITransientSet)   …
//! else if(coll instanceof ITransientAssociative2) …
//! throw new IllegalArgumentException("contains? not supported on type: " + coll.getClass().getName());
//! ```
//!
//! Two shapes fall out of that, and this rule reports exactly those two:
//!
//! **A sequence.** `PersistentList`, `LazySeq`, `Cons`, `Range` and
//! `LongRange` all reach `ASeq`/`Obj` implementing `ISeq, Sequential, List` —
//! `java.util.List`, which is neither `Map` nor `Set`. So none of the branches
//! matches and the call **throws**. If the producer returned `nil` instead —
//! `(keys {})`, `(seq [])` — the first branch answers `false`. Either way
//! `contains?` over a sequence **can never answer true**, which is the uniform
//! claim this rule makes. `contains?`'s own docstring says why one would reach
//! for it anyway: "it will not perform a linear search for a value. See also
//! 'some'."
//!
//! **A vector with a non-integer key.** `APersistentVector.containsKey`
//! (`APersistentVector.java`:387-392) is
//!
//! ```java
//! if(!(Util.isInteger(key))) return false;
//! int i = ((Number) key).intValue();
//! return i >= 0 && i < count();
//! ```
//!
//! so `(contains? [:a :b] :a)` is `false` — silently, with no exception to
//! notice. This is the "contains? tests keys, not values" trap, and on a
//! vector the keys are the indexes.
//!
//! # What it looks at
//!
//! Only what it can prove from the call itself:
//!
//! - the collection argument is a **literal `'(…)` list** or a call to one of
//!   [`crate::support::SEQ_PRODUCER_HEADS`]; or
//! - the collection argument is a **literal `[…]` vector** *and* the key
//!   argument is a **keyword or string literal**, neither of which
//!   `Util.isInteger` can accept.
//!
//! A `contains?` over a symbol — which is nearly all of them — is not a claim
//! this rule can make, and is not counted against it: the denominator is every
//! `contains?` call scanned.
//!
//! # What it does not attempt
//!
//! - **`(get seq k)`**, which answers `nil` rather than throwing. Same trap,
//!   different repair, and a different rule's subject.
//! - **`shuffle` and `split-at`**, which return vectors and are therefore
//!   absent from the producer list.
//! - **A key that is a symbol.** `(contains? [:a] k)` may be an index at run
//!   time; only a literal keyword or string is provably not one.
//! - **A user function returning a seq.** Invisible.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};
use serde_json::{Value, json};

use crate::support::{
    SEQ_PRODUCER_HEADS, for_each_evaluated_subview, head_is, is_quoted_list_literal,
    is_vector_literal, normalized_symbol,
};

/// The one head [`examine_contains`] matches.
pub const CONTAINS_HEADS: &[&str] = &["contains?"];

/// Why a particular `contains?` can never answer true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainsDefect {
    /// The collection is an `ISeq`, so `RT.contains` reaches its final
    /// `throw` — or, for a producer that answered `nil`, its first `return F`.
    Sequence,
    /// The collection is a vector and the key is provably not an index, so
    /// `APersistentVector.containsKey` returns `false` without looking.
    VectorNonIndexKey,
}

impl ContainsDefect {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::VectorNonIndexKey => "vector-non-index-key",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContainsOnNonAssociativeItem {
    /// The span of the whole `contains?` call.
    pub span: ByteSpan,
    /// Which of the two shapes this is.
    pub defect: ContainsDefect,
    /// How the collection argument was spelled — a producer head, or the
    /// literal's shape.
    pub collection: String,
}

impl Finding for ContainsOnNonAssociativeItem {
    fn kind(&self) -> &'static str {
        "contains-on-non-associative"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("defect={}", self.defect.as_str()),
            format!("collection={}", self.collection),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("defect", json!(self.defect.as_str())),
            ("collection", json!(self.collection)),
        ]
    }

    fn message(&self) -> String {
        match self.defect {
            ContainsDefect::Sequence => format!(
                "contains? over a sequence ({}) can never answer true: RT.contains throws \
                 \"contains? not supported on type\" for an ISeq and answers false for nil — \
                 use (some #{{…}} coll) or put the values in a set",
                self.collection
            ),
            ContainsDefect::VectorNonIndexKey => format!(
                "contains? on a vector tests indexes, not values, so this is always false; \
                 {} is not an integer — use (some #{{…}} v) or a set",
                self.collection
            ),
        }
    }
}

/// Whether an atom is a **keyword or string literal**, neither of which
/// `clojure.lang.Util.isInteger` can accept.
///
/// A symbol is deliberately not enough: `(contains? [:a] k)` may well be an
/// index at run time, and this rule reports only what the call itself proves.
/// An earlier revision opened with `if !view.children.is_empty() { return
/// None; }`. Mutation testing showed it killed nothing, and it could not:
/// [`atom_text`] answers `Some` only for an `ExpressionKind::Atom`, and every
/// node with children is a list. It was dead and is gone.
fn is_non_index_literal(view: &ExpressionView) -> Option<String> {
    let text = atom_text(view)?;
    let first = text.as_bytes().first()?;
    matches!(first, b':' | b'"').then(|| text.to_owned())
}

/// Whether a collection argument is provably an `ISeq` (or `nil`).
fn sequence_spelling(view: &ExpressionView) -> Option<String> {
    if is_quoted_list_literal(view) {
        return Some("'(…)".to_owned());
    }
    head_is(view, SEQ_PRODUCER_HEADS)
        .then(|| list_head(view).map_or_else(|| "seq".to_owned(), |head| format!("{head} …")))
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_contains(
    view: &ExpressionView,
    contains_count: &mut usize,
    violations: &mut Vec<ContainsOnNonAssociativeItem>,
) {
    if !head_is(view, CONTAINS_HEADS) {
        return;
    }
    *contains_count += 1;

    // `(contains? coll key)` is the only arity; anything else is malformed and
    // is a different rule's subject.
    if view.children.len() != 3 {
        return;
    }
    let (Some(collection), Some(key)) = (view.children.get(1), view.children.get(2)) else {
        return;
    };

    if let Some(spelling) = sequence_spelling(collection) {
        violations.push(ContainsOnNonAssociativeItem {
            span: view.span,
            defect: ContainsDefect::Sequence,
            collection: spelling,
        });
        return;
    }

    if is_vector_literal(collection) {
        if let Some(literal) = is_non_index_literal(key) {
            violations.push(ContainsOnNonAssociativeItem {
                span: view.span,
                defect: ContainsDefect::VectorNonIndexKey,
                collection: normalized_symbol(&literal),
            });
        }
    }
}

/// Collects every impossible `contains?` in one file, with the number of
/// `contains?` calls scanned as the denominator beside them.
pub fn build_contains_on_non_associative_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ContainsOnNonAssociativeItem>> {
    let mut contains_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_contains(view, &mut contains_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("contains_count", json!(contains_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ContainsOnNonAssociativeItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_contains_on_non_associative_report(Path::new("test.clj"), Dialect::Clojure, &tree)
            .expect("build report")
    }

    fn defects(input: &str) -> Vec<ContainsDefect> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.defect)
            .collect()
    }

    // --- the sequence shape --------------------------------------------------

    #[test]
    fn flags_contains_over_a_sequence_producing_call() {
        for source in [
            "(contains? (keys m) :id)",
            "(contains? (vals m) 1)",
            "(contains? (map :id xs) 3)",
            "(contains? (filter p xs) x)",
            "(contains? (range 10) 3)",
            "(contains? (list 1 2 3) 1)",
            "(contains? (concat a b) x)",
            "(contains? (seq coll) x)",
            "(contains? (rest xs) x)",
            "(contains? (sort xs) x)",
            "(contains? (reverse xs) x)",
            "(contains? (line-seq r) \"x\")",
        ] {
            assert_eq!(defects(source), vec![ContainsDefect::Sequence], "{source}");
        }
    }

    #[test]
    fn flags_contains_over_a_quoted_list_literal() {
        assert_eq!(
            defects("(contains? '(1 2 3) 1)"),
            vec![ContainsDefect::Sequence]
        );
        assert_eq!(
            report("(contains? '(:a) :a)").findings[0].collection,
            "'(…)"
        );
    }

    // --- the vector shape ----------------------------------------------------

    #[test]
    fn flags_contains_on_a_literal_vector_with_a_keyword_or_string_key() {
        assert_eq!(
            defects("(contains? [:a :b] :a)"),
            vec![ContainsDefect::VectorNonIndexKey]
        );
        assert_eq!(
            defects("(contains? [\"a\" \"b\"] \"a\")"),
            vec![ContainsDefect::VectorNonIndexKey]
        );
    }

    /// A vector's keys *are* its indexes, so an integer key is the correct
    /// use of `contains?` and must stay silent.
    #[test]
    fn does_not_flag_an_index_key_on_a_vector() {
        assert!(defects("(contains? [:a :b] 0)").is_empty());
        assert!(defects("(contains? [:a :b] 1)").is_empty());
    }

    /// Only a literal keyword or string is provably not an index. A symbol may
    /// hold one at run time.
    #[test]
    fn does_not_flag_a_computed_key_on_a_vector() {
        assert!(defects("(contains? [:a :b] k)").is_empty());
        assert!(defects("(contains? [:a :b] (inc i))").is_empty());
    }

    // --- realistic, correct Clojure that must stay silent --------------------

    #[test]
    fn does_not_flag_the_associative_and_set_collections() {
        for source in [
            "(contains? {:a 1} :a)",
            "(contains? #{:a :b} :a)",
            "(contains? m :id)",
            "(contains? (:opts m) :retries)",
            "(contains? (set xs) x)",
            "(contains? (into #{} xs) x)",
            "(contains? (zipmap ks vs) k)",
            "(contains? (select-keys m [:a]) :a)",
            "(contains? (frequencies xs) x)",
            "(contains? (group-by :k xs) 1)",
            "(contains? (merge a b) :k)",
            "(contains? (vec xs) 0)",
            "(contains? (mapv f xs) 0)",
            // `shuffle` returns a vector, not a seq.
            "(contains? (shuffle xs) 0)",
        ] {
            assert!(defects(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_malformed_call() {
        assert!(defects("(contains? (keys m))").is_empty());
        assert!(defects("(contains?)").is_empty());
        assert!(defects("(contains? (keys m) :a :extra)").is_empty());
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(defects("'(contains? (keys m) :id)").is_empty());
        assert!(defects("`(contains? (keys m) :id)").is_empty());
        assert!(defects("(quote (contains? (keys m) :id))").is_empty());
    }

    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(defects("'(a ,(contains? (keys m) :id))").is_empty());
        assert!(defects("`(a ,(contains? (keys m) :id))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            defects("`(do ~(contains? (keys m) :id))"),
            vec![ContainsDefect::Sequence]
        );
    }

    #[test]
    fn a_comment_body_is_never_flagged() {
        assert!(defects("(comment (contains? (keys m) :id))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(defects("(println \"(contains? (keys m) :id)\")").is_empty());
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_contains_call_scanned() {
        let report = report("(contains? m :a)\n(contains? (keys m) :a)\n(contains? #{1} 1)\n");
        assert_eq!(report.summary, vec![("contains_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_columns() {
        let report = report("(defn known? [m id]\n  (contains? (keys m) id))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "contains-on-non-associative");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("defect", json!("sequence")),
                ("collection", json!("keys …")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["defect=sequence".to_owned(), "collection=keys …".to_owned(),]
        );
        assert!(finding.message().starts_with("contains? over a sequence"));
    }

    #[test]
    fn the_vector_message_names_the_key_it_proved_is_not_an_index() {
        let finding = &report("(contains? [:a :b] :a)").findings[0];
        assert_eq!(
            finding.message(),
            "contains? on a vector tests indexes, not values, so this is always false; \
             :a is not an integer — use (some #{…} v) or a set"
        );
    }

    /// The producer list must not name anything that returns a vector or a
    /// set, because those are exactly the shapes `RT.contains` handles.
    #[test]
    fn the_producer_list_excludes_the_eager_collection_builders() {
        for eager in [
            "vec",
            "vector",
            "set",
            "into",
            "mapv",
            "filterv",
            "shuffle",
            "zipmap",
            "frequencies",
            "group-by",
            "split-at",
            "merge",
            "select-keys",
            "assoc",
            "hash-map",
            "hash-set",
        ] {
            assert!(
                !SEQ_PRODUCER_HEADS.contains(&eager),
                "{eager} does not produce an ISeq"
            );
        }
    }

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(contains? '(1 2) 1)", Dialect::CommonLisp)
            .expect("parse");
        let report = build_contains_on_non_associative_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("contains_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(contains? m :a)").dialect_modelled);
    }
}
