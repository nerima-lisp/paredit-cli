//! `unnecessary-copy`: a fresh copy handed straight to something that only
//! reads.
//!
//! `(length (copy-list xs))` allocates a whole list to measure the one it
//! copied. `copy-seq`, `copy-list`, `copy-alist`, and `copy-tree` exist to
//! protect a sequence from being mutated; when the consumer provably does not
//! mutate, the copy is pure allocation and garbage.
//!
//! The consumer list is deliberately short and deliberately conservative: only
//! operators that are specified never to modify their sequence argument. `sort`
//! is absent because `(sort (copy-seq xs) …)` is the *correct* idiom — the copy
//! is what makes the destructive sort safe — and reporting it would be exactly
//! backwards. `nreverse`, `nconc`, and their family are absent for the same
//! reason, and get their own rule
//! ([`crate::copy_before_destructive`]) that rewrites the pair rather than
//! deleting half of it.
//!
//! Fixable: the copy is replaced by the sequence it copied, which is an
//! equivalence given the consumer does not mutate.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleFix, RuleMeta,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::shared::{symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "unnecessary-copy",
    RuleCategory::Allocation,
    Severity::Warning,
    "a copy-seq/copy-list result passed to an operator that only reads it, so the copy is garbage",
    Fixability::Fixable,
)
.with_explanation(
    RuleExplanation::new(
        "A `copy-*` call exists to keep a caller's sequence safe from mutation. When the value \
         goes straight into an operator specified never to modify its argument, nothing is being \
         protected and the copy is allocated only to be discarded.",
    )
    .with_example("(length (copy-list xs))", "(length xs)")
    .with_caveat(
        "`(sort (copy-seq xs) #'<)` is not reported: `sort` is destructive, and the copy is \
         precisely what makes calling it safe.",
    ),
);

/// The copy constructors this rule can remove.
const COPIES: [&str; 5] = [
    "copy-seq",
    "copy-list",
    "copy-alist",
    "copy-tree",
    "copy-structure",
];

/// Operators the standard specifies never modify the sequence argument named
/// here, paired with the argument positions that sequence can occupy.
///
/// The positions matter: `(find item sequence)` reads its *second* argument,
/// and treating position 1 as the sequence would report `(find (copy-list x)
/// s)` — where the copy is the item being searched for and deleting it changes
/// what is compared.
const READERS: [(&str, &[usize]); 16] = [
    ("length", &[1]),
    ("elt", &[1]),
    ("car", &[1]),
    ("first", &[1]),
    ("last", &[1]),
    ("nth", &[2]),
    ("nthcdr", &[2]),
    ("aref", &[1]),
    ("find", &[2]),
    ("position", &[2]),
    ("member", &[2]),
    ("assoc", &[2]),
    ("count", &[2]),
    ("some", &[2]),
    ("every", &[2]),
    ("reduce", &[2]),
];

/// One removable copy: the copy call, and the sequence it copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovableCopy {
    /// The whole `(copy-* …)` form.
    pub span: ByteSpan,
    /// The argument it copied, which replaces it.
    pub source_span: ByteSpan,
    pub copy_operator: String,
    pub reader: String,
}

/// Reads one reader call and reports the copies it renders pointless.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<RemovableCopy> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    let Some((reader, positions)) = READERS
        .iter()
        .find(|(name, _)| symbol_in(head, &[name]))
        .copied()
    else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for position in positions {
        let Some(argument) = view.children.get(*position) else {
            continue;
        };
        // A reader prefix on the copy call (`'(copy-list x)` is not a call at
        // all) means the form is data, not a call to evaluate.
        if !argument.reader_prefixes.is_empty() {
            continue;
        }
        let Some(copy_head) = list_head(argument) else {
            continue;
        };
        if !symbol_in(copy_head, &COPIES) {
            continue;
        }
        // Only the one-argument form: `(copy-seq s)` copies exactly `s`, while
        // a longer call is not a shape this rule modelled and must not rewrite.
        if argument.children.len() != 2 {
            continue;
        }
        found.push(RemovableCopy {
            span: argument.span,
            source_span: argument.children[1].span,
            copy_operator: unqualified(copy_head).to_ascii_lowercase(),
            reader: unqualified(reader).to_ascii_lowercase(),
        });
    }
    found
}

const HEADS: [NormalizedHead; 16] = [
    NormalizedHead::new("length"),
    NormalizedHead::new("elt"),
    NormalizedHead::new("car"),
    NormalizedHead::new("first"),
    NormalizedHead::new("last"),
    NormalizedHead::new("nth"),
    NormalizedHead::new("nthcdr"),
    NormalizedHead::new("aref"),
    NormalizedHead::new("find"),
    NormalizedHead::new("position"),
    NormalizedHead::new("member"),
    NormalizedHead::new("assoc"),
    NormalizedHead::new("count"),
    NormalizedHead::new("some"),
    NormalizedHead::new("every"),
    NormalizedHead::new("reduce"),
];

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
        for copy in examine(view) {
            let replacement = context.slice(copy.source_span).to_owned();
            sink.report_fixed(
                copy.span,
                format!(
                    "{} only reads its argument, so this {} allocates a copy nothing can observe",
                    copy.reader, copy.copy_operator
                ),
                RuleFix::single(
                    copy.span,
                    replacement,
                    format!("Drop the {} nothing mutates", copy.copy_operator),
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn parsed(input: &str) -> (String, ExpressionView) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        (input.to_owned(), view)
    }

    fn sources(input: &str) -> Vec<String> {
        let (text, view) = parsed(input);
        examine(&view)
            .into_iter()
            .map(|copy| {
                text[copy.source_span.start().get()..copy.source_span.end().get()].to_owned()
            })
            .collect()
    }

    #[test]
    fn flags_a_copy_measured_by_length() {
        assert_eq!(sources("(length (copy-list xs))"), vec!["xs"]);
    }

    #[test]
    fn flags_every_copy_constructor() {
        assert_eq!(sources("(car (copy-seq xs))"), vec!["xs"]);
        assert_eq!(sources("(first (copy-alist xs))"), vec!["xs"]);
        assert_eq!(sources("(length (copy-tree xs))"), vec!["xs"]);
    }

    #[test]
    fn reads_the_sequence_position_not_the_item_position() {
        // The copy here is the *item* being searched for; removing it would
        // change what `find` compares against.
        assert!(sources("(find (copy-list x) haystack)").is_empty());
        assert_eq!(sources("(find x (copy-list haystack))"), vec!["haystack"]);
    }

    #[test]
    fn does_not_flag_a_copy_protecting_a_destructive_operator() {
        // `(sort (copy-seq xs) …)` is the correct idiom, not a finding.
        assert!(sources("(sort (copy-seq xs) #'<)").is_empty());
        assert!(sources("(nreverse (copy-list xs))").is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_sequence() {
        assert!(sources("(length xs)").is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_argument_copy_call() {
        // Not a shape this rule modelled, so not one it may rewrite.
        assert!(sources("(length (copy-seq xs 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_form_that_only_looks_like_a_call() {
        assert!(sources("(length '(copy-list xs))").is_empty());
    }

    #[test]
    fn the_replacement_is_the_copied_expression_verbatim() {
        assert_eq!(
            sources("(length (copy-list (remove nil raw)))"),
            vec!["(remove nil raw)"]
        );
    }

    #[test]
    fn the_package_qualified_spelling_is_read_the_same() {
        assert_eq!(sources("(cl:length (cl:copy-list xs))"), vec!["xs"]);
    }
}
