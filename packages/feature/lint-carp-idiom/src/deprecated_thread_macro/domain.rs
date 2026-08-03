//! `carp-deprecated-thread-macro` detection: a threading macro that Carp's own
//! standard library marks deprecated.
//!
//! This is not a judgement call, and it is not read off prose. `core/ControlMacros.carp`
//! declares it in Carp:
//!
//! ```text
//! core/ControlMacros.carp:27  (deprecated =>  "deprecated in favor of `->`.")
//! core/ControlMacros.carp:28  (defmacro =>  [:rest forms] (thread-first-internal forms))
//! core/ControlMacros.carp:31  (deprecated ==> "deprecated in favor of `-->`.")
//! core/ControlMacros.carp:32  (defmacro ==> [:rest forms] (thread-last-internal forms))
//! ```
//!
//! and the replacements are defined in the same file from the *same* helper:
//!
//! ```text
//! core/ControlMacros.carp:45  (defmacro ->  [:rest forms] (thread-first-internal forms))
//! core/ControlMacros.carp:59  (defmacro --> [:rest forms] (thread-last-internal forms))
//! ```
//!
//! # Why the compiler will not tell you
//!
//! `(deprecated name "…")` is `core/Macros.carp:174`, and it expands to
//! `(meta-set! name "deprecated" v)` — it only writes metadata. The compiler
//! reads that key in exactly two places:
//!
//! - `src/Primitives.hs:355`, inside `primitiveInfo` — the REPL's `(info …)`
//!   command, which a build never invokes.
//! - `src/RenderDocs.hs:197-219`, which renders a "deprecated" badge into
//!   generated HTML documentation.
//!
//! There is no call from any compilation path. **Using `=>` compiles cleanly
//! with no diagnostic whatsoever**, which is precisely why the spelling
//! survives: Carp's own `core/Binary.carp:68,77` still uses `==>`.
//!
//! # Why the fix is safe
//!
//! `=>` and `->` have byte-identical macro bodies, as do `==>` and `-->`, so
//! the replacement is a rename and not a rewrite. The one way that could stop
//! being true is a file that defines its own `->`; the rule withholds the fix
//! in that case rather than assuming core's binding is the one in scope.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};

use crate::support::head_symbol;

/// Carp only. `=>` is a live, non-deprecated operator elsewhere: it is
/// Clojure's `clojure.test` assertion arrow and a common user-defined macro,
/// and `==>` is an implication operator in several test libraries. Widening
/// the scope would report on code Carp's standard library says nothing about.
pub const DIALECTS: [Dialect; 1] = [Dialect::Carp];

/// One deprecated threading macro and the replacement core names for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deprecation {
    pub head: &'static str,
    pub replacement: &'static str,
    /// Where in Carp's own sources the deprecation is declared.
    pub citation: &'static str,
}

/// Every binding in Carp's standard library carrying `deprecated` metadata.
///
/// The list is exhaustive as of the upstream tree scanned: `(deprecated …)`
/// has exactly two call sites in all of `core/`, both in `ControlMacros.carp`.
/// It is a closed set because Carp's deprecation mechanism is an explicit
/// declaration rather than an inferred property — a new one would be a new
/// `(deprecated …)` line.
pub const DEPRECATIONS: [Deprecation; 2] = [
    Deprecation {
        head: "=>",
        replacement: "->",
        citation: "core/ControlMacros.carp:27, `(deprecated => \"deprecated in favor of `->`.\")`",
    },
    Deprecation {
        head: "==>",
        replacement: "-->",
        citation: "core/ControlMacros.carp:31, `(deprecated ==> \"deprecated in favor of `-->`.\")`",
    },
];

/// The deprecation for `head`, if it names one.
#[must_use]
pub fn deprecation_for(head: &str) -> Option<Deprecation> {
    DEPRECATIONS
        .iter()
        .copied()
        .find(|entry| entry.head == head)
}

/// One use of a deprecated threading macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedUse {
    /// The whole call, which is what gets reported.
    pub span: ByteSpan,
    /// The head atom alone, which is the only part a fix rewrites.
    pub head_span: ByteSpan,
    pub deprecation: Deprecation,
}

/// Examines one form.
///
/// Reads the head and nothing else. It deliberately does **not** check the
/// call's arity: for Carp this workspace's reader inflates the child count of
/// any call containing `@(…)` or `&(…)`, so an arity guard here would silently
/// drop real findings — `(=> x @(f y))` reads as four children, not three.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<DeprecatedUse> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let head = head_symbol(view)?;
    let deprecation = deprecation_for(head)?;
    let head_span = view.children.first()?.span;
    Some(DeprecatedUse {
        span: view.span,
        head_span,
        deprecation,
    })
}

/// Every deprecated threading macro call in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<DeprecatedUse> {
    let root = tree.root_view();
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// Every threading macro Carp defines, deprecated or not.
///
/// This is the population [`candidate_count`] measures, and the choice matters.
/// Counting only `=>` and `==>` would make the denominator equal the numerator
/// — this rule has no arity or shape guard to reject a matched head — so a
/// clean corpus would report "0 findings over 0 candidates", which is the
/// false-clean a denominator exists to rule out. Counting *all four* asks the
/// question the rule actually adjudicates: of the threading macro calls in
/// this code, how many use a spelling core deprecated?
pub const THREADING_HEADS: [&str; 4] = ["->", "-->", "=>", "==>"];

/// How many threading macro calls this rule adjudicated: the denominator for a
/// zero-finding sweep.
///
/// A clean run over a corpus that contains no threading macros at all is not
/// evidence of anything, and this is what distinguishes the two.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| THREADING_HEADS.contains(&head)) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heads(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        collect(dialect, &tree)
            .into_iter()
            .map(|item| item.deprecation.head)
            .collect()
    }

    #[test]
    fn flags_both_deprecated_spellings() {
        assert_eq!(heads("(=> 8 inc inc)", Dialect::Carp), vec!["=>"]);
        assert_eq!(heads("(==> results f)", Dialect::Carp), vec!["==>"]);
    }

    #[test]
    fn names_the_replacement_core_names() {
        assert_eq!(deprecation_for("=>").expect("entry").replacement, "->");
        assert_eq!(deprecation_for("==>").expect("entry").replacement, "-->");
    }

    #[test]
    fn leaves_the_supported_spellings_alone() {
        assert!(heads("(-> 1 (- 10) (* 5))", Dialect::Carp).is_empty());
        assert!(heads("(--> 1 (- 10) (/ 45))", Dialect::Carp).is_empty());
    }

    /// `head_key` is verbatim for Carp, so these must not collide.
    #[test]
    fn a_head_that_merely_contains_the_same_bytes_is_not_one() {
        assert!(heads("(=>> x f)", Dialect::Carp).is_empty());
        assert!(heads("(===> x f)", Dialect::Carp).is_empty());
        assert!(heads("(= a b)", Dialect::Carp).is_empty());
        assert!(heads("(x=> a b)", Dialect::Carp).is_empty());
    }

    /// The sigil is part of the atom text for Carp, so a `=>` appearing as an
    /// *argument* rather than a head is not a call to it.
    #[test]
    fn the_operator_in_argument_position_is_not_a_call() {
        assert!(heads("(map => xs)", Dialect::Carp).is_empty());
    }

    /// A `[…]` in Carp is an array literal or a parameter vector, never a
    /// call, so its first element is not a head.
    ///
    /// The engine's head index happens to be paren-only too, so the dispatcher
    /// would not hand the rule one of these — but `collect` and
    /// `candidate_count` walk the tree themselves and do see them, and without
    /// the delimiter test both would read an array of symbols as a call.
    /// Mutation-testing found this: relaxing `head_symbol` to accept any list
    /// killed no test until this one existed.
    #[test]
    fn a_bracket_or_brace_list_is_not_a_call() {
        assert!(heads("(def ops [=> ==> ->])", Dialect::Carp).is_empty());
        assert!(heads("(def m {=> 1})", Dialect::Carp).is_empty());
        // And the denominator must not count them either.
        let tree = SyntaxTree::parse_with_dialect("(def ops [=> ==> -> -->])", Dialect::Carp)
            .expect("parse");
        assert_eq!(candidate_count(Dialect::Carp, &tree), 0);
    }

    #[test]
    fn a_nested_use_is_reached() {
        assert_eq!(
            heads("(defn f [state]\n  (=> state inc))", Dialect::Carp),
            vec!["=>"]
        );
    }

    /// The reader gives `@(…)` an extra sibling. This pins that the rule still
    /// fires through it, which an arity guard would have broken.
    #[test]
    fn an_inflated_arity_call_is_still_reached() {
        assert_eq!(heads("(=> x @(f y))", Dialect::Carp), vec!["=>"]);
        assert_eq!(heads("(==> x &(f y))", Dialect::Carp), vec!["==>"]);
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        assert!(heads("(=> 8 inc)", Dialect::Clojure).is_empty());
        assert!(heads("(=> 8 inc)", Dialect::CommonLisp).is_empty());
        assert!(heads("(=> 8 inc)", Dialect::Fennel).is_empty());
    }

    #[test]
    fn the_candidate_count_counts_every_threading_macro() {
        let tree =
            SyntaxTree::parse_with_dialect("(=> a b) (==> c d) (-> e f) (--> g h)", Dialect::Carp)
                .expect("parse");
        // All four are adjudicated; two are deprecated.
        assert_eq!(candidate_count(Dialect::Carp, &tree), 4);
        assert_eq!(collect(Dialect::Carp, &tree).len(), 2);
        // And nothing at all for a dialect out of scope.
        let other = SyntaxTree::parse_with_dialect("(=> a b)", Dialect::Clojure).expect("parse");
        assert_eq!(candidate_count(Dialect::Clojure, &other), 0);
    }

    /// The denominator has to be able to exceed the numerator, or it is not a
    /// denominator. Correct code has candidates and no findings.
    #[test]
    fn correct_code_has_candidates_and_no_findings() {
        let tree = SyntaxTree::parse_with_dialect("(-> a b) (--> c d) (-> e f)", Dialect::Carp)
            .expect("parse");
        assert_eq!(candidate_count(Dialect::Carp, &tree), 3);
        assert!(collect(Dialect::Carp, &tree).is_empty());
    }

    #[test]
    fn the_head_span_covers_only_the_operator() {
        let source = "(=> 8 inc)";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Carp).expect("parse");
        let item = collect(Dialect::Carp, &tree).remove(0);
        assert_eq!(item.head_span.slice(source), "=>");
        assert_eq!(item.span.slice(source), "(=> 8 inc)");
    }
}
