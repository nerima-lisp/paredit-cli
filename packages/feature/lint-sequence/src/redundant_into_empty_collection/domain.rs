//! Clojure `(into [] coll)` detection: a conversion spelled as an
//! accumulation.
//!
//! `(into [] coll)` conjes every element of `coll` onto an empty vector, which
//! is what `(vec coll)` does and says. Likewise `(into #{} coll)` and
//! `(set coll)`. The direct conversion is one call rather than two, and names
//! the result type instead of building it.
//!
//! # Only two of the four empty literals, and why
//!
//! - **`[]` → `vec`** and **`#{}` → `set`**: reported. `conj` onto a vector
//!   appends and onto a set inserts, so the result of the accumulation is the
//!   same collection the conversion builds, for every input — a seq, a vector,
//!   a map (which conjes as map entries, exactly as `vec`/`set` convert them),
//!   or `nil` (both give the empty collection).
//! - **`(into (list) coll)` / `(into '() coll)`**: *not* reported, and this is
//!   the half of the idea that does not survive contact with `conj`. `conj`
//!   onto a list **prepends**, so `(into '() [1 2 3])` is `(3 2 1)` while
//!   `(apply list [1 2 3])` is `(1 2 3)`. There is no direct conversion to
//!   suggest that preserves order, and suggesting one would silently reverse a
//!   sequence. Reversal may well be exactly what the author wanted, which makes
//!   the shape not even suspicious.
//! - **`(into {} coll)`**: not reported either. `clojure.core` has no
//!   single-function equivalent — `(apply hash-map coll)` takes a flat
//!   key-value sequence rather than the pairs `into` accepts, so it is a
//!   different function, not a shorter spelling of this one.
//!
//! # What is *not* reported, and why
//!
//! - **The transducer arity.** `(into [] (map f) coll)` is three operands and
//!   has no `vec` equivalent; only the exact two-operand call is read.
//! - **A non-empty literal.** `(into [0] coll)` and `(into #{:seen} coll)` keep
//!   their initial contents, which no conversion reproduces.
//! - **A metadata-carrying literal**, `^:foo []`, whose metadata the conversion
//!   would drop.
//!
//! Report-only: `vec` and `into` differ in performance characteristics that
//! depend on the input, and a project may prefer the `into` spelling for
//! symmetry with its transducer-arity siblings. The rewrite is a decision, not
//! a repair.
//!
//! # Head spelling
//!
//! The dispatcher's head index does not fold namespace qualifiers for Clojure,
//! so a call written `clojure.core/into` is not matched. That is a false
//! negative, and the fully qualified spelling of a `clojure.core` staple is
//! rare enough to leave alone rather than to widen the index for.
//!
//! Scope: Clojure only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::for_each_evaluated_subview;

/// The direct conversion an empty literal target stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    /// `(into [] coll)` is `(vec coll)`.
    Vec,
    /// `(into #{} coll)` is `(set coll)`.
    Set,
}

impl Conversion {
    /// The function that replaces the whole call.
    #[must_use]
    pub const fn function(self) -> &'static str {
        match self {
            Self::Vec => "vec",
            Self::Set => "set",
        }
    }

    /// The literal as it is written, for the message.
    #[must_use]
    pub const fn literal(self) -> &'static str {
        match self {
            Self::Vec => "[]",
            Self::Set => "#{}",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedundantIntoItem {
    /// The span of the whole `(into [] coll)` form.
    pub span: ByteSpan,
    /// The span of the source collection.
    pub source_span: ByteSpan,
    pub conversion: Conversion,
}

impl Finding for RedundantIntoItem {
    fn kind(&self) -> &'static str {
        "redundant-into"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.conversion.function().to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("conversion", json!(self.conversion.function())),
            ("source_span", span_json(self.source_span)),
        ]
    }

    fn message(&self) -> String {
        message_for(self.conversion)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(conversion: Conversion) -> String {
    format!(
        "(into {} coll) is ({} coll); the direct conversion names the result \
         instead of accumulating it",
        conversion.literal(),
        conversion.function()
    )
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// The conversion an `into` target stands for, or `None` when it stands for
/// none.
///
/// An empty `[]` is a bracket list with no children and no reader prefix; an
/// empty `#{}` is a brace list with no children carrying exactly the
/// [`ReaderPrefix::HashLiteral`] that distinguishes a set literal from the map
/// literal `{}` — which is deliberately not one of the answers.
fn conversion_for(target: &ExpressionView) -> Option<Conversion> {
    if !target.children.is_empty() {
        return None;
    }
    match target.delimiter? {
        Delimiter::Bracket if target.reader_prefixes.is_empty() => Some(Conversion::Vec),
        Delimiter::Brace if target.reader_prefixes == [ReaderPrefix::HashLiteral] => {
            Some(Conversion::Set)
        }
        _ => None,
    }
}

///
/// # Cost
///
/// One `children.len()` rejects every transducer-arity call, and the
/// `children.is_empty()` inside [`conversion_for`] rejects every call whose
/// target is anything but an empty literal — before any delimiter or prefix is
/// read. `into` is not a dense head to begin with.
pub fn examine(
    view: &ExpressionView,
    into_form_count: &mut usize,
    violations: &mut Vec<RedundantIntoItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["into"]) {
        return;
    }
    *into_form_count += 1;

    // children: [into, target, source] — exactly, so the transducer arity
    // `(into to xform from)` is left alone.
    if view.children.len() != 3 {
        return;
    }
    let Some(conversion) = conversion_for(&view.children[1]) else {
        return;
    };

    violations.push(RedundantIntoItem {
        span: view.span,
        source_span: view.children[2].span,
        conversion,
    });
}

/// Collects every `(into [] coll)` in one file, with the number of `into` forms
/// scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn collect_redundant_intos(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantIntoItem>> {
    if dialect != Dialect::Clojure {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("into_form_count", json!(0))],
        ));
    }

    let mut into_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut into_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("into_form_count", json!(into_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::for_each_subview;

    fn report(input: &str) -> FileFindings<RedundantIntoItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        collect_redundant_intos(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect redundant intos")
    }

    /// `examine` applied to every node of a source, which is what the lint rule
    /// sees through the dispatcher — quoting and all.
    fn examined(input: &str) -> Vec<RedundantIntoItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        let mut count = 0;
        let mut violations = Vec::new();
        for_each_subview(&tree.root_view(), |view| {
            examine(view, &mut count, &mut violations);
        });
        violations
    }

    fn conversions(input: &str) -> Vec<Conversion> {
        examined(input)
            .into_iter()
            .map(|item| item.conversion)
            .collect()
    }

    /// What the rule's whole premise rests on: an empty `[]` is a childless
    /// bracket list, an empty `#{}` is a childless brace list carrying
    /// `HashLiteral`, and the map literal `{}` is a childless brace list
    /// carrying nothing. If the reader ever stopped telling the last two apart,
    /// this rule would recommend `set` for a map.
    #[test]
    fn the_reader_distinguishes_the_three_empty_literals() {
        let tree =
            SyntaxTree::parse_with_dialect("(f [] #{} {} ())", Dialect::Clojure).expect("parse");
        let root = tree.root_view();
        let call = &root.children[0];
        let vector = &call.children[1];
        let set = &call.children[2];
        let map = &call.children[3];
        let list = &call.children[4];

        assert_eq!(vector.delimiter, Some(Delimiter::Bracket));
        assert!(vector.reader_prefixes.is_empty());
        assert!(vector.children.is_empty());

        assert_eq!(set.delimiter, Some(Delimiter::Brace));
        assert_eq!(set.reader_prefixes, vec![ReaderPrefix::HashLiteral]);
        assert!(set.children.is_empty());

        assert_eq!(map.delimiter, Some(Delimiter::Brace));
        assert!(map.reader_prefixes.is_empty());

        assert_eq!(list.delimiter, Some(Delimiter::Paren));

        assert_eq!(conversion_for(vector), Some(Conversion::Vec));
        assert_eq!(conversion_for(set), Some(Conversion::Set));
        assert_eq!(conversion_for(map), None);
        assert_eq!(conversion_for(list), None);
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_an_empty_vector_target() {
        let source = "(into [] coll)";
        let violations = examined(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].conversion, Conversion::Vec);
        assert_eq!(violations[0].conversion.function(), "vec");
        assert!(violations[0].message().contains("(vec coll)"));
    }

    #[test]
    fn flags_an_empty_set_target() {
        assert_eq!(conversions("(into #{} coll)"), vec![Conversion::Set]);
    }

    #[test]
    fn flags_a_compound_source_expression() {
        let source = "(into [] (map inc xs))";
        let violations = examined(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            &source[violations[0].source_span.start().get()..violations[0].source_span.end().get()],
            "(map inc xs)"
        );
    }

    // -- the half of the premise that does not hold --------------------------

    /// `conj` onto a list prepends, so `(into '() [1 2 3])` is `(3 2 1)`.
    /// There is no order-preserving one-call conversion to suggest, and the
    /// reversal may be the point.
    #[test]
    fn does_not_flag_a_list_target() {
        assert!(conversions("(into (list) coll)").is_empty());
        assert!(conversions("(into '() coll)").is_empty());
        assert!(conversions("(into () coll)").is_empty());
    }

    /// `clojure.core` has no single-function equivalent of `(into {} pairs)`.
    #[test]
    fn does_not_flag_a_map_target() {
        assert!(conversions("(into {} pairs)").is_empty());
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_the_transducer_arity() {
        assert!(conversions("(into [] (map inc) coll)").is_empty());
        assert!(conversions("(into #{} (filter odd?) coll)").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_empty_literal_target() {
        assert!(conversions("(into [0] coll)").is_empty());
        assert!(conversions("(into #{:seen} coll)").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_target() {
        assert!(conversions("(into acc coll)").is_empty());
        assert!(conversions("(into (vec xs) coll)").is_empty());
    }

    #[test]
    fn does_not_flag_a_metadata_carrying_literal() {
        assert!(conversions("(into ^:private [] coll)").is_empty());
    }

    #[test]
    fn does_not_flag_a_wrong_arity_into() {
        assert!(conversions("(into [])").is_empty());
        assert!(conversions("(into)").is_empty());
    }

    #[test]
    fn does_not_flag_an_unrelated_head() {
        assert!(conversions("(conj [] x)").is_empty());
        assert!(conversions("(vec coll)").is_empty());
    }

    // -- quoting and strings, through the report path ------------------------

    /// The five quote shapes, in the Clojure spellings: `~` is unquote and `,`
    /// is whitespace, so a test written with a comma would prove nothing.
    #[test]
    fn the_report_skips_the_five_quote_shapes() {
        for source in [
            "'(into [] coll)",
            "(quote (into [] coll))",
            "`(into [] coll)",
            "'(a ~(into [] coll))",
            "'(outer (into [] coll))",
        ] {
            assert!(
                report(source).findings.is_empty(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn an_unquote_inside_a_syntax_quote_is_code_again() {
        assert_eq!(report("`(a ~(into [] coll))").findings.len(), 1);
    }

    #[test]
    fn a_call_inside_a_string_literal_is_not_a_form() {
        assert!(report("(println \"(into [] coll)\")").findings.is_empty());
    }

    // -- report envelope -----------------------------------------------------

    /// `[` is not a list delimiter in Common Lisp, so the source here is one
    /// that parses in both dialects; what is being pinned is that the report
    /// says "not modelled" rather than "clean".
    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(into acc coll)", Dialect::CommonLisp).expect("parse");
        let report = collect_redundant_intos(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(into acc coll)").dialect_modelled);
    }

    #[test]
    fn the_summary_counts_every_into_scanned_not_only_the_flagged_ones() {
        let report = report("(into [] xs)\n(into acc ys)\n");
        assert_eq!(report.summary, vec![("into_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_source_span() {
        let report = report("(defn f [coll]\n  (into [] coll))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-into");
        assert_eq!(finding.text_columns(), vec!["vec".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("conversion", json!("vec")),
                ("source_span", span_json(finding.source_span)),
            ]
        );
    }
}
