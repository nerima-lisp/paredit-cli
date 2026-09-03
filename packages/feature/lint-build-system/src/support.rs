//! What the build-system rules share: which parts of a file are *code*, and
//! how to read the two forms every rule here anchors on — `defsystem` and
//! `defpackage`.
//!
//! # Evaluation context
//!
//! The quote machinery below ([`QuoteState`], [`for_each_evaluated_subview`],
//! [`is_unevaluated_at`]) is a deliberate copy of
//! `paredit-feature-lint-condition-system`'s `support.rs`, not a new design and
//! not a cross-package dependency. Two independent counters are required
//! because `'` and `` ` `` are not the same thing:
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
//! nothing is exactly what they measure. Every rule in this package anchors on
//! `HeadFilter::Heads`, confirms a candidate from that node alone, and only then
//! asks a whole-file question. No rule here touches
//! `RuleContext::binding_table`/`value_table`/`type_table` at all.
//!
//! Nor is anything here quadratic in the number of candidates.
//! [`is_unevaluated_at`] is called once per candidate, so it is allowed to cost
//! the *enclosing top-level form* and never the file: it binary-searches the top
//! level for the one root child containing the span and materializes only that
//! form. Starting it from `tree.root_view()` instead — which builds an
//! `ExpressionView` for every node in the document — is what made a file of T
//! candidates cost T×T, and no `--rule` or `--exclude` could avoid it, since
//! `inspect lint` runs every rule and filters afterwards.
//!
//! # ASDF spellings
//!
//! `defsystem` is written `defsystem`, `asdf:defsystem`, `asdf::defsystem` and
//! `asdf/parse-defsystem:defsystem` in the wild. The engine's head index folds
//! all four onto the key `defsystem` (see
//! `paredit_core_lint_engine::engine::head_index::head_key`, which strips any
//! package qualifier), and [`symbol_is`] does the same for a rule's own
//! re-check. This package therefore never goes through
//! `paredit_core_syntax::common_lisp::operator`, whose table lists only
//! `"asdf:defsystem" | "defsystem"` and would silently skip the
//! `asdf/parse-defsystem:` spelling.

use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
};

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

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document. [`is_unevaluated_at`] is called once per candidate,
/// which made a file of T candidates cost T×T, and no `--rule` or `--exclude`
/// could avoid it, since `inspect lint` runs every rule and filters afterwards.
///
/// Selecting the one root child instead costs a binary search over the top
/// level — each step a node-id lookup and a span read, neither of which
/// allocates — plus that one form's own subtree.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let start_of = |index: usize| {
        tree.select_path(&Path::root_child(index))
            .ok()
            .map(|selection| selection.span().start().get())
    };
    // Top-level forms are in document order and do not overlap, so the only
    // candidate is the last one beginning at or before `target`.
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
    let selection = tree
        .select_path(&Path::root_child(low.checked_sub(1)?))
        .ok()?;
    span_contains(selection.span(), target).then(|| selection.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(defsystem "x")) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it, and
/// that is already modelled by `hard` never clearing.
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

/// Whether `view` is the keyword `keyword` (given with its colon, e.g.
/// `":version"`), in any case and past any package qualifier.
#[must_use]
pub fn is_keyword(view: &ExpressionView, keyword: &str) -> bool {
    symbol_name(view).is_some_and(|name| name == keyword)
}

/// Whether the file's bytes contain `needle` at all, ignoring ASCII case.
///
/// A byte scan, not a tree walk: no allocation, no per-node work, and it reads
/// each byte once instead of visiting each node. Used only as a *negative*
/// guard — an answer of `true` may come from a string or a comment, in which
/// case the real analysis runs and finds nothing, exactly as it would have
/// without the guard.
#[must_use]
pub fn mentions(source: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    source.len() >= needle.len()
        && source
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

// --- ASDF -----------------------------------------------------------------

/// Whether `view` is a `(defsystem …)` form, in any of the four spellings ASDF
/// is written with (`defsystem`, `asdf:defsystem`, `asdf::defsystem`,
/// `asdf/parse-defsystem:defsystem`).
#[must_use]
pub fn is_defsystem(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| symbol_is(head, "defsystem"))
}

/// A system-name designator reduced to the string ASDF's `coerce-name` would
/// produce.
///
/// `coerce-name` is case-*preserving* for a string designator and
/// case-*folding* for a symbol one: the reader upcases `:Foo` to the symbol
/// `FOO`, and `coerce-name` then does `string-downcase`. So `:Foo`, `#:FOO` and
/// `foo` all name the system `"foo"`, while `"Foo"` names `"Foo"` and `"foo"`
/// names `"foo"` — two different systems.
///
/// Reproducing that exactly is what keeps [`crate::asdf_self_referential_depends_on`]
/// from reporting a self-dependency that ASDF would not see, in either
/// direction.
#[must_use]
pub fn asdf_name(text: &str) -> Option<String> {
    if let Some(inner) = text.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        // A string designator: taken verbatim, escapes resolved.
        return Some(unescape_string(inner));
    }
    let symbol = text
        .strip_prefix("#:")
        .or_else(|| text.strip_prefix(':'))
        .unwrap_or(text);
    (!symbol.is_empty()).then(|| symbol.to_ascii_lowercase())
}

fn unescape_string(inner: &str) -> String {
    let mut result = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(escaped) = characters.next() {
                result.push(escaped);
            }
        } else {
            result.push(character);
        }
    }
    result
}

/// The name a `(defsystem NAME …)` form declares, as ASDF would coerce it.
#[must_use]
pub fn defsystem_name(view: &ExpressionView) -> Option<String> {
    atom_text(view.children.get(1)?).and_then(asdf_name)
}

/// A `defsystem`'s option plist: everything after the head and the name.
#[must_use]
pub fn defsystem_options(view: &ExpressionView) -> &[ExpressionView] {
    view.children.get(2..).unwrap_or_default()
}

/// The value of `keyword` in a `defsystem` option plist, or `None`.
///
/// Reads the plist as the strict keyword/value pairing ASDF requires, and
/// **bails at the first even-indexed element that is not a keyword** rather
/// than resynchronizing. A `#+sbcl :depends-on (…)` puts a reader-conditional
/// atom in a key position, and every following pair is then off by one; a
/// resynchronizing scan would read a *value* as a key and answer confidently
/// about the wrong form. Bailing turns that into a missed finding instead.
#[must_use]
pub fn plist_value<'a>(options: &'a [ExpressionView], keyword: &str) -> Option<&'a ExpressionView> {
    let mut index = 0;
    while index < options.len() {
        let key = symbol_name(&options[index])?;
        if !key.starts_with(':') {
            return None;
        }
        if key == keyword {
            return options.get(index + 1);
        }
        index += 2;
    }
    None
}

/// Whether `keyword` appears as a direct element of a `defsystem`'s option
/// plist at all.
///
/// Deliberately looser than [`plist_value`] in three ways, all of them in the
/// fail-*silent* direction, because every caller asks "is this option absent?":
///
/// - **Pair alignment is ignored.** A plist this module refuses to read as
///   pairs still counts as mentioning the option.
/// - **A reader conditional attached to the keyword still counts.**
///   `#-slow :version` is folded by the dialect-aware reader into the *single
///   atom* `"#-slow :version"`, so an equality test on the atom's symbol never
///   sees the keyword. Matching the atom's text as a substring does. Without
///   this, a conditionally-supplied option reads as an absent one — the one
///   direction a "the option is missing" rule must not get wrong.
/// - **Only direct children are examined**, and only *atoms*. The `:version`
///   inside a `:depends-on ((:version "alexandria" "1.0"))` entry is nested
///   inside a list and is therefore never mistaken for the system's own
///   `:version`.
#[must_use]
pub fn plist_mentions(options: &[ExpressionView], keyword: &str) -> bool {
    options.iter().any(|option| {
        is_keyword(option, keyword)
            || atom_text(option).is_some_and(|text| text.to_ascii_lowercase().contains(keyword))
    })
}

// --- definitions ----------------------------------------------------------

/// Whether `view` is a definition form whose name is interned into whatever
/// package is *current* when the file is read.
///
/// Two exclusions, and both matter:
///
/// - **By category.** [`DefinitionCategory::Package`] and
///   [`DefinitionCategory::System`] name a package and a system, neither of
///   which is a symbol interned into the current package in the sense this
///   asks about. `Other`, `UnknownMacro`, `Customization` and `Mode` are
///   excluded because they are either another dialect's forms or a macro whose
///   expansion — and therefore whose interning — is unknown.
/// - **By spelling.** A name written with an explicit package qualifier —
///   `(defun tiny-util:square (x) …)`, `(defmethod asdf:perform …)` — is
///   interned exactly where the prefix says, and `*package*` has nothing to do
///   with it. Counting those would make
///   `crate::defpackage_without_in_package` claim that a correct,
///   fully-qualified single-file library interns its definitions in the wrong
///   package, which is simply false.
#[must_use]
pub fn is_package_interning_definition(view: &ExpressionView) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    let Some(shape) = definition_shape(Dialect::CommonLisp, view, head) else {
        return false;
    };
    if !matches!(
        shape.category,
        DefinitionCategory::Function
            | DefinitionCategory::Macro
            | DefinitionCategory::GenericFunction
            | DefinitionCategory::Method
            | DefinitionCategory::Class
            | DefinitionCategory::Struct
            | DefinitionCategory::Condition
            | DefinitionCategory::Variable
            | DefinitionCategory::Constant
            | DefinitionCategory::Parameter
            | DefinitionCategory::Test
    ) {
        return false;
    }
    // A name this reader cannot extract (`(defstruct (point (:conc-name pt-)) …)`
    // shapes, malformed forms) is left counted rather than dropped: the
    // qualifier exclusion is about names that provably carry one.
    shape.name(view).is_none_or(|name| !name.contains(':'))
}

/// The two forms that declare a Common Lisp package: `defpackage` and ASDF's
/// `uiop:define-package`, which is a superset of it.
pub const PACKAGE_DECLARATION_HEADS: [&str; 2] = ["defpackage", "define-package"];

/// Whether `view` declares a package.
#[must_use]
pub fn is_package_declaration(view: &ExpressionView) -> bool {
    is_paren_list(view)
        && list_head(view).is_some_and(|head| {
            PACKAGE_DECLARATION_HEADS
                .iter()
                .any(|expected| symbol_is(head, expected))
        })
}

/// Runs one rule end to end through the real lint engine and returns the
/// messages it emitted, in report order.
///
/// Every rule here is also tested at the `domain` level, which is where the
/// detection lives — but a domain test cannot catch a wrong
/// [`HeadFilter::Heads`] list. A rule that declares the wrong head compiles,
/// passes every domain test, and is simply **never invoked** by the CLI. This
/// puts the engine's own head index between the test and the rule, so the head
/// list is exercised by the same dispatch the binary uses.
///
/// [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads
#[cfg(test)]
#[must_use]
pub fn run_rule(
    entries: &'static [paredit_core_lint_engine::rule::RuleEntry],
    source: &str,
) -> Vec<String> {
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::RuleCatalog;

    let catalog = RuleCatalog::new(entries);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    collect_lint_outcomes(
        catalog,
        &index,
        std::path::Path::new("app.lisp"),
        Dialect::CommonLisp,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| outcome.into_parts().0.message)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // --- the five quote shapes every rule in this package is pinned against

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(defsystem (foo))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (defsystem (foo)))"), vec!["quote"]);
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(defsystem (foo)))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(defsystem (foo))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(defsystem (foo)))"),
            vec!["defsystem", "foo"]
        );
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(defsystem (foo))\")"), vec!["f"]);
    }

    /// The span-directed lookup must agree with the walk, since one is used by
    /// the standalone reports and the other by the registered rules.
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
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("'(defsystem \"x\")", "defsystem"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(quote (defsystem \"x\"))",
            "defsystem"
        ));
    }

    #[test]
    fn a_span_inside_a_comma_in_a_hard_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "'(a ,(defsystem \"x\"))",
            "defsystem"
        ));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head("(defsystem \"x\")", "defsystem"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "`(a ,(defsystem \"x\"))",
            "defsystem"
        ));
    }

    /// The linear scan `child_containing` replaced, kept as the oracle it is
    /// tested against.
    fn child_containing_linearly(
        view: &ExpressionView,
        target: ByteSpan,
    ) -> Option<&ExpressionView> {
        view.children
            .iter()
            .find(|child| span_contains(child.span, target))
    }

    /// The binary search is only correct if a node's children are ordered and
    /// disjoint. Rather than assert that property directly, this asks for the
    /// same answer as the scan at every level of the descent to every node of a
    /// set of sources chosen for the shapes that could break the ordering —
    /// reader prefixes, reader conditionals, strings, dotted pairs.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for source in [
            "(a (b) (c (d)) e)",
            "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
            "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
            "(defsystem \"x\" #+sbcl :serial t :depends-on (\"a\" \"b\"))",
            "(defpackage :app (:use :cl) (:export #:run))",
        ] {
            let parsed = tree(source);
            let root = parsed.root_view();
            let mut targets = Vec::new();
            paredit_core_syntax::view_query::for_each_subview(&root, |view| {
                targets.push(view.span);
            });
            assert!(targets.len() > 1, "{source} must parse into several nodes");
            for target in targets {
                let mut view: &ExpressionView = &root;
                loop {
                    assert_eq!(
                        child_containing(view, target).map(|child| child.span),
                        child_containing_linearly(view, target).map(|child| child.span),
                        "{source} at {target:?}"
                    );
                    let Some(child) = child_containing_linearly(view, target) else {
                        break;
                    };
                    if child.span == target {
                        break;
                    }
                    view = child;
                }
            }
        }
    }

    /// The cost regression this module's descent exists to avoid:
    /// `is_unevaluated_at` is called once per candidate, and reading
    /// `tree.root_view()` to start the descent made a file of T candidates cost
    /// T×T. The budget is deliberately hundreds of times the linear cost, so
    /// only an asymptotic regression can trip it.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(defsystem \"s{index}\" :depends-on (\"a\"))\n"))
            .collect();
        let parsed = tree(&source);
        let spans: Vec<ByteSpan> = parsed
            .root_view()
            .children
            .iter()
            .map(|child| child.span)
            .collect();
        assert_eq!(spans.len(), 4000);
        let started = std::time::Instant::now();
        for span in spans {
            assert!(!is_unevaluated_at(&parsed, span));
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "4000 lookups took {elapsed:?}; the descent is scanning the top level again"
        );
    }

    // --- ASDF spelling

    #[test]
    fn every_defsystem_spelling_is_recognized() {
        for source in [
            "(defsystem \"x\")",
            "(asdf:defsystem \"x\")",
            "(asdf::defsystem \"x\")",
            "(asdf/parse-defsystem:defsystem \"x\")",
            "(ASDF:DEFSYSTEM \"x\")",
        ] {
            let parsed = tree(source);
            assert!(
                is_defsystem(&parsed.root_view().children[0]),
                "not recognized: {source}"
            );
        }
    }

    #[test]
    fn a_form_that_merely_ends_in_defsystem_is_not_one() {
        let parsed = tree("(mk:mk-defsystem \"x\")");
        assert!(!is_defsystem(&parsed.root_view().children[0]));
    }

    #[test]
    fn a_symbol_designator_folds_case_and_a_string_one_does_not() {
        assert_eq!(asdf_name(":Foo").as_deref(), Some("foo"));
        assert_eq!(asdf_name("#:FOO").as_deref(), Some("foo"));
        assert_eq!(asdf_name("Foo").as_deref(), Some("foo"));
        assert_eq!(asdf_name("\"Foo\"").as_deref(), Some("Foo"));
        assert_eq!(asdf_name("\"foo\"").as_deref(), Some("foo"));
        assert_eq!(asdf_name("\"a\\\"b\"").as_deref(), Some("a\"b"));
        assert_eq!(asdf_name(""), None);
    }

    #[test]
    fn a_defsystem_name_is_read_from_child_one() {
        let parsed = tree("(defsystem \"my-app\" :version \"1.0\")");
        let form = &parsed.root_view().children[0];
        assert_eq!(defsystem_name(form).as_deref(), Some("my-app"));
    }

    // --- plists

    #[test]
    fn a_plist_value_is_read_from_the_pair_position() {
        let parsed = tree("(defsystem \"x\" :description \"d\" :depends-on (\"a\" \"b\"))");
        let form = &parsed.root_view().children[0];
        let options = defsystem_options(form);
        let value = plist_value(options, ":depends-on").expect("the depends-on value");
        assert_eq!(value.children.len(), 2);
        assert!(plist_value(options, ":version").is_none());
    }

    #[test]
    fn a_plist_scan_bails_rather_than_resynchronizing_after_a_desync() {
        // `#+sbcl` occupies a key slot, so `:version`'s pair alignment is
        // unknowable from here. Answering `None` is a missed finding; answering
        // `Some(:version)` would be a wrong one.
        let parsed = tree("(defsystem \"x\" #+sbcl :serial t :version \"1.0\")");
        let form = &parsed.root_view().children[0];
        assert!(plist_value(defsystem_options(form), ":version").is_none());
    }

    #[test]
    fn plist_mentions_ignores_alignment_but_not_nesting() {
        let parsed = tree("(defsystem \"x\" #+sbcl :serial t :version \"1.0\")");
        let form = &parsed.root_view().children[0];
        assert!(plist_mentions(defsystem_options(form), ":version"));

        // A dependency version floor is nested, and is not the system's own.
        let parsed = tree("(defsystem \"x\" :depends-on ((:version \"alexandria\" \"1.0\")))");
        let form = &parsed.root_view().children[0];
        assert!(!plist_mentions(defsystem_options(form), ":version"));
    }

    /// The reader folds `#-slow :version` into a *single atom*, so an equality
    /// test on the keyword never sees it. Confirmed here against the parse
    /// itself, not just the predicate, because the whole guard rests on that
    /// folding being real.
    #[test]
    fn a_reader_conditional_attached_to_a_keyword_folds_into_one_atom_and_still_counts() {
        let parsed = tree("(defsystem \"x\" #-slow :version #-slow \"1.0\")");
        let form = &parsed.root_view().children[0];
        let options = defsystem_options(form);
        assert_eq!(
            atom_text(&options[0]),
            Some("#-slow :version"),
            "the reader no longer folds the conditional into the keyword's atom"
        );
        assert!(plist_mentions(options, ":version"));
    }

    #[test]
    fn a_keyword_named_only_inside_a_nested_list_is_still_not_a_mention() {
        let parsed = tree("(defsystem \"x\" :defsystem-depends-on ((:version \"a\" \"1\")))");
        let form = &parsed.root_view().children[0];
        assert!(!plist_mentions(defsystem_options(form), ":version"));
    }

    /// A symbol read with an explicit package prefix is interned where the
    /// prefix says, not into `*package*`, so it is not one of the definitions
    /// `defpackage-without-in-package` is about.
    #[test]
    fn a_definition_named_with_a_package_qualifier_does_not_intern_into_the_current_package() {
        for source in [
            "(defun tiny-util:square (x) (* x x))",
            "(defvar tiny-util::*calls* 0)",
            "(defmethod asdf:perform ((o load-op) c) nil)",
        ] {
            let parsed = tree(source);
            assert!(
                !is_package_interning_definition(&parsed.root_view().children[0]),
                "wrongly counted the qualified definition: {source}"
            );
        }
    }

    /// A name this reader cannot extract is left counted: the exclusion is for
    /// names that provably carry a qualifier, not for every name it cannot see.
    #[test]
    fn a_definition_whose_name_is_not_a_bare_symbol_is_still_counted() {
        let parsed = tree("(defstruct (point (:conc-name pt-)) x y)");
        assert!(is_package_interning_definition(
            &parsed.root_view().children[0]
        ));
    }

    // --- definitions and package declarations

    #[test]
    fn definition_forms_that_intern_a_symbol_are_recognized() {
        for source in [
            "(defun f () 1)",
            "(defmacro m () 1)",
            "(defgeneric g (x))",
            "(defmethod g ((x t)) 1)",
            "(defclass c () ())",
            "(defstruct s)",
            "(define-condition e (error) ())",
            "(defvar *v* 1)",
            "(defconstant +c+ 1)",
            "(defparameter *p* 1)",
        ] {
            let parsed = tree(source);
            assert!(
                is_package_interning_definition(&parsed.root_view().children[0]),
                "not recognized as a definition: {source}"
            );
        }
    }

    #[test]
    fn defpackage_and_defsystem_are_not_counted_as_interning_definitions() {
        for source in [
            "(defpackage :app (:use :cl))",
            "(uiop:define-package :app (:use :cl))",
            "(asdf:defsystem \"app\")",
            "(in-package :app)",
            "(format t \"hi\")",
        ] {
            let parsed = tree(source);
            assert!(
                !is_package_interning_definition(&parsed.root_view().children[0]),
                "wrongly counted as a definition: {source}"
            );
        }
    }

    #[test]
    fn both_package_declaration_spellings_are_recognized() {
        for source in [
            "(defpackage :app (:use :cl))",
            "(cl:defpackage :app (:use :cl))",
            "(uiop:define-package :app (:use :cl))",
            "(DEFPACKAGE :app)",
        ] {
            let parsed = tree(source);
            assert!(
                is_package_declaration(&parsed.root_view().children[0]),
                "not recognized as a package declaration: {source}"
            );
        }
        let parsed = tree("(in-package :app)");
        assert!(!is_package_declaration(&parsed.root_view().children[0]));
    }

    // --- the byte-scan guard

    #[test]
    fn the_mention_guard_answers_yes_only_for_the_spelling() {
        assert!(mentions("(in-package :app)", "in-package"));
        assert!(mentions("(CL:IN-PACKAGE :APP)", "in-package"));
        assert!(!mentions("(defpackage :app)", "in-package"));
        assert!(!mentions("", "in-package"));
        assert!(!mentions("in-packag", "in-package"));
    }
}
