//! What the documentation rules share: which parts of a file are *code*, where
//! a definition keeps its docstring, and how to read a comment's prose.
//!
//! # Two data sources, not one
//!
//! This package's rules split cleanly in half, and the halves reach their
//! subject by different routes:
//!
//! - A **docstring is a node** — a string literal sitting in a definition's
//!   body or in a fixed slot. [`docstring_of`] reads it from the
//!   [`ExpressionView`] the dispatcher already handed the rule, so those rules
//!   are `HeadFilter::Heads` and never look at the file as a whole.
//! - A **comment is not a node.** The parser keeps comments in a list beside
//!   the tree ([`SyntaxTree::comments`]), by design, so a rule that walks
//!   `ExpressionView` children cannot see one at all. A rule whose subject is a
//!   comment has to read that list, and a rule whose subject is *only* comments
//!   therefore has nothing to anchor a head filter on.
//!
//! # Evaluation context
//!
//! The quote machinery below ([`QuoteState`], [`is_unevaluated_at`]) is a
//! deliberate copy of `paredit-feature-lint-build-system`'s `support.rs`, not a
//! new design and not a cross-package dependency. Two independent counters are
//! required because `'` and `` ` `` are not the same thing:
//!
//! - a comma inside `'(…)` is a comma *character* in a literal list, so `hard`
//!   never clears — a single `i32` depth counter gets `'(a ,X)` wrong;
//! - a comma inside `` `(…) `` escapes back to code, so `quasi` counts up and
//!   down.
//!
//! A node one level *inside* a quote is still data, so a node-local
//! `reader_prefixes` check is not enough either: the state has to be
//! reconstructed by descending to the node ([`is_unevaluated_at`]).
//!
//! This matters more here than for most rule families. A macro that writes
//! definitions carries a `` `(defun ,name (,@args) ,docstring …) `` template,
//! and a docstring rule that judged the template would be complaining about a
//! docstring that is *spliced in at expansion time* and is not there to read.
//!
//! # Cost
//!
//! Nothing here is called per visited node. The `clean/forms/*` benchmarks lint
//! files with zero findings, so the per-file cost of a rule that matches
//! nothing is exactly what they measure.
//!
//! [`is_unevaluated_at`] is called once per *confirmed candidate* — after a
//! rule has already decided it would report — so it is allowed to cost the
//! enclosing top-level form and never the file. It binary-searches the top
//! level for the one root child containing the span and materializes only that
//! form. Starting it from `tree.root_view()` instead — which builds an
//! `ExpressionView` for every node in the document — is what made a file of T
//! candidates cost T×T in a sibling package, and no `--rule` or `--exclude`
//! could avoid it, since `inspect lint` runs every rule and filters afterwards.
//!
//! No rule in this package touches
//! `RuleContext::binding_table`/`value_table`/`type_table`, or
//! `RuleContext::scratch_cache`.

use paredit_core_syntax::definition::DefinitionShape;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, ReaderPrefix, SourceComment, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, symbol_is};

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
/// the whole document.
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
/// data does not settle it: `` `(a ,(defun f (x) "doc" x)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// A span inside no top-level form at all — one a caller synthesized rather
/// than took from the tree — is evaluated, because nothing quotes it.
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

// --- docstrings -----------------------------------------------------------

/// Whether `view` is a string literal.
///
/// The opening quote is enough to tell: the reader only produces an atom
/// beginning with `"` for a string, and it is always closed (an unterminated
/// string is a parse error, not an atom).
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && atom_text(view).is_some_and(|text| text.starts_with('"') && text.len() >= 2)
}

/// Whether any *direct* child of `view` is a string literal whose raw source
/// satisfies `predicate`.
///
/// The pre-filter both docstring rules run before anything else. Every
/// docstring position this package reads — a body head, or a fixed child index
/// — is a direct child of the definition, so a definition with no qualifying
/// child literal has no qualifying docstring, whatever
/// [`crate::support::docstring_view_of`] would have picked. Reading the raw
/// literal keeps it allocation-free: no `DefinitionShape` is built, no
/// docstring is unescaped, and no name is owned.
///
/// This exists because both rules anchor on `defun`, so on a file of ordinary
/// definitions they are invoked once per definition, and *everything* they did
/// before reaching their real test was per-definition cost paid to conclude
/// nothing. `clean/forms/*` is exactly that file.
#[must_use]
pub fn has_child_string_literal(view: &ExpressionView, predicate: impl Fn(&str) -> bool) -> bool {
    view.children
        .iter()
        .any(|child| is_string_literal(child) && atom_text(child).is_some_and(&predicate))
}

/// The contents of a string literal, delimiters removed and the handful of
/// escapes Common Lisp has (`\"` and `\\`) resolved.
///
/// `None` for anything that is not a string literal. The escapes are resolved
/// because a rule measuring a docstring's width is measuring what a reader
/// sees, and `\"` is one character to them and two in the source.
#[must_use]
pub fn string_literal_text(view: &ExpressionView) -> Option<String> {
    if !is_string_literal(view) {
        return None;
    }
    let text = atom_text(view)?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
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
    Some(result)
}

/// Where a defining form keeps its docstring, for the forms this package reads.
///
/// A deliberate mirror of `paredit-feature-lint-convention`'s
/// `missing_docstring::DocstringPlace`. Two shapes only, because these are the
/// two whose docstring is a plain string literal a width or an example can be
/// read out of; a `(:documentation "…")` option is read by
/// [`documentation_option`] instead, and `defstruct`'s slot-or-docstring
/// position is deliberately not read at all (see [`docstring_of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocstringPlace {
    /// At the head of the body, after the lambda list: `defun`, `defmacro`,
    /// `defmethod`.
    BodyHead,
    /// In a fixed slot: `(defvar name value "doc")`.
    Fixed(usize),
}

/// The docstring of one definition, or `None`.
///
/// Two guards, and both are load-bearing:
///
/// - **A lone string body is a return value, not a docstring.**
///   `(defun greeting () "hello")` returns a greeting; reading it as
///   documentation would be wrong in both directions. This is exactly the
///   guard `missing-docstring` documents, and it is repeated here rather than
///   depended on because a rule that measured `"hello"` as a summary line
///   would be measuring a *value*.
/// - **The reader prefix is not stripped.** `#.(compute-doc)` is not a string
///   literal and produces `None`, rather than a docstring nobody wrote.
///
/// `defstruct` is not read. Its docstring position (`(defstruct name "doc"
/// slot…)`) collides with a slot name, and telling the two apart needs the
/// whole slot grammar — a false positive there would report a *slot* as an
/// over-long summary line.
///
/// The *node* is returned rather than its text, so a caller can report at the
/// docstring itself rather than at the whole definition.
#[must_use]
pub fn docstring_view_of(
    shape: DefinitionShape,
    place: DocstringPlace,
    view: &ExpressionView,
) -> Option<&ExpressionView> {
    let candidate = match place {
        DocstringPlace::BodyHead => {
            let body = shape.body_forms(view);
            // A lone string is the return value, not documentation.
            if body.len() < 2 {
                return None;
            }
            body.first()?
        }
        DocstringPlace::Fixed(index) => view.children.get(index)?,
    };
    is_string_literal(candidate).then_some(candidate)
}

/// Where a head this package anchors on keeps its docstring, or `None` for a
/// head with no docstring position this package reads.
#[must_use]
pub fn docstring_place(head: &str) -> Option<DocstringPlace> {
    if symbol_in(head, &["defun", "defmacro", "defmethod"]) {
        return Some(DocstringPlace::BodyHead);
    }
    if symbol_in(head, &["defvar", "defparameter", "defconstant"]) {
        // `(defvar name value "doc")`
        return Some(DocstringPlace::Fixed(3));
    }
    None
}

/// The non-empty text of a `(:documentation "…")` option, when the form carries
/// one.
///
/// The keyword must *head* its option, which is what makes this the form's own
/// documentation rather than some other option's second element. Without that
/// test the scan returns the second element of whichever option comes first —
/// `:cl` from a `(:use :cl)`, say — and a documented package then reads as
/// undocumented. A `:documentation` appearing anywhere but in head position
/// (an imported or shadowed symbol of that name, a nested option) is somebody
/// else's.
///
/// An earlier draft also filtered on `ExpressionKind::List` before this.
/// Mutation testing showed that removing *that* failed no test, because an atom
/// has no children at all and is already excluded by the `first()` call below:
/// it was a second spelling of one condition, not a second condition, and it is
/// gone.
///
/// An *empty* `(:documentation "")` is not documentation, and is reported as
/// absent: satisfying a rule with an empty string is exactly the loophole
/// `missing-docstring` refuses to open with an autofix.
#[must_use]
pub fn documentation_option(view: &ExpressionView) -> Option<String> {
    view.children
        .iter()
        .filter(|child| {
            child
                .children
                .first()
                .and_then(atom_text)
                .is_some_and(|key| key.eq_ignore_ascii_case(":documentation"))
        })
        .find_map(|child| child.children.get(1))
        .and_then(string_literal_text)
        .filter(|text| !text.trim().is_empty())
}

/// A docstring's summary line: everything up to the first newline.
///
/// Doc generators, `describe`, and every editor tooltip show this line on its
/// own, which is what makes its width a question distinct from the docstring's
/// total length.
#[must_use]
pub fn summary_line(docstring: &str) -> &str {
    docstring.split('\n').next().unwrap_or(docstring)
}

// --- comments -------------------------------------------------------------

/// A comment's prose, with its delimiters removed.
///
/// `None` for a *datum* comment — `#;form` in Scheme, `#_form` in Clojure —
/// which comments out a form rather than carrying prose. The parser records
/// those in the same list as line comments, and reading one as prose would let
/// `#;(todo-list x)` be judged as a `TODO` marker.
///
/// Both line-comment leads are stripped, because there are two. Every dialect
/// this tool parses writes a line comment with `;` **except Janet**, where `;`
/// is the splice operator and the line comment is `#`
/// (`ReaderPolicy::line_comment_width`). Stripping only `;` left every Janet
/// comment reading as the prose `# …`, which begins with no marker word and so
/// silently exempted the entire dialect. Nothing else reaches this point
/// leading with `#`: the block and datum forms are handled above.
#[must_use]
pub fn comment_prose(comment: SourceComment<'_>) -> Option<&str> {
    let text = comment.text();
    if text.starts_with("#;") || text.starts_with("#_") {
        return None;
    }
    let body = match text.strip_prefix("#|") {
        Some(inner) => inner.strip_suffix("|#").unwrap_or(inner),
        None => text.trim_start_matches([';', '#']),
    };
    Some(body.trim())
}

// --- the test harness ------------------------------------------------------

/// Runs one rule end to end through the real lint engine and returns the
/// messages it emitted, in report order.
///
/// Every rule here is also tested at the `domain` level, which is where the
/// detection lives — but a domain test cannot catch a wrong
/// `HeadFilter::Heads` list, and cannot catch a missing `is_unevaluated_at`
/// call either, because the dispatcher hands a rule quoted nodes and a domain
/// test hands it whichever node the test picked. A rule that declares the wrong
/// head compiles, passes every domain test, and is simply **never invoked** by
/// the CLI. This puts the engine's own head index and dispatch between the test
/// and the rule.
#[cfg(test)]
#[must_use]
pub fn run_rule_with(
    entries: &'static [paredit_core_lint_engine::rule::RuleEntry],
    source: &str,
    settings: &paredit_core_lint_engine::model::RuleSettings,
) -> Vec<String> {
    run_rule_in(
        entries,
        source,
        settings,
        paredit_core_syntax::dialect::Dialect::CommonLisp,
    )
}

/// [`run_rule_with`], in a nominated dialect.
#[cfg(test)]
#[must_use]
pub fn run_rule_in(
    entries: &'static [paredit_core_lint_engine::rule::RuleEntry],
    source: &str,
    settings: &paredit_core_lint_engine::model::RuleSettings,
    dialect: paredit_core_syntax::dialect::Dialect,
) -> Vec<String> {
    use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::RuleCatalog;

    let catalog = RuleCatalog::new(entries);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    collect_lint_pass(
        catalog,
        &index,
        std::path::Path::new("app.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: Some(settings),
            measure: false,
        },
    )
    .expect("lint pass")
    .outcomes
    .into_iter()
    .map(|outcome| outcome.into_parts().0.message)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::view_query::for_each_subview;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    // --- the five quote shapes every node-based rule here is pinned against

    /// The span-directed lookup is what each rule's `check` calls, so it is
    /// what the quote shapes have to be asserted against.
    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node
        // even when it is data.
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "'(defun f (x) \"Doc.\" x)",
            "defun"
        ));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "(quote (defun f (x) \"Doc.\" x))",
            "defun"
        ));
    }

    #[test]
    fn a_span_inside_a_comma_in_a_hard_quote_reads_as_unevaluated() {
        // The shape a single depth counter gets wrong: the comma is a literal
        // comma character inside list data, not an escape back to code.
        assert!(unevaluated_at_first_head(
            "'(a ,(defun f (x) \"Doc.\" x))",
            "defun"
        ));
    }

    #[test]
    fn a_span_inside_a_backquote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head(
            "`(defun f (x) \"Doc.\" x)",
            "defun"
        ));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "`(a ,(defun f (x) \"Doc.\" x))",
            "defun"
        ));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f (x) \"Doc.\" x)",
            "defun"
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
    /// set of sources chosen for the shapes that could break the ordering.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for source in [
            "(a (b) (c (d)) e)",
            "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
            "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
            "(defun f (x &optional y) \"Doc (with parens).\" (list x y))",
            "(defpackage :app (:use :cl) (:export #:run))",
        ] {
            let parsed = tree(source);
            let root = parsed.root_view();
            let mut targets = Vec::new();
            for_each_subview(&root, |view| targets.push(view.span));
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

    /// The cost regression this descent exists to avoid: `is_unevaluated_at` is
    /// called once per candidate, and reading `tree.root_view()` to start the
    /// descent made a file of T candidates cost T×T.
    ///
    /// # Why 10 seconds, and why this one may assert a duration
    ///
    /// Measured on this fixture, in the `test` profile CI runs: the descent as
    /// written does the 4000 lookups in **21 ms**, and the `root_view()` shape
    /// projects to **~34 s**. The budget sits ~485× above the first and ~3.4×
    /// below the second, so there is a real window and it is occupied.
    ///
    /// That is what distinguishes this from the *ratio* assertions this batch
    /// removed elsewhere (see `cost_probe.rs`). A ratio of two short durations
    /// has no safe threshold — its variance under load is unbounded and the
    /// value being bounded sits right next to the bound. An absolute budget
    /// three orders of magnitude above the real cost is a hang detector, and a
    /// test would have to be starved 485× to trip it.
    ///
    /// Two things would invalidate that: shrinking the fixture (which lowers
    /// the 34 s and closes the window from above), or running these tests in
    /// `--release` (which lowers both numbers and could make the budget
    /// vacuous). If either changes, re-measure rather than adjust the constant.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(defun f{index} (x) \"Doc.\" x)\n"))
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

    // --- string literals

    #[test]
    fn a_string_literals_text_is_read_without_its_delimiters() {
        let parsed = tree("(f \"hello\")");
        let argument = &parsed.root_view().children[0].children[1];
        assert_eq!(string_literal_text(argument).as_deref(), Some("hello"));
    }

    #[test]
    fn an_escaped_quote_counts_as_one_character() {
        let parsed = tree("(f \"a \\\"b\\\" c\")");
        let argument = &parsed.root_view().children[0].children[1];
        assert_eq!(string_literal_text(argument).as_deref(), Some("a \"b\" c"));
    }

    #[test]
    fn a_symbol_is_not_a_string_literal() {
        let parsed = tree("(f hello)");
        let argument = &parsed.root_view().children[0].children[1];
        assert_eq!(string_literal_text(argument), None);
        assert!(!is_string_literal(argument));
    }

    // --- docstring position

    fn docstring(source: &str) -> Option<String> {
        let parsed = tree(source);
        let form = parsed.root_view().children[0].clone();
        let head = list_head(&form)?;
        let place = docstring_place(head)?;
        let shape =
            paredit_core_syntax::definition::definition_shape(Dialect::CommonLisp, &form, head)?;
        docstring_view_of(shape, place, &form).and_then(string_literal_text)
    }

    #[test]
    fn a_functions_docstring_is_read_from_the_head_of_its_body() {
        assert_eq!(
            docstring("(defun f (x) \"Adds one.\" (+ x 1))").as_deref(),
            Some("Adds one.")
        );
    }

    #[test]
    fn a_macros_and_a_methods_docstring_are_read_the_same_way() {
        assert_eq!(
            docstring("(defmacro m (x) \"Expands.\" x)").as_deref(),
            Some("Expands.")
        );
        assert_eq!(
            docstring("(defmethod area ((s square)) \"The area.\" 1)").as_deref(),
            Some("The area.")
        );
    }

    /// The guard `missing-docstring` documents, repeated here because a rule
    /// that measured `"hello"` as a summary line would be measuring a *value*.
    #[test]
    fn a_lone_string_body_is_a_return_value_and_not_a_docstring() {
        assert_eq!(docstring("(defun greeting () \"hello\")"), None);
    }

    #[test]
    fn a_function_with_no_docstring_has_none() {
        assert_eq!(docstring("(defun f (x) (+ x 1))"), None);
    }

    #[test]
    fn a_variables_docstring_is_read_from_its_fixed_slot() {
        assert_eq!(
            docstring("(defparameter *timeout* 30 \"Seconds to wait.\")").as_deref(),
            Some("Seconds to wait.")
        );
        assert_eq!(docstring("(defparameter *timeout* 30)"), None);
    }

    #[test]
    fn a_head_with_no_docstring_position_this_package_reads_has_no_place() {
        assert_eq!(docstring_place("defclass"), None);
        assert_eq!(docstring_place("defstruct"), None);
        assert_eq!(docstring_place("let"), None);
    }

    /// A computed docstring is not a string literal, and is read as absent
    /// rather than as a docstring nobody wrote.
    #[test]
    fn a_computed_docstring_is_not_read_as_one() {
        assert_eq!(docstring("(defun f (x) #.(compute-doc) (+ x 1))"), None);
    }

    // --- summary line

    #[test]
    fn the_summary_line_stops_at_the_first_newline() {
        assert_eq!(summary_line("First line.\nSecond line."), "First line.");
        assert_eq!(summary_line("Only line."), "Only line.");
        assert_eq!(summary_line(""), "");
    }

    // --- the documentation option

    #[test]
    fn a_documentation_option_is_read_from_a_direct_child() {
        let parsed = tree("(defclass c () () (:documentation \"A thing.\"))");
        let form = &parsed.root_view().children[0];
        assert_eq!(documentation_option(form).as_deref(), Some("A thing."));
    }

    #[test]
    fn an_empty_documentation_option_reads_as_absent() {
        let parsed = tree("(defclass c () () (:documentation \"\"))");
        let form = &parsed.root_view().children[0];
        assert_eq!(documentation_option(form), None);

        let parsed = tree("(defclass c () () (:documentation \"   \"))");
        let form = &parsed.root_view().children[0];
        assert_eq!(documentation_option(form), None);
    }

    #[test]
    fn a_form_with_no_documentation_option_reads_as_absent() {
        let parsed = tree("(defclass c () ((x :initarg :x)))");
        let form = &parsed.root_view().children[0];
        assert_eq!(documentation_option(form), None);
    }

    /// A `:documentation` belonging to a *slot* is not the form's own. Reading
    /// it would let one documented slot silence the whole class.
    #[test]
    fn a_slots_documentation_is_not_the_forms_documentation() {
        let parsed = tree("(defclass c () ((x :initarg :x :documentation \"The x slot.\")))");
        let form = &parsed.root_view().children[0];
        assert_eq!(documentation_option(form), None);
    }

    // --- comments

    fn prose(source: &str) -> Vec<String> {
        let parsed = tree(source);
        parsed
            .comments()
            .filter_map(comment_prose)
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_line_comments_prose_is_read_without_its_semicolons() {
        assert_eq!(
            prose(";; Explains the next form.\n(f)"),
            vec!["Explains the next form.".to_owned()]
        );
        assert_eq!(prose(";;;; A heading.\n(f)"), vec!["A heading.".to_owned()]);
    }

    /// The trap that is to a comment rule what a quoted form is to a node rule:
    /// text that *looks* like a comment but is a string literal is not a
    /// comment, and the parser already knows the difference.
    #[test]
    fn a_semicolon_inside_a_string_literal_is_not_a_comment() {
        assert!(prose("(f \"; not a comment\")").is_empty());
        assert!(prose("(defun f () \"; TODO: not a comment\" 1)").is_empty());
    }

    #[test]
    fn a_datum_comment_carries_no_prose() {
        // `#;` comments out a *form*. Reading it as prose would let the form's
        // own symbols be judged as English.
        let parsed =
            SyntaxTree::parse_with_dialect("#;(todo-list x)\n(f)", Dialect::Scheme).expect("parse");
        assert!(parsed.comments().filter_map(comment_prose).next().is_none());
    }

    #[test]
    fn a_block_comments_prose_is_read_without_its_delimiters() {
        assert_eq!(prose("#| A block. |#\n(f)"), vec!["A block.".to_owned()]);
    }

    /// Janet is the one dialect whose line comment is not `;`. Stripping only
    /// `;` left every Janet comment reading as the prose `# …`, which exempted
    /// the whole dialect from every comment rule without failing anything.
    #[test]
    fn a_janet_hash_line_comment_is_read_as_prose() {
        let parsed =
            SyntaxTree::parse_with_dialect("# Explains the next form.\n(f)", Dialect::Janet)
                .expect("parse");
        let read: Vec<&str> = parsed.comments().filter_map(comment_prose).collect();
        assert_eq!(read, vec!["Explains the next form."]);
    }

    /// And `;` in Janet is the splice operator, not a comment lead — so there
    /// is nothing for a comment rule to read there at all.
    #[test]
    fn a_semicolon_in_janet_is_not_a_comment() {
        let parsed = SyntaxTree::parse_with_dialect("(f ;x)", Dialect::Janet).expect("parse");
        assert_eq!(parsed.comments().count(), 0);
    }
}
