//! What every rule here shares: which parts of a form are *code*, which forms
//! are test definitions, and where a test definition's body starts.
//!
//! Three things none of these rules can do without, and none of which the
//! engine or `core/syntax` provides:
//!
//! - **Evaluation context.** `'(deftest foo)` is a list of symbols, not a test.
//!   Every rule here anchors on a test-definition head, and the engine's
//!   dispatch walks into quoted data like any other subtree, so a head-matched
//!   node cannot tell on its own whether it is code.
//!   [`for_each_evaluated_subview`] answers that for the *inside* of a test
//!   form, and [`is_unevaluated_at`] answers it for the test form itself.
//!
//! - **Which macro defines a test.** [`TestKind`] is a closed set, per dialect.
//!   `deftest` means two unrelated things (Common Lisp's `rt`-style positional
//!   test and Clojure's `clojure.test` body-of-assertions test) and `defspec`
//!   means a third, so a dialect-blind head table would be wrong for at least
//!   one of them.
//!
//! - **Where the body starts.** `(ert-deftest name () "doc" :tags '(:slow)
//!   BODY…)` and `(def-test name (:suite s) BODY…)` both put non-body children
//!   between the name and the first body form, and both spell that prefix
//!   differently. [`read_test_form`] is the single place that is written down;
//!   it returns `None` rather than guessing whenever a form does not match a
//!   shape it can read.
//!
//! # Quote semantics
//!
//! [`QuoteState`] and [`for_each_evaluated_subview`] are copied from
//! `paredit-feature-lint-condition-system`'s `support.rs`, tests included,
//! deliberately as a copy rather than as a dependency: a lint feature package
//! depending on another lint feature package would be a new feature→feature
//! edge for a hundred lines of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down.
//!
//! # Cost
//!
//! Nothing here runs per visited node, and nothing here is quadratic in the
//! number of test definitions.
//!
//! Five of the six rules declare `HeadFilter::Heads` over the test-definition
//! heads, so all of their work is paid only once a
//! `deftest`/`ert-deftest`/`def-test`/`defspec` has already matched — which, in
//! the `clean/forms/*` benchmarks that lint files with no findings, is never.
//! Each of those five then reads only the matched form's own subtree.
//!
//! `duplicate-test-name` is the exception, because correlating two definitions
//! is not a per-node question: it declares `HeadFilter::WholeTree`, is handed
//! the document once per *file*, and answers by hashing each top-level name
//! through [`for_each_evaluated_root_child`]. A per-definition whole-file scan
//! would make a file of T tests cost T×T, and did — see that rule's own `Cost`
//! section.

use paredit_core_lint_engine::model::NormalizedHead;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, unqualified,
};

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

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
    list_head(view).is_some_and(|head| symbol_in(head, &["quote"]))
}

/// Whether `outer` covers every byte of `inner`. Equal spans contain each
/// other, so a caller that means "strictly inside" compares the spans too.
#[must_use]
pub const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
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

/// Calls `visit` on every *top-level* form of `root` that is evaluated code.
///
/// The depth-1 slice of [`for_each_evaluated_subview`], computed the same way
/// and therefore agreeing with it exactly: `'(deftest …)` is data and is
/// skipped, `` `(a ,(deftest …)) `` is a code node whose *children* are where
/// the escape happens, and a `(quote …)` form is itself code even though
/// everything under it is not.
///
/// One pass over the root's children with no descent, so a rule that correlates
/// top-level definitions costs the number of top-level forms and not the size
/// of the file.
pub fn for_each_evaluated_root_child(
    root: &ExpressionView,
    mut visit: impl FnMut(&ExpressionView),
) {
    let state = QuoteState::EVALUATED.after_prefixes(root);
    if state.is_data() {
        return;
    }
    let inside = if is_quote_form(root) {
        state.quoted()
    } else {
        state
    };
    for child in &root.children {
        if !inside.after_prefixes(child).is_data() {
            visit(child);
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
/// the whole document. [`is_unevaluated_at`] is called once per finding, which
/// made a file of T reported top-level forms cost T×T: on 4000 `deftest` forms
/// that was 34s inside `test-asserts-constant` alone, and no `--rule` or
/// `--exclude` could avoid it, since `inspect lint` runs every rule and filters
/// afterwards.
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
/// data does not settle it: `` `(a ,(deftest x)) `` has a quasiquoted ancestor
/// and an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all. A span inside no
/// top-level form at all — one a caller synthesized rather than took from the
/// tree — is evaluated, because nothing quotes it.
///
/// Every rule here calls this at most once per finding, *after* a
/// test-definition head has already matched — never per visited node.
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

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/// A Clojure metadata annotation (`^:private`, `^{:doc "…"}`, `^String`).
///
/// The reader keeps the annotation and the form it annotates as *siblings*, so
/// `(deftest ^:focus my-test …)` has four children, not three. Every consumer
/// of a test form's shape has to skip these to find the name.
#[must_use]
pub fn is_metadata_marker(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Metadata)
}

/// A string literal, which the reader keeps as one atom including its quotes.
///
/// This is what keeps every rule here out of string contents: `"(is (= 1 1))"`
/// is this atom and has no children, so no walk can reach a form inside it.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(|text| unqualified(text).to_ascii_lowercase())
}

/// Whether an atom is the given symbol, ignoring case and package qualifier.
#[must_use]
pub fn atom_is(view: &ExpressionView, expected: &str) -> bool {
    atom_symbol_text(view).is_some_and(|text| unqualified(text).eq_ignore_ascii_case(expected))
}

/// Whether an atom is any of `expected`.
#[must_use]
pub fn atom_in(view: &ExpressionView, expected: &[&str]) -> bool {
    expected.iter().any(|name| atom_is(view, name))
}

/// Whether a node is a `(head …)` call to one of `heads`.
#[must_use]
pub fn calls_any(view: &ExpressionView, heads: &[&str]) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, heads))
}

// ---------------------------------------------------------------------------
// Test definition forms
// ---------------------------------------------------------------------------

/// Which test-definition macro a form is.
///
/// A closed set on purpose. `deftest` alone spans two frameworks whose body
/// shape and assertion mechanism have nothing in common, so "the head is
/// `deftest`" is not enough to know what a form means — the dialect is part of
/// the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    /// Common Lisp `(deftest NAME FORM &rest EXPECTED-VALUES)` — the `rt` /
    /// `regression-test` shape, whose assertion is the positional comparison
    /// of `FORM`'s values against `EXPECTED-VALUES`, with no assertion *form*
    /// anywhere. Nothing here may conclude "this test asserts nothing" from
    /// one of these.
    CommonLispDeftest,
    /// FiveAM `(def-test NAME (&key depends-on suite fixture compile-at
    /// profile) BODY…)`, whose body is assertion forms.
    ///
    /// FiveAM's other definition macro, `test`, is deliberately **not** modelled:
    /// `(test …)` is too generic a head to assume belongs to a test framework.
    /// It expands to `def-test`, so the two are the same form — this package
    /// simply declines to guess which `(test …)` calls are which.
    CommonLispDefTest,
    /// lisp-unit `(define-test NAME BODY…)`, whose body is `assert-*` forms.
    CommonLispDefineTest,
    /// ERT `(ert-deftest NAME () [DOC] [:tags …] [:expected-result …] BODY…)`.
    ErtDeftest,
    /// `clojure.test` `(deftest NAME BODY…)`.
    ClojureDeftest,
    /// `clojure.test.check.clojure-test` `(defspec NAME [TIMES] PROPERTY)`,
    /// whose "body" is a generated property, not assertion forms.
    ClojureDefspec,
}

impl TestKind {
    /// Whether this form's body is a sequence of assertion forms, as opposed
    /// to a positional expected-value list or a generated property.
    ///
    /// The rules that ask "does this body contain an assertion?" are only
    /// meaningful for the kinds that answer `true`.
    #[must_use]
    pub const fn body_is_assertions(self) -> bool {
        matches!(
            self,
            Self::CommonLispDefTest
                | Self::CommonLispDefineTest
                | Self::ErtDeftest
                | Self::ClojureDeftest
        )
    }
}

/// Every head a rule in this package anchors on, as `HeadFilter::Heads` wants
/// them: normalized, so the dispatcher's head index and the rules agree.
///
/// This is the *union* over dialects; [`test_kind`] then rejects the pairings
/// that do not exist (`defspec` in Common Lisp, `def-test` in Clojure), so a
/// wider index costs a head comparison and never a finding.
pub const TEST_DEFINITION_HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("def-test"),
    NormalizedHead::new("define-test"),
    NormalizedHead::new("defspec"),
    NormalizedHead::new("deftest"),
    NormalizedHead::new("ert-deftest"),
];

/// The dialects any rule in this package models. A rule that models fewer says
/// so itself.
pub const TEST_DIALECTS: [Dialect; 3] = [Dialect::CommonLisp, Dialect::EmacsLisp, Dialect::Clojure];

/// Which test macro `head` names in `dialect`, or `None`.
///
/// Deliberately not a flat table: `deftest` is `rt`'s positional test in Common
/// Lisp and `clojure.test`'s body-of-assertions test in Clojure, and treating
/// them as one form is the mistake that would make `test-without-assertion`
/// fire on every `rt` test in a file.
#[must_use]
pub fn test_kind(head: &str, dialect: Dialect) -> Option<TestKind> {
    let name = unqualified(head);
    match dialect {
        Dialect::CommonLisp => {
            if name.eq_ignore_ascii_case("deftest") {
                Some(TestKind::CommonLispDeftest)
            } else if name.eq_ignore_ascii_case("def-test") {
                Some(TestKind::CommonLispDefTest)
            } else if name.eq_ignore_ascii_case("define-test") {
                Some(TestKind::CommonLispDefineTest)
            } else {
                None
            }
        }
        // `ert-deftest` only. `deftest` in an Emacs Lisp file is some local
        // macro this package knows nothing about, and ERT itself spells no
        // other definition form.
        Dialect::EmacsLisp => name
            .eq_ignore_ascii_case("ert-deftest")
            .then_some(TestKind::ErtDeftest),
        Dialect::Clojure => {
            if name.eq_ignore_ascii_case("deftest") {
                Some(TestKind::ClojureDeftest)
            } else if name.eq_ignore_ascii_case("defspec") {
                Some(TestKind::ClojureDefspec)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// One test definition, read far enough to say where its name and body are.
#[derive(Debug)]
pub struct TestForm<'a> {
    pub kind: TestKind,
    /// The node naming the test.
    pub name: &'a ExpressionView,
    /// The children that make up the body, which may be empty.
    pub body: &'a [ExpressionView],
}

impl TestForm<'_> {
    /// The test's name in the spelling comparisons here are written in, or
    /// `None` when the name is not a plain symbol (a `(name :suite s)` list, a
    /// computed name, a macro parameter).
    #[must_use]
    pub fn name_text(&self) -> Option<String> {
        normalized_symbol(self.name)
    }
}

/// Reads one test-definition form, or `None` when it is not one, or is one
/// whose shape this module does not claim to understand.
///
/// `None` is the answer for every uncertainty, because every rule here treats
/// "cannot read it" as "say nothing":
///
/// - not a `(…)` list, or a head that is not a test macro in `dialect`
/// - a name that is not an atom (a `(name :suite s)` designator list, or a
///   macro-generated name), since a rule cannot then say which test it is
/// - an `ert-deftest` with no argument list, which is not an `ert-deftest`
///   this module can locate a body in
#[must_use]
pub fn read_test_form<'a>(view: &'a ExpressionView, dialect: Dialect) -> Option<TestForm<'a>> {
    let head = list_head(view)?;
    let kind = test_kind(head, dialect)?;
    let children = &view.children;

    // Clojure writes `(deftest ^:focus my-test …)`, where the annotation is a
    // sibling of the name rather than part of it.
    let mut index = 1;
    while children.get(index).is_some_and(is_metadata_marker) {
        index += 1;
    }
    let name = children.get(index)?;
    if atom_symbol_text(name).is_none() || is_string_literal(name) {
        return None;
    }
    index += 1;

    let body_start = match kind {
        // `(ert-deftest NAME () [DOC] [:key VALUE]… BODY…)`.
        TestKind::ErtDeftest => {
            let arglist = children.get(index)?;
            if arglist.kind != paredit_core_syntax::sexpr::ExpressionKind::List {
                return None;
            }
            index += 1;
            // A docstring, but only when something follows it: a trailing
            // string is read as the body, so an `(ert-deftest f () "doc")` is
            // never reported as having an empty body.
            if children.get(index).is_some_and(is_string_literal) && children.len() > index + 1 {
                index += 1;
            }
            while children.get(index).is_some_and(is_keyword_atom) && children.len() > index + 1 {
                index += 2;
            }
            index
        }
        // `(def-test NAME (&key …) BODY…)`. The option list is optional, and
        // `(def-test f (is (= 1 1)))` is a body, so the second child counts as
        // options only when it is empty or starts with a keyword — the two
        // shapes an option list can have and an assertion call cannot.
        TestKind::CommonLispDefTest => {
            if children
                .get(index)
                .is_some_and(|child| is_paren_list(child) && is_option_list(child))
            {
                index += 1;
            }
            index
        }
        // `rt`'s `(deftest NAME [:key VALUE]* FORM &rest EXPECTED-VALUES)`.
        // The MIT/pfdietz `rt.lisp` pops leading keyword properties before it
        // pops the form, so they are not part of what runs.
        TestKind::CommonLispDeftest => {
            while children.get(index).is_some_and(is_keyword_atom) && children.len() > index + 1 {
                index += 2;
            }
            index
        }
        TestKind::CommonLispDefineTest | TestKind::ClojureDeftest | TestKind::ClojureDefspec => {
            index
        }
    };

    Some(TestForm {
        kind,
        name,
        body: children.get(body_start..).unwrap_or_default(),
    })
}

/// A `:keyword` atom, which is how both ERT options and FiveAM option lists
/// start.
#[must_use]
pub fn is_keyword_atom(view: &ExpressionView) -> bool {
    atom_symbol_text(view).is_some_and(|text| text.starts_with(':') && text.len() > 1)
}

/// Whether a `(…)` list is an option list rather than a body form: empty, or
/// beginning with a keyword. A call can be neither.
fn is_option_list(view: &ExpressionView) -> bool {
    view.children.first().is_none_or(is_keyword_atom)
}

// ---------------------------------------------------------------------------
// Assertion vocabulary
// ---------------------------------------------------------------------------

/// The assertion forms this package recognizes for `dialect`.
///
/// Every name here was read out of the framework's own source (FiveAM
/// `src/check.lisp`, lisp-unit `lisp-unit.lisp`, Emacs `lisp/emacs-lisp/ert.el`,
/// Clojure `src/clj/clojure/test.clj`), not recalled.
///
/// Only used to answer "is there an assertion in here?", never "is this call an
/// assertion I should inspect". That direction matters: a name missing from
/// this list makes a rule *quieter* (it concludes it cannot tell), never
/// louder. Which is why the general runtime assertions `cl:assert` and
/// `cl-assert` appear here despite not being test-framework forms — a body
/// containing one is a body whose author was asserting something, and that is
/// enough to stay silent about it.
#[must_use]
pub const fn assertion_heads(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &[
            // FiveAM, src/check.lisp.
            "is",
            "is-true",
            "is-false",
            "is-every",
            "signals",
            "finishes",
            "pass",
            "fail",
            "skip",
            // lisp-unit, exported assertion set.
            "assert-eq",
            "assert-eql",
            "assert-equal",
            "assert-equalp",
            "assert-equality",
            "assert-prints",
            "assert-expands",
            "assert-true",
            "assert-test",
            "assert-false",
            "assert-nil",
            "assert-error",
            // CLHS, not a test framework — see above.
            "assert",
        ],
        Dialect::EmacsLisp => &[
            // ert.el: the three `should` macros, plus the outcome functions.
            "should",
            "should-not",
            "should-error",
            "ert-fail",
            "ert-skip",
            // cl-lib, not a test framework — see above.
            "cl-assert",
        ],
        // clojure.test's two assertion macros, plus clojure.core/assert.
        Dialect::Clojure => &["is", "are", "assert"],
        _ => &[],
    }
}

/// The assertion forms whose *single* argument is a boolean expression, which
/// is the only shape `test-asserts-constant` inspects.
///
/// Narrower than [`assertion_heads`] in both directions, because this list is
/// used to *report* rather than to stay silent:
///
/// - `cl:assert` and `cl-assert` are excluded. They are general defensive
///   assertions whose CLHS shape is `(assert test [(place*) [datum …]])`, and
///   reporting on one would take this package outside test forms in spirit.
/// - Only the unary forms are listed. `assert-equal` takes an expected value
///   *and* a form, so its first argument being a literal is the normal case,
///   not a defect.
#[must_use]
pub const fn boolean_assertion_heads(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["is", "is-true", "assert-true"],
        Dialect::EmacsLisp => &["should"],
        Dialect::Clojure => &["is"],
        _ => &[],
    }
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

/// Whether an atom is the dialect's literal for truth.
///
/// Per dialect on purpose: `t` is a symbol like any other in Clojure, and
/// `true` is one in Common Lisp and Emacs Lisp.
#[must_use]
pub fn is_true_literal(view: &ExpressionView, dialect: Dialect) -> bool {
    match dialect {
        Dialect::CommonLisp | Dialect::EmacsLisp => atom_is(view, "t"),
        Dialect::Clojure => atom_is(view, "true"),
        _ => false,
    }
}

/// Whether an atom is the dialect's literal for falsity.
#[must_use]
pub fn is_false_literal(view: &ExpressionView, dialect: Dialect) -> bool {
    match dialect {
        Dialect::CommonLisp | Dialect::EmacsLisp => atom_is(view, "nil"),
        Dialect::Clojure => atom_in(view, &["false", "nil"]),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// In-place skip markers
// ---------------------------------------------------------------------------

/// Clojure metadata keys a runner reads as "do not run this test".
///
/// Both are Kaocha's, and both were read out of Kaocha itself rather than
/// recalled: `:kaocha/skip` is the shipped default of
/// `:kaocha.filter/skip-meta` in `resources/kaocha/default_config.edn`, and
/// `:kaocha/pending` is hardcoded in `kaocha.testable/run-testable`.
///
/// Nothing else is listed because nothing else is a *framework* spelling.
/// `lein test`'s selectors and Kaocha's `--skip-meta`/`--focus-meta` both take
/// user-chosen keywords with no defaults, eftest recognises only
/// `^:eftest/synchronized` and `^:eftest/slow`, and Midje's fact metadata is
/// entirely user-defined — so `^:skip`, `^:pending` and `^:focus` mean whatever
/// a given `project.clj` says they mean, including nothing.
pub const CLOJURE_SKIP_METADATA: [&str; 2] = [":kaocha/skip", ":kaocha/pending"];

/// The Clojure skip/pending metadata a test definition carries, if any.
///
/// Read from the test form's own children, because the reader keeps `^:x` and
/// the form it annotates as siblings. Two rules need it for opposite reasons:
/// `disabled-test-left-in` reports it, and `empty-test-body` stays silent when
/// it is present — a `^:kaocha/pending` test with no body is a placeholder
/// someone wrote on purpose, not a mistake.
#[must_use]
pub fn clojure_skip_metadata(view: &ExpressionView, dialect: Dialect) -> Option<&ExpressionView> {
    if dialect != Dialect::Clojure {
        return None;
    }
    view.children
        .iter()
        .filter(|child| is_metadata_marker(child))
        .find(|child| atom_in(child, &CLOJURE_SKIP_METADATA))
}

/// An unconditional in-place skip, as a direct body form of a test.
///
/// Returns the marker's name for the message, or `None` — and `None` is the
/// answer for every *conditional* skip, which is the whole point. A
/// `(skip-unless (executable-find "git"))` is a test that runs wherever it can,
/// not a test someone disabled; only a skip whose condition is a literal, or
/// which takes no condition at all, is one that can never run.
///
/// Spellings verified in the frameworks' own source: FiveAM's `skip` is
/// `(defmacro skip (&rest reason))` in `src/check.lisp`; ERT's `ert-skip` is a
/// global function in `ert.el`, while `skip-unless` and `skip-when` are
/// `cl-macrolet` bindings that `ert-deftest` injects around the body — which is
/// why they are recognised only *inside* a test form and nowhere else.
/// `skip-when` is Emacs 30 and later.
#[must_use]
pub fn unconditional_skip(view: &ExpressionView, dialect: Dialect) -> Option<&'static str> {
    let head = list_head(view)?;
    let argument = view.children.get(1);
    match dialect {
        // FiveAM `(skip "reason")`, which records a skipped result outright.
        Dialect::CommonLisp => symbol_in(head, &["skip"]).then_some("skip"),
        Dialect::EmacsLisp => {
            if symbol_in(head, &["ert-skip"]) {
                return Some("ert-skip");
            }
            if symbol_in(head, &["skip-unless"])
                && argument.is_some_and(|value| is_false_literal(value, dialect))
            {
                return Some("skip-unless nil");
            }
            if symbol_in(head, &["skip-when"])
                && argument.is_some_and(|value| is_true_literal(value, dialect))
            {
                return Some("skip-when t");
            }
            None
        }
        _ => None,
    }
}

/// Whether the test form's subtree contains a call to any assertion form.
///
/// Walks evaluated code only, so an `(is …)` inside `'(…)` does not count as
/// the test's assertion.
#[must_use]
pub fn contains_assertion(body: &[ExpressionView], dialect: Dialect) -> bool {
    let heads = assertion_heads(dialect);
    let mut found = false;
    for form in body {
        for_each_evaluated_subview(form, |view| {
            if calls_any(view, heads) {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::list_head;

    fn tree(source: &str, dialect: Dialect) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, dialect).expect("parse")
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source, Dialect::CommonLisp);
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    // -- the five quote shapes every rule here depends on --------------------

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(deftest (is x))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (deftest (is x)))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(deftest (is x))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(deftest (is x)))"),
            vec!["deftest", "is"]
        );
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a
    /// hard quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(deftest (is x)))").is_empty());
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner
    /// node carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(evaluated_heads("'(outer (inner))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(deftest (is x))\")"), vec!["f"]);
    }

    fn data_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source, Dialect::CommonLisp);
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
    fn a_span_inside_a_quote_reads_as_data() {
        assert!(data_at_first_head("'(deftest foo (is x))", "deftest"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_data() {
        assert!(data_at_first_head(
            "(quote (deftest foo (is x)))",
            "deftest"
        ));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!data_at_first_head("(deftest foo (is x))", "deftest"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!data_at_first_head("`(a ,(deftest foo (is x)))", "deftest"));
    }

    #[test]
    fn a_span_under_a_comma_in_a_hard_quote_reads_as_data() {
        assert!(data_at_first_head("'(a ,(deftest foo (is x)))", "deftest"));
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
    /// reader prefixes, Clojure metadata siblings, strings, dotted pairs.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for (source, dialect) in [
            ("(a (b) (c (d)) e)", Dialect::CommonLisp),
            (
                "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
                Dialect::CommonLisp,
            ),
            (
                "(deftest ^:focus ^{:doc \"d\"} t1 (is true))\n(deftest t2 (is false))",
                Dialect::Clojure,
            ),
            (
                "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
                Dialect::CommonLisp,
            ),
            (
                "(ert-deftest t () \"doc\" :tags '(:slow) (should t))",
                Dialect::EmacsLisp,
            ),
        ] {
            let parsed = tree(source, dialect);
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

    /// The cost regression for the descent, next to `duplicate-test-name`'s for
    /// the correlation: `is_unevaluated_at` is called once per finding, and its
    /// scan of the root's children made a file of T reported top-level forms
    /// cost T×T. The budget is deliberately hundreds of times the linear cost,
    /// so only an asymptotic regression can trip it.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(deftest n{index} (is (= 1 1)))\n"))
            .collect();
        let parsed = tree(&source, Dialect::CommonLisp);
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

    // -- head classification -------------------------------------------------

    #[test]
    fn deftest_means_a_different_thing_in_each_dialect() {
        assert_eq!(
            test_kind("deftest", Dialect::CommonLisp),
            Some(TestKind::CommonLispDeftest)
        );
        assert_eq!(
            test_kind("deftest", Dialect::Clojure),
            Some(TestKind::ClojureDeftest)
        );
        assert_eq!(test_kind("deftest", Dialect::EmacsLisp), None);
    }

    #[test]
    fn a_package_qualified_common_lisp_head_still_reads_as_a_test() {
        assert_eq!(
            test_kind("rt:deftest", Dialect::CommonLisp),
            Some(TestKind::CommonLispDeftest)
        );
        assert_eq!(
            test_kind("5AM:DEF-TEST", Dialect::CommonLisp),
            Some(TestKind::CommonLispDefTest)
        );
    }

    #[test]
    fn a_head_that_belongs_to_another_dialect_is_not_a_test_here() {
        assert_eq!(test_kind("defspec", Dialect::CommonLisp), None);
        assert_eq!(test_kind("def-test", Dialect::Clojure), None);
        assert_eq!(test_kind("ert-deftest", Dialect::Clojure), None);
        assert_eq!(test_kind("deftest", Dialect::Scheme), None);
    }

    #[test]
    fn the_rt_shape_is_marked_as_not_being_a_body_of_assertions() {
        assert!(!TestKind::CommonLispDeftest.body_is_assertions());
        assert!(!TestKind::ClojureDefspec.body_is_assertions());
        assert!(TestKind::CommonLispDefTest.body_is_assertions());
        assert!(TestKind::CommonLispDefineTest.body_is_assertions());
        assert!(TestKind::ErtDeftest.body_is_assertions());
        assert!(TestKind::ClojureDeftest.body_is_assertions());
    }

    #[test]
    fn lisp_units_define_test_is_a_common_lisp_test() {
        assert_eq!(
            test_kind("define-test", Dialect::CommonLisp),
            Some(TestKind::CommonLispDefineTest)
        );
    }

    /// FiveAM has no `deftest` and no `test` is modelled, so a Common Lisp
    /// `deftest` is `rt`'s and nothing else.
    #[test]
    fn five_ams_generic_test_head_is_not_modelled() {
        assert_eq!(test_kind("test", Dialect::CommonLisp), None);
    }

    // -- body location -------------------------------------------------------

    fn read(source: &str, dialect: Dialect) -> Option<(String, Vec<String>)> {
        let parsed = tree(source, dialect);
        let root = parsed.root_view();
        let form = read_test_form(&root.children[0], dialect)?;
        Some((
            form.name_text().unwrap_or_default(),
            form.body
                .iter()
                .map(|child| child.span.slice(source).to_owned())
                .collect(),
        ))
    }

    #[test]
    fn an_ert_test_body_starts_after_the_arglist() {
        let (name, body) = read("(ert-deftest my-test () (should t))", Dialect::EmacsLisp)
            .expect("a readable ert test");
        assert_eq!(name, "my-test");
        assert_eq!(body, vec!["(should t)"]);
    }

    #[test]
    fn an_ert_docstring_and_keyword_options_are_not_body_forms() {
        let (_, body) = read(
            "(ert-deftest my-test () \"doc\" :tags '(:slow) :expected-result :failed (should t))",
            Dialect::EmacsLisp,
        )
        .expect("a readable ert test");
        assert_eq!(body, vec!["(should t)"]);
    }

    /// A trailing string is read as the body, so `empty-test-body` cannot
    /// report a test whose only child is a docstring.
    #[test]
    fn a_trailing_ert_string_is_read_as_the_body_not_a_docstring() {
        let (_, body) = read("(ert-deftest my-test () \"doc\")", Dialect::EmacsLisp)
            .expect("a readable ert test");
        assert_eq!(body, vec!["\"doc\""]);
    }

    #[test]
    fn an_ert_test_without_an_arglist_is_not_read_at_all() {
        assert!(read("(ert-deftest my-test)", Dialect::EmacsLisp).is_none());
    }

    #[test]
    fn a_five_am_option_list_is_not_a_body_form() {
        let (name, body) = read(
            "(def-test my-test (:suite outer) (is (= 1 2)))",
            Dialect::CommonLisp,
        )
        .expect("a readable def-test");
        assert_eq!(name, "my-test");
        assert_eq!(body, vec!["(is (= 1 2))"]);
    }

    #[test]
    fn an_empty_five_am_option_list_is_not_a_body_form() {
        let (_, body) =
            read("(def-test my-test () (is t))", Dialect::CommonLisp).expect("a readable def-test");
        assert_eq!(body, vec!["(is t)"]);
    }

    /// The disambiguation that keeps a first body form from being eaten as an
    /// option list: an assertion call starts with a symbol, not a keyword.
    #[test]
    fn a_def_test_whose_first_body_form_is_a_call_keeps_it() {
        let (_, body) = read("(def-test my-test (is (= 1 2)))", Dialect::CommonLisp)
            .expect("a readable def-test");
        assert_eq!(body, vec!["(is (= 1 2))"]);
    }

    #[test]
    fn clojure_metadata_sits_between_the_head_and_the_name() {
        let (name, body) =
            read("(deftest ^:focus my-test (is true))", Dialect::Clojure).expect("a readable test");
        assert_eq!(name, "my-test");
        assert_eq!(body, vec!["(is true)"]);
    }

    #[test]
    fn a_name_that_is_not_a_symbol_is_not_read() {
        assert!(read("(deftest (my-test :suite s) (is t))", Dialect::CommonLisp).is_none());
        assert!(read("(deftest \"my test\" (is t))", Dialect::Clojure).is_none());
    }

    // -- assertion detection -------------------------------------------------

    fn body_of(source: &str, dialect: Dialect) -> (SyntaxTree, Dialect) {
        (tree(source, dialect), dialect)
    }

    fn asserts(source: &str, dialect: Dialect) -> bool {
        let (parsed, dialect) = body_of(source, dialect);
        let root = parsed.root_view();
        let form = read_test_form(&root.children[0], dialect).expect("a readable test");
        contains_assertion(form.body, dialect)
    }

    #[test]
    fn an_assertion_anywhere_in_the_body_counts() {
        assert!(asserts(
            "(deftest t1 (let [x 1] (testing \"x\" (is (= x 1)))))",
            Dialect::Clojure
        ));
        assert!(asserts(
            "(ert-deftest t1 () (dolist (x '(1 2)) (should x)))",
            Dialect::EmacsLisp
        ));
    }

    #[test]
    fn an_assertion_inside_quoted_data_does_not_count() {
        assert!(!asserts(
            "(ert-deftest t1 () (setq forms '(should t)))",
            Dialect::EmacsLisp
        ));
    }

    #[test]
    fn an_assertion_spelled_only_inside_a_string_does_not_count() {
        assert!(!asserts(
            "(ert-deftest t1 () (setq doc \"(should t)\"))",
            Dialect::EmacsLisp
        ));
    }

    #[test]
    fn a_body_with_no_assertion_says_so() {
        assert!(!asserts("(deftest t1 (println \"hi\"))", Dialect::Clojure));
    }

    // -- rt keyword properties ----------------------------------------------

    #[test]
    fn rt_keyword_properties_are_not_body_forms() {
        let (name, body) = read(
            "(deftest my-test :compile-at :run-time (+ 1 2) 3)",
            Dialect::CommonLisp,
        )
        .expect("a readable rt test");
        assert_eq!(name, "my-test");
        assert_eq!(body, vec!["(+ 1 2)", "3"]);
    }

    // -- literals ------------------------------------------------------------

    fn first_child(source: &str, dialect: Dialect) -> (SyntaxTree, Dialect) {
        (tree(source, dialect), dialect)
    }

    fn literal_is_true(source: &str, dialect: Dialect) -> bool {
        let (parsed, dialect) = first_child(source, dialect);
        is_true_literal(&parsed.root_view().children[0], dialect)
    }

    #[test]
    fn truth_is_spelled_per_dialect() {
        assert!(literal_is_true("t", Dialect::CommonLisp));
        assert!(literal_is_true("t", Dialect::EmacsLisp));
        assert!(!literal_is_true("t", Dialect::Clojure));
        assert!(literal_is_true("true", Dialect::Clojure));
        assert!(!literal_is_true("true", Dialect::CommonLisp));
    }

    // -- skip markers --------------------------------------------------------

    fn skip_of(source: &str, dialect: Dialect) -> Option<&'static str> {
        let (parsed, dialect) = first_child(source, dialect);
        unconditional_skip(&parsed.root_view().children[0], dialect)
    }

    #[test]
    fn an_unconditional_skip_is_recognized_per_dialect() {
        assert_eq!(
            skip_of("(skip \"flaky\")", Dialect::CommonLisp),
            Some("skip")
        );
        assert_eq!(
            skip_of("(ert-skip \"flaky\")", Dialect::EmacsLisp),
            Some("ert-skip")
        );
        assert_eq!(
            skip_of("(skip-unless nil)", Dialect::EmacsLisp),
            Some("skip-unless nil")
        );
        assert_eq!(
            skip_of("(skip-when t)", Dialect::EmacsLisp),
            Some("skip-when t")
        );
    }

    /// The guard that keeps a portability skip from being read as a disabled
    /// test.
    #[test]
    fn a_conditional_skip_is_not_an_unconditional_one() {
        assert_eq!(
            skip_of(
                "(skip-unless (executable-find \"git\"))",
                Dialect::EmacsLisp
            ),
            None
        );
        assert_eq!(
            skip_of(
                "(skip-when (eq system-type 'windows-nt))",
                Dialect::EmacsLisp
            ),
            None
        );
        // Inverted literals: `(skip-unless t)` always runs, `(skip-when nil)`
        // always runs. Neither disables anything.
        assert_eq!(skip_of("(skip-unless t)", Dialect::EmacsLisp), None);
        assert_eq!(skip_of("(skip-when nil)", Dialect::EmacsLisp), None);
    }

    /// `skip-unless` is a `cl-macrolet` binding ERT injects around a test body,
    /// so it means nothing in another dialect.
    #[test]
    fn a_skip_spelling_from_another_dialect_is_not_recognized() {
        assert_eq!(skip_of("(skip-unless nil)", Dialect::CommonLisp), None);
        assert_eq!(skip_of("(skip \"x\")", Dialect::EmacsLisp), None);
        assert_eq!(skip_of("(skip \"x\")", Dialect::Clojure), None);
    }

    /// The metadata reader has to see past the `^`, or every marker comparison
    /// silently fails.
    #[test]
    fn a_metadata_marker_reads_as_its_keyword_without_the_caret() {
        let parsed = tree(
            "(deftest ^:kaocha/skip my-test (is true))",
            Dialect::Clojure,
        );
        let form = &parsed.root_view().children[0];
        let marker = &form.children[1];
        assert!(is_metadata_marker(marker));
        assert_eq!(atom_symbol_text(marker), Some(":kaocha/skip"));
        assert!(atom_in(marker, &CLOJURE_SKIP_METADATA));
    }
}
