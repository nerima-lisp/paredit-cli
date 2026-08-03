//! What the package-hygiene rules share: which parts of a file are *code*, and
//! the sequence of packages the file's top-level `in-package` forms select.
//!
//! # Evaluation context
//!
//! The quote machinery below ([`QuoteState`], [`for_each_evaluated_subview`],
//! [`is_unevaluated_at`]) is a deliberate copy of
//! `paredit-feature-lint-condition-system`'s `support.rs` — by way of
//! `paredit-feature-lint-build-system`'s span-local refinement of it — and not
//! a cross-package dependency. Two independent counters are required because
//! `'` and `` ` `` are not the same thing:
//!
//! - a comma inside `'(…)` is a comma *character* in a literal list, so `hard`
//!   never clears — a single `i32` depth counter gets `'(a ,X)` wrong;
//! - a comma inside `` `(…) `` escapes back to code, so `quasi` counts up and
//!   down.
//!
//! A node one level *inside* a quote is still data, so a node-local
//! `reader_prefixes` check is not enough either: the state has to be threaded
//! down the tree ([`for_each_evaluated_subview`]) or reconstructed by
//! descending to the node ([`is_unevaluated_at`]).
//!
//! # Cost
//!
//! Nothing here is called per visited node. The `clean/forms/*` benchmarks lint
//! files with zero findings, so the per-file cost of a rule that matches
//! nothing is exactly what they measure — and those fixtures contain no
//! `in-package`, so nothing in this module runs on them at all.
//!
//! [`SyntaxTree::root_view`] is never called. It deep-materializes the whole
//! document — a `Vec` per node and a `String` per atom, uncached, on every call.
//! [`is_unevaluated_at`] instead binary-searches the top level and materializes
//! only the one top-level form containing its target, and
//! [`top_level_in_package_chain`] materializes nothing at all: it reads each
//! top-level form's head and argument through [`SyntaxTree::select_path`],
//! which walks node ids, and slices their text out of the source.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_is, unqualified};

// --- evaluation context ---------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none
    /// of them turns code into data.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
///
/// Iterative, not recursive: a deeply nested document must not depend on stack
/// depth.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for child in view.children.iter().rev() {
            stack.push((child, inside));
        }
    }
}

/// The one child of `view` whose span covers `target`, found without reading
/// the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let child = view.children.get(after.checked_sub(1)?)?;
    span_contains(child.span, target).then_some(child)
}

/// The index of the top-level form containing `target`, or `None`.
///
/// Top-level forms are in document order and do not overlap, so the only
/// candidate is the last one beginning at or before `target`. Each step of the
/// search is a node-id lookup and a span read; nothing is materialized.
#[must_use]
pub fn root_child_index_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
    let start_of = |index: usize| {
        tree.select_path(&Path::root_child(index))
            .ok()
            .map(|selection| selection.span().start().get())
    };
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if start_of(middle)? <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.checked_sub(1)?;
    let span = tree.select_path(&Path::root_child(index)).ok()?.span();
    span_contains(span, target).then_some(index)
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document. [`is_unevaluated_at`] is called once per candidate,
/// which would make a file of T candidates cost T×T; no `--rule` or `--exclude`
/// could avoid it, since `inspect lint` runs every rule and filters afterwards.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let index = root_child_index_containing(tree, target)?;
    Some(tree.select_path(&Path::root_child(index)).ok()?.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(in-package :app)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all. A span inside no
/// top-level form at all — one a caller synthesized rather than took from the
/// tree — is evaluated, because nothing quotes it.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(top_level) = root_child_containing(tree, target) else {
        return false;
    };
    let mut view: &ExpressionView = &top_level;
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(view);

    while view.span != target {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = child_containing(view, target) else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
    }
    state.is_data()
}

// --- symbols --------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every head comparison here is written
/// in.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    unqualified(text).to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

// --- package designators --------------------------------------------------

/// The operator this package's rule anchors on, and the byte-scan needle:
/// `in-package`, `cl:in-package` and `CL:IN-PACKAGE` all contain it.
pub const IN_PACKAGE: &str = "in-package";

/// The packages every Common Lisp image provides without a `defpackage` form.
///
/// The same set `paredit_feature_project_analysis::undefined_package_report`
/// exempts, for the same reason: they are not declared anywhere, so nothing in
/// a file can be compared against their declaration. Here they matter for a
/// second reason as well — bouncing out to `CL-USER` between two package
/// declarations is a normal single-file-library shape, and re-entering
/// `CL-USER` is not a defect.
///
/// Written in the normalized (lowercase, unqualified, colon-free) spelling
/// [`package_designator_name`] produces.
pub const AMBIENT_PACKAGES: [&str; 5] = [
    "cl",
    "common-lisp",
    "cl-user",
    "common-lisp-user",
    "keyword",
];

/// Whether `name` — already normalized — is one of [`AMBIENT_PACKAGES`].
#[must_use]
pub fn is_ambient_package(name: &str) -> bool {
    AMBIENT_PACKAGES.contains(&name)
}

/// One package designator, reduced to the name it denotes.
///
/// `:app`, `#:app`, `"APP"`, `app` and `CL:APP` all designate the package named
/// `APP`, and this returns `app` for every one of them. Case is folded because
/// the standard reader upcases unescaped symbol text, so `:app` and `:APP` are
/// the same designator; a `"…"` string designator is *not* upcased by the
/// reader, but a package written as `"APP"` in one place and `:app` in another
/// is the same package in every real program, so folding both to lowercase is
/// what makes the two forms compare equal.
///
/// Returns `None` for text that designates nothing readable — the empty string,
/// or a designator that is only its own punctuation.
///
/// A leading `:` is stripped here, which [`unqualified`] deliberately does not
/// do. That is not a disagreement: for a *symbol*, `:test` and `test` are
/// different objects and folding them would make a keyword compare equal to a
/// function name. For a *package designator* the leading colon is the keyword
/// idiom for naming a package, so `:app`, `#:app`, `app` and `"APP"` all
/// designate the one package `APP` and must fold together.
#[must_use]
pub fn package_designator_name(text: &str) -> Option<String> {
    let trimmed = text.trim();
    // A `"…"` string designator: take what is between the quotes verbatim.
    let bare = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        // `#:app` → `app`; `:app` → `app`; `cl:app` → `app`. The uninterned
        // marker `#:` comes off first, then the keyword marker, then
        // `unqualified` takes any remaining `pkg:`/`pkg::` prefix.
        let bare = trimmed.strip_prefix("#:").unwrap_or(trimmed);
        unqualified(bare.trim_start_matches(':'))
    };
    let bare = bare.trim();
    if bare.is_empty() {
        return None;
    }
    Some(bare.to_ascii_lowercase())
}

/// One top-level `(in-package …)` form, as the chain reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InPackageEntry {
    /// The index of this form among the document's top-level children.
    pub root_index: usize,
    /// The span of the whole `(in-package …)` form, which is what a finding
    /// points at.
    pub span: ByteSpan,
    /// The designated package, normalized by [`package_designator_name`].
    pub package: String,
    /// The designator exactly as written, so a message can echo the source
    /// rather than this module's spelling of it.
    pub written: String,
}

/// Whether `head_text` — a form's head, borrowed from the source — spells
/// `in-package` in any case and past any package qualifier.
fn head_is_in_package(head_text: &str) -> bool {
    symbol_is(head_text, IN_PACKAGE)
}

/// The exact source of `span`, or `""` for a span that is not a character
/// boundary of this document.
fn slice(source: &str, span: ByteSpan) -> &str {
    source
        .get(span.start().get()..span.end().get())
        .unwrap_or_default()
}

/// The packages this file's *top-level* `in-package` forms select, in document
/// order.
///
/// # Why only the top level
///
/// CLHS 11.1.2.2 requires `in-package` to appear at top level for its
/// compile-time effect. One nested inside `eval-when`, `progn` or a function
/// body does not switch the reader for the following top-level forms, so it is
/// not part of the chain the reader actually walks and including it would
/// report a switch that never happened.
#[must_use]
pub fn top_level_in_package_chain(tree: &SyntaxTree) -> Vec<InPackageEntry> {
    top_level_in_package_chain_through(tree, tree.root_children().len().saturating_sub(1))
}

/// [`top_level_in_package_chain`], stopping after the top-level form at
/// `last_index`.
///
/// # Why a bound exists at all
///
/// Whether a given switch re-enters a package it had already left depends
/// entirely on what came *before* it; nothing after it can change the answer,
/// and nothing after it can change which earlier re-entry was reported first
/// either. A rule dispatched at one switch therefore never needs the rest of
/// the file, and reading it would be work thrown away.
///
/// This is what keeps the ordinary case free rather than merely cheap. An
/// implementation file has exactly one top-level `in-package`, at the very top,
/// and a chain of one can never contain a re-entry — so the scan that rule pays
/// for stops after one or two forms instead of walking a thousand definitions
/// it cannot learn anything from.
///
/// # Cost
///
/// One [`SyntaxTree::select_path`] per top-level form scanned, whose path is a
/// single index; resolving it is a node-id lookup, and
/// [`paredit_core_syntax::sexpr::Selection::head`] and `span` then borrow out of
/// the source without allocating. Only a form whose head is `in-package` costs
/// anything more: one further `select_path` for the designator, one owned
/// `String` for it, and one [`is_unevaluated_at`] call to reject
/// `'(in-package :a)`, which materializes that one small form.
///
/// No `ExpressionView` is built for an ordinary definition, and
/// [`SyntaxTree::root_view`] — which deep-materializes the whole document,
/// uncached, on every call — is never called at all.
#[must_use]
pub fn top_level_in_package_chain_through(
    tree: &SyntaxTree,
    last_index: usize,
) -> Vec<InPackageEntry> {
    let source = tree.source();
    let mut chain = Vec::new();
    let limit = last_index.min(tree.root_children().len().saturating_sub(1));
    for root_index in 0..tree.root_children().len().min(limit + 1) {
        let form = Path::root_child(root_index);
        let Ok(selection) = tree.select_path(&form) else {
            continue;
        };
        // `head` is `None` for an atom and for an empty list, neither of which
        // is an `in-package` form. Borrowed from the source: no allocation.
        let Some(head) = selection.head() else {
            continue;
        };
        if !head_is_in_package(head) {
            continue;
        }
        let span = selection.span();
        // `'(in-package :a)` is a list of two symbols. Asked only of the
        // handful of forms that got this far, never of a definition.
        if is_unevaluated_at(tree, span) {
            continue;
        }
        let Ok(designator) = tree.select_path(&form.child(1)) else {
            // `(in-package)` is malformed; `malformed`-category rules own that
            // complaint, and this one cannot name a package for it.
            continue;
        };
        let written = slice(source, designator.span()).to_owned();
        let Some(package) = package_designator_name(&written) else {
            continue;
        };
        chain.push(InPackageEntry {
            root_index,
            span,
            package,
            written,
        });
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::view_query::is_paren_list;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source);
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    // --- the five quote shapes, on the walk

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(in-package (foo))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (in-package (foo)))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(in-package (foo))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(in-package (foo)))"),
            vec!["in-package", "foo"]
        );
    }

    /// The shape a single `i32` depth counter reads wrongly: a comma inside a
    /// *hard* quote is a literal comma, not an escape back to code.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(in-package (foo)))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(in-package (foo))\")"), vec!["f"]);
    }

    // --- the five quote shapes, on the span lookup

    /// The span-directed lookup must agree with the walk, since one is used by
    /// the standalone report and the other by the registered rule.
    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node
        // even when it is data.
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f () (in-package :a))",
            "in-package"
        ));
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("'(in-package :a)", "in-package"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(quote (in-package :a))",
            "in-package"
        ));
    }

    #[test]
    fn a_span_inside_a_backquote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("`(in-package :a)", "in-package"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "`(a ,(in-package :a))",
            "in-package"
        ));
    }

    #[test]
    fn a_span_under_a_comma_inside_a_hard_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "'(a ,(in-package :a))",
            "in-package"
        ));
    }

    /// The whole point of the span-local lookup: it must not need the root
    /// view, and it must give the same answer whichever top-level form the
    /// target is in.
    #[test]
    fn the_span_lookup_is_read_at_the_target_not_at_an_ancestor() {
        let parsed = tree("(defun a () 1)\n(defun b () 2)\n'(in-package :x)\n(in-package :y)\n");
        let quoted = parsed
            .select_path(&Path::root_child(2))
            .expect("the quoted form")
            .span();
        let live = parsed
            .select_path(&Path::root_child(3))
            .expect("the live form")
            .span();
        assert!(is_unevaluated_at(&parsed, quoted));
        assert!(!is_unevaluated_at(&parsed, live));
    }

    #[test]
    fn a_span_inside_no_top_level_form_is_evaluated() {
        let parsed = tree("(a)");
        let past_the_end = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(100),
            paredit_core_syntax::sexpr::ByteOffset::new(200),
        );
        assert!(!is_unevaluated_at(&parsed, past_the_end));
    }

    // --- designators

    #[test]
    fn every_designator_spelling_reduces_to_the_same_name() {
        for text in [":app", "#:app", "app", "\"APP\"", "CL:APP", ":APP", "#:APP"] {
            assert_eq!(
                package_designator_name(text).as_deref(),
                Some("app"),
                "`{text}` did not designate `app`"
            );
        }
    }

    #[test]
    fn a_designator_that_names_nothing_is_none() {
        for text in ["", "   ", ":", "#:", "\"\""] {
            assert_eq!(
                package_designator_name(text),
                None,
                "`{text}` should designate nothing"
            );
        }
    }

    #[test]
    fn a_slash_qualified_package_inferred_name_survives_intact() {
        // `unqualified` strips a *package marker*; a `/` is part of the name.
        assert_eq!(
            package_designator_name(":app/util").as_deref(),
            Some("app/util")
        );
    }

    #[test]
    fn the_ambient_set_is_recognised_in_the_normalized_spelling() {
        for text in [":cl-user", "#:CL-USER", "\"COMMON-LISP-USER\"", ":keyword"] {
            let name = package_designator_name(text).expect("a name");
            assert!(is_ambient_package(&name), "`{text}` should be ambient");
        }
        assert!(!is_ambient_package("app"));
        assert!(!is_ambient_package("cl-ppcre"));
    }

    #[test]
    fn the_head_test_folds_case_and_package_qualifiers() {
        for text in [
            "in-package",
            "IN-PACKAGE",
            "cl:in-package",
            "CL::IN-PACKAGE",
        ] {
            assert!(head_is_in_package(text), "`{text}` should match");
        }
        for text in ["in-packages", "find-package", "defpackage"] {
            assert!(!head_is_in_package(text), "`{text}` should not match");
        }
    }

    // --- the chain

    fn chain(source: &str) -> Vec<String> {
        top_level_in_package_chain(&tree(source))
            .into_iter()
            .map(|entry| entry.package)
            .collect()
    }

    #[test]
    fn the_chain_is_the_top_level_switches_in_document_order() {
        assert_eq!(
            chain("(in-package :a)\n(defun f () 1)\n(in-package :b)\n(defun g () 2)\n"),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn the_chain_folds_every_designator_spelling_together() {
        assert_eq!(
            chain("(in-package #:app)\n(in-package \"APP\")\n(in-package :APP)\n"),
            vec!["app".to_owned(), "app".to_owned(), "app".to_owned()]
        );
    }

    #[test]
    fn a_nested_in_package_is_not_part_of_the_chain() {
        assert_eq!(
            chain("(in-package :a)\n(eval-when (:execute) (in-package :b))\n"),
            vec!["a".to_owned()]
        );
        assert_eq!(
            chain("(defun f () (in-package :b))\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_quoted_top_level_in_package_is_not_part_of_the_chain() {
        for source in [
            "'(in-package :a)",
            "(quote (in-package :a))",
            "`(in-package :a)",
            "'(x ,(in-package :a))",
        ] {
            assert_eq!(chain(source), Vec::<String>::new(), "`{source}` entered");
        }
    }

    #[test]
    fn an_unquoted_in_package_inside_a_backquote_is_not_top_level_either() {
        // The top-level form is the backquoted list, not the `in-package`, so
        // it is out of scope for a different reason than being data.
        assert_eq!(chain("`(x ,(in-package :a))"), Vec::<String>::new());
    }

    #[test]
    fn a_qualified_or_upcased_operator_still_enters_the_chain() {
        assert_eq!(
            chain("(cl:in-package :a)\n(IN-PACKAGE :b)\n"),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn a_malformed_in_package_with_no_designator_is_skipped() {
        assert_eq!(
            chain("(in-package)\n(in-package :a)\n"),
            vec!["a".to_owned()]
        );
    }

    #[test]
    fn an_in_package_mentioned_only_in_a_string_is_not_a_form() {
        assert_eq!(
            chain("(format t \"(in-package :a)\")\n"),
            Vec::<String>::new()
        );
    }

    /// `#+sbcl (in-package :a)` parses as a single opaque atom in this
    /// codebase, so it is not a list and has no head to match.
    #[test]
    fn a_reader_conditional_switch_is_one_atom_and_never_enters_the_chain() {
        let parsed = tree("#+sbcl (in-package :a)\n(in-package :b)\n");
        let entries = top_level_in_package_chain(&parsed);
        assert_eq!(
            entries
                .iter()
                .map(|e| e.package.as_str())
                .collect::<Vec<_>>(),
            vec!["b"]
        );
    }

    #[test]
    fn an_entry_records_its_span_index_and_written_designator() {
        let parsed = tree("(defun f () 1)\n(in-package #:app)\n");
        let entries = top_level_in_package_chain(&parsed);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].root_index, 1);
        assert_eq!(entries[0].written, "#:app");
        assert_eq!(entries[0].package, "app");
        let expected = parsed
            .select_path(&Path::root_child(1))
            .expect("the form")
            .span();
        assert_eq!(entries[0].span, expected);
    }

    #[test]
    fn a_top_level_atom_has_no_head_and_is_skipped_without_error() {
        assert_eq!(
            chain("42\n:keyword\n\"a string\"\n()\n"),
            Vec::<String>::new()
        );
    }

    // --- the scan bound

    /// The bound is an optimisation and must not be observable in the answer:
    /// a chain read through form `i` has to equal the prefix of the full chain
    /// that ends at form `i`.
    #[test]
    fn a_bounded_chain_is_exactly_the_prefix_of_the_full_one() {
        let parsed = tree(
            "(in-package :a)\n\
             (defun f () 1)\n\
             (in-package :b)\n\
             (defun g () 2)\n\
             (in-package :a)\n\
             (defun h () 3)\n",
        );
        let full = top_level_in_package_chain(&parsed);
        assert_eq!(full.len(), 3);
        for last_index in 0..parsed.root_children().len() {
            let bounded = top_level_in_package_chain_through(&parsed, last_index);
            let expected: Vec<InPackageEntry> = full
                .iter()
                .filter(|entry| entry.root_index <= last_index)
                .cloned()
                .collect();
            assert_eq!(bounded, expected, "bound at {last_index}");
        }
    }

    #[test]
    fn a_bound_past_the_end_reads_the_whole_file() {
        let parsed = tree("(in-package :a)\n(in-package :b)\n");
        assert_eq!(
            top_level_in_package_chain_through(&parsed, 999),
            top_level_in_package_chain(&parsed)
        );
    }

    #[test]
    fn a_bound_before_the_first_switch_reads_nothing() {
        let parsed = tree("(defun f () 1)\n(in-package :a)\n");
        assert!(top_level_in_package_chain_through(&parsed, 0).is_empty());
        assert_eq!(top_level_in_package_chain_through(&parsed, 1).len(), 1);
    }

    /// A bound on an empty document must not underflow to "the whole file".
    #[test]
    fn a_bound_on_an_empty_document_is_empty() {
        let parsed = tree("");
        assert!(top_level_in_package_chain_through(&parsed, 0).is_empty());
        assert!(top_level_in_package_chain(&parsed).is_empty());
    }

    #[test]
    fn a_file_with_no_top_level_forms_has_an_empty_chain() {
        assert_eq!(chain(""), Vec::<String>::new());
        assert_eq!(chain(";; only a comment\n"), Vec::<String>::new());
    }

    #[test]
    fn is_paren_list_is_not_confused_by_a_bracket_form() {
        // Guards the assumption `top_level_in_package_chain` leans on: a form
        // written with brackets is still read as a list here, so restricting to
        // parens is the rule's job, not the chain's.
        let parsed = tree("(in-package :a)");
        let view = parsed
            .select_path(&Path::root_child(0))
            .expect("the form")
            .view();
        assert!(is_paren_list(&view));
    }
}
