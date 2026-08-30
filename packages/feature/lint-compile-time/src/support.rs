//! What the compile-time rules share: which parts of a file are *code*, which
//! forms are *top level* in the sense CLHS 3.2.3.1 means, and how to read an
//! `eval-when` situation list.
//!
//! # Evaluation context
//!
//! The quote machinery below ([`QuoteState`], [`for_each_evaluated_subview`])
//! is a deliberate copy of `paredit-feature-lint-condition-system`'s
//! `support.rs`, not a new design and not a cross-package dependency — the same
//! copy `paredit-feature-lint-build-system` keeps, for the same reason. Two
//! independent counters are required because `'` and `` ` `` are not the same
//! thing:
//!
//! - a comma inside `'(…)` is a comma *character* in a literal list, so `hard`
//!   never clears — a single `i32` depth counter gets `'(a ,X)` wrong;
//! - a comma inside `` `(…) `` escapes back to code, so `quasi` counts up and
//!   down.
//!
//! That distinction is not incidental here, it is the whole of
//! `crate::macro_helper_not_compile_time`: a helper called from a macro's
//! quasiquote *template* runs at the expansion's run time and needs nothing at
//! compile time, while the same call one comma deeper runs at macroexpansion
//! time and does. Both were checked against SBCL 2.6.0 and behave exactly that
//! way — see that rule's module docs.
//!
//! # Top level, and why it is not "depth 0"
//!
//! CLHS 3.2.3.1 defines a *top level form* by a recursion, not by depth. The
//! body of a top-level `progn`, `locally`, `macrolet`, `symbol-macrolet` or
//! `eval-when` is itself processed as top level. Confirmed against SBCL 2.6.0:
//! `(progn (eval-when (:execute) (defmacro m …)))` behaves exactly as the same
//! `eval-when` written at depth 0 — the macro vanishes under `compile-file` and
//! survives under `load` — while wrapping it in `(let () …)` instead makes the
//! `eval-when` an ordinary nested form, where only `:execute` is ever
//! considered and naming it is correct rather than suspect.
//!
//! A previous batch in this repository shipped rules that got this wrong and
//! produced false positives on exactly the `locally`/`macrolet`/
//! `symbol-macrolet` shapes, which is why [`is_top_level_form`] enumerates them
//! rather than testing a depth.
//!
//! # Cost
//!
//! Nothing here is called per visited node. The `clean/forms/*` benchmarks lint
//! files with zero findings, so the per-file cost of a rule that matches
//! nothing is exactly what they measure. Every rule in this package anchors on
//! [`HeadFilter::Heads`], answers a *node-local* question from the dispatched
//! node alone, and only then — if that question came back interesting — asks
//! anything that touches the tree.
//!
//! That ordering is load-bearing rather than tidy. [`is_top_level_form`]
//! materializes the enclosing top-level form, and a sibling package measured
//! 450843 ns/call against 28 ns/call purely from asking such a question before
//! the cheap one instead of after it. Each rule's `check` documents its own
//! ordering.
//!
//! Nor is anything here quadratic in the number of candidates.
//! [`is_top_level_form`] is called at most once per candidate, so it is allowed
//! to cost the *enclosing top-level form* and never the file: it binary-searches
//! the top level for the one root child containing the span and materializes
//! only that form. The search itself reads [`SyntaxTree::root_child_span`],
//! which is an index into a slice and a field read;
//! `select_path(&Path::root_child(i))` would heap-allocate an
//! `ExpressionPath`'s `Vec` on every step of the search instead of once at the
//! end.
//!
//! # `scratch_cache` is deliberately not used
//!
//! `RuleContext::scratch_cache` looks like the right home for
//! `crate::macro_helper_not_compile_time`'s per-file set of same-file
//! `defun` names. It is not usable: the slot holds **one type per file's
//! pass**, `paredit-feature-lint-repl-debug` already stores its evaluated-forms
//! walk there (`packages/feature/lint-repl-debug/src/support.rs:612`), and a
//! second caller with a different `T` *panics* rather than missing the cache.
//! `inspect lint` runs every rule on every file, so the two would meet on the
//! first Common Lisp file with both a `defmacro` and a REPL-debug candidate.
//! That rule therefore pays its own scan, under guards documented there.
//!
//! [`HeadFilter::Heads`]: paredit_core_lint_engine::model::HeadFilter::Heads

use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, unqualified};

// --- evaluation context ---------------------------------------------------

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    pub const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    #[must_use]
    pub const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none of
    /// them turns code into data.
    #[must_use]
    pub fn after_prefixes(mut self, view: &ExpressionView) -> Self {
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

    #[must_use]
    pub const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| normalized_symbol(head) == "quote")
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

// --- symbols --------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every head comparison here is written
/// in.
///
/// A keyword keeps its leading colon: `unqualified(":execute")` is
/// `":execute"`, which is what [`EvalWhenSituations`] compares against.
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

/// Whether an atom's text carries a reader conditional.
///
/// The dialect-aware parse folds `#+sbcl :compile-toplevel` into a **single
/// atom** whose text is `"#+sbcl :compile-toplevel"`, so an equality test on the
/// symbol never sees the keyword. Every reader of a situation list has to notice
/// that and bail rather than read the conditional as an unrecognized situation
/// — which would turn a conditionally-supplied `:compile-toplevel` into an
/// absent one, the single direction these rules must not get wrong.
#[must_use]
pub fn carries_reader_conditional(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("#+") || trimmed.starts_with("#-")
}

// --- eval-when situations -------------------------------------------------

/// Which of the three situations an `eval-when` names.
///
/// The deprecated spellings `compile`, `load` and `eval` are accepted alongside
/// `:compile-toplevel`, `:load-toplevel` and `:execute`. SBCL 2.6.0 emits a
/// style warning for them and then honours them exactly like the modern names
/// (verified: `(eval-when (compile load eval) (defmacro m …))` behaves
/// identically to the `:compile-toplevel :load-toplevel :execute` spelling under
/// both `load` and `compile-file`), so a rule that ignored them would miss real
/// findings in old code and — worse — report the modern-name rules' findings
/// against files that had in fact named the situation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EvalWhenSituations {
    pub compile_toplevel: bool,
    pub load_toplevel: bool,
    pub execute: bool,
}

impl EvalWhenSituations {
    /// Whether the body survives into the compiled file at all.
    ///
    /// A top-level `eval-when` naming neither `:compile-toplevel` nor
    /// `:load-toplevel` is **discarded entirely** by `compile-file` — not merely
    /// deferred. Verified against SBCL 2.6.0.
    #[must_use]
    pub const fn reaches_the_compiler(self) -> bool {
        self.compile_toplevel || self.load_toplevel
    }
}

/// Reads an `eval-when`'s situation list, or `None` if it cannot be read
/// exactly.
///
/// `None` for a situations form that is not a `(…)` list, that contains a
/// non-atom, that contains an atom carrying a reader conditional, or that names
/// a situation this reader does not recognize. Every caller treats `None` as
/// "say nothing", so an unreadable situation list is a missed finding rather
/// than a guessed one.
#[must_use]
pub fn read_situations(situations: &ExpressionView) -> Option<EvalWhenSituations> {
    if !is_paren_list(situations) {
        return None;
    }
    let mut found = EvalWhenSituations::default();
    for element in &situations.children {
        if !element.children.is_empty() {
            return None;
        }
        let text = atom_symbol_text(element)?;
        // No explicit reader-conditional bail here, deliberately. Mutation
        // testing showed one to be **dead code**: `#+sbcl :compile-toplevel`
        // folds into a single atom whose normalized spelling is
        // `"#+sbcl :compile-toplevel"`, which matches no arm below and so
        // already falls through to `_ => return None`. Removing the explicit
        // guard changed no test's outcome, which is what "the guard kills
        // nothing" means. The *property* stays pinned by
        // `a_reader_conditional_in_the_situation_list_bails_rather_than_guessing`
        // regardless of which arm delivers it.
        match normalized_symbol(text).as_str() {
            ":compile-toplevel" | "compile" => found.compile_toplevel = true,
            ":load-toplevel" | "load" => found.load_toplevel = true,
            ":execute" | "eval" => found.execute = true,
            _ => return None,
        }
    }
    Some(found)
}

// --- top level, per CLHS 3.2.3.1 ------------------------------------------

/// The child index at which a top-level-preserving operator's *body* begins.
///
/// CLHS 3.2.3.1 processes the body of each of these as top level forms in their
/// own right. The index matters as much as the head does: the situations list of
/// an `eval-when` and the bindings list of a `macrolet` are **not** body, and a
/// candidate found inside one of them is not a top-level form.
fn top_level_body_start(head: &str) -> Option<usize> {
    match head {
        // (progn form*) / (locally declaration* form*)
        "progn" | "locally" => Some(1),
        // (eval-when (situation*) form*)
        // (macrolet (definition*) form*) / (symbol-macrolet (binding*) form*)
        "eval-when" | "macrolet" | "symbol-macrolet" => Some(2),
        _ => None,
    }
}

/// The index of the one child of `view` whose span covers `target`, found
/// without reading the others.
///
/// A node's children are in document order and do not overlap, so the only child
/// that can contain `target` is the last one beginning at or before it — which a
/// binary search finds in `log₂ k` comparisons instead of `k`.
fn child_index_containing(view: &ExpressionView, target: ByteSpan) -> Option<usize> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let index = after.checked_sub(1)?;
    span_contains(view.children.get(index)?.span, target).then_some(index)
}

/// The top-level form containing `target`, materialized on its own.
///
/// The binary search reads [`SyntaxTree::root_child_span`], which is an index
/// into a slice and a field read. The obvious spelling
/// `select_path(&Path::root_child(middle))?.span()` looks like the same thing
/// but [`Path::root_child`] builds an `ExpressionPath`, which owns a `Vec`, so
/// it heap-allocates once per step of the search. Only the single surviving
/// candidate is materialized.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if tree.root_child_span(middle)?.start().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.checked_sub(1)?;
    if !span_contains(tree.root_child_span(index)?, target) {
        return None;
    }
    Some(tree.select_path(&Path::root_child(index)).ok()?.view())
}

/// Whether the node at `target` is a **top level form** in the sense CLHS
/// 3.2.3.1 means.
///
/// True for a root child, and for a node reached from one through nothing but
/// the *body* positions of `progn`, `locally`, `macrolet`, `symbol-macrolet` and
/// `eval-when`. False for anything under a `let`, a `defun`, a lambda, the
/// situations list of an `eval-when`, or the bindings list of a `macrolet` —
/// and false for anything under a `'` or `` ` ``, which is data rather than a
/// form at all.
///
/// Cost is the enclosing top-level form's size and never the file's; see the
/// module docs. Call it only after the node-local question has come back
/// interesting.
#[must_use]
pub fn is_top_level_form(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(root_child) = root_child_containing(tree, target) else {
        return false;
    };
    let mut view = &root_child;
    loop {
        // A quoted or quasiquoted ancestor makes this data, not a form. Checked
        // at every level, including the root child itself, because `'(progn …)`
        // carries its prefix on the outermost node.
        if QuoteState::EVALUATED.after_prefixes(view).is_data() {
            return false;
        }
        if view.span == target {
            return true;
        }
        let Some(head) = list_head(view) else {
            return false;
        };
        let Some(body_start) = top_level_body_start(&normalized_symbol(head)) else {
            return false;
        };
        let Some(index) = child_index_containing(view, target) else {
            return false;
        };
        if index < body_start {
            // The head itself, an `eval-when`'s situations, or a `macrolet`'s
            // bindings: inside the form but not one of its body forms.
            return false;
        }
        view = &view.children[index];
    }
}

// --- definitions ----------------------------------------------------------

/// Whether `view` is a definition form — anything the shared classifier
/// recognizes as introducing a name.
///
/// Used to decide whether an `eval-when` body is *worth* complaining about.
/// `(eval-when (:execute) (format t "hi"))` is a judgement call about intent;
/// `(eval-when (:execute) (defmacro m …))` is a definition that silently does
/// not exist in the compiled file, which is not.
#[must_use]
pub fn is_definition_form(view: &ExpressionView) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    definition_shape(Dialect::CommonLisp, view, head).is_some()
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
    use paredit_core_syntax::view_query::for_each_subview;

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
        assert!(evaluated_heads("'(eval-when (foo))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (eval-when (foo)))"), vec!["quote"]);
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(eval-when (foo)))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(eval-when (foo))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(eval-when (foo)))"),
            vec!["eval-when", "foo"]
        );
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(eval-when (foo))\")"), vec!["f"]);
    }

    // --- situations

    fn situations(source: &str) -> Option<EvalWhenSituations> {
        let parsed = tree(source);
        let form = &parsed.root_view().children[0];
        read_situations(&form.children[1])
    }

    #[test]
    fn the_modern_situation_names_are_read() {
        assert_eq!(
            situations("(eval-when (:compile-toplevel :load-toplevel :execute) 1)"),
            Some(EvalWhenSituations {
                compile_toplevel: true,
                load_toplevel: true,
                execute: true,
            })
        );
    }

    /// SBCL still honours these and merely style-warns, so the rules must read
    /// them or they would report against a file that named the situation.
    #[test]
    fn the_deprecated_situation_names_are_read_too() {
        assert_eq!(
            situations("(eval-when (compile load eval) 1)"),
            Some(EvalWhenSituations {
                compile_toplevel: true,
                load_toplevel: true,
                execute: true,
            })
        );
    }

    #[test]
    fn an_empty_situation_list_names_nothing() {
        assert_eq!(
            situations("(eval-when () 1)"),
            Some(EvalWhenSituations::default())
        );
    }

    #[test]
    fn a_package_qualified_situation_is_still_read() {
        assert_eq!(
            situations("(eval-when (cl:compile) 1)").map(|s| s.compile_toplevel),
            Some(true)
        );
    }

    #[test]
    fn an_upcased_situation_is_still_read() {
        assert_eq!(
            situations("(eval-when (:EXECUTE) 1)").map(|s| s.execute),
            Some(true)
        );
    }

    /// The reader folds the conditional into the keyword's atom, so an equality
    /// test never sees `:compile-toplevel`. Reading it as an unrecognized
    /// situation would turn a conditionally-supplied situation into an absent
    /// one — a false positive. Bailing turns it into a missed finding.
    #[test]
    fn a_reader_conditional_in_the_situation_list_bails_rather_than_guessing() {
        let parsed = tree("(eval-when (#+sbcl :compile-toplevel :execute) 1)");
        let form = &parsed.root_view().children[0];
        let list = &form.children[1];
        assert_eq!(
            atom_symbol_text(&list.children[0]),
            Some("#+sbcl :compile-toplevel"),
            "the reader no longer folds the conditional into the keyword's atom"
        );
        assert_eq!(read_situations(list), None);
    }

    #[test]
    fn an_unreadable_situation_list_says_nothing() {
        assert_eq!(situations("(eval-when :execute 1)"), None);
        assert_eq!(situations("(eval-when (:bogus) 1)"), None);
        assert_eq!(situations("(eval-when ((:execute)) 1)"), None);
    }

    #[test]
    fn reaching_the_compiler_needs_one_of_the_two_toplevel_situations() {
        let read = |source| situations(source).expect("readable");
        assert!(!read("(eval-when (:execute) 1)").reaches_the_compiler());
        assert!(!read("(eval-when () 1)").reaches_the_compiler());
        assert!(read("(eval-when (:load-toplevel) 1)").reaches_the_compiler());
        assert!(read("(eval-when (:compile-toplevel) 1)").reaches_the_compiler());
    }

    // --- top level, per CLHS 3.2.3.1

    /// Finds the first node whose head is `head` and asks whether it is a top
    /// level form. Uses the *unfiltered* walk, so the node is found even when it
    /// is data.
    fn top_level_at(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_top_level_form(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_root_child_is_a_top_level_form() {
        assert!(top_level_at("(eval-when (:execute) 1)", "eval-when"));
    }

    /// The four operators CLHS 3.2.3.1 recurses through, each verified against
    /// SBCL 2.6.0 to behave exactly as the same form at depth 0.
    #[test]
    fn the_top_level_preserving_operators_keep_their_body_top_level() {
        for source in [
            "(progn (eval-when (:execute) 1))",
            "(locally (eval-when (:execute) 1))",
            "(macrolet () (eval-when (:execute) 1))",
            "(symbol-macrolet () (eval-when (:execute) 1))",
            "(eval-when (:execute) (eval-when (:execute) 1))",
            "(progn (progn (locally (eval-when (:execute) 1))))",
        ] {
            assert!(top_level_at(source, "eval-when"), "not top level: {source}");
        }
    }

    #[test]
    fn an_ordinary_binding_form_does_not_keep_its_body_top_level() {
        for source in [
            "(let () (eval-when (:execute) 1))",
            "(defun f () (eval-when (:execute) 1))",
            "(lambda () (eval-when (:execute) 1))",
            "(when t (eval-when (:execute) 1))",
            "(flet ((g () 1)) (eval-when (:execute) 1))",
        ] {
            assert!(
                !top_level_at(source, "eval-when"),
                "wrongly top level: {source}"
            );
        }
    }

    /// The body-start index, not just the head, is what makes these false. A
    /// candidate inside a `macrolet`'s bindings or an `eval-when`'s situations
    /// is inside the form but is not one of its body forms.
    #[test]
    fn a_non_body_position_of_a_top_level_operator_is_not_top_level() {
        assert!(!top_level_at(
            "(macrolet ((m () (eval-when (:execute) 1))) 2)",
            "eval-when"
        ));
        assert!(!top_level_at(
            "(symbol-macrolet ((x (eval-when (:execute) 1))) 2)",
            "eval-when"
        ));
    }

    /// The `index < body_start` test, isolated.
    ///
    /// The two cases above do not actually reach it: a `macrolet`'s bindings
    /// list has a *list* as its own head, so the descent stops at the
    /// `list_head` test one level further in and the body-start test is never
    /// consulted. Mutation testing caught that — deleting `index < body_start`
    /// left every test green.
    ///
    /// This is the shape that does reach it. The situations list of the outer
    /// `eval-when` is child 1, below its body start of 2, and it happens to be
    /// a list whose head *is* one of the top-level-preserving operators — so
    /// without the position test the descent would walk straight through the
    /// situations list and call the inner `eval-when` a top level form.
    #[test]
    fn a_top_level_operator_nested_in_a_situations_list_is_still_not_top_level() {
        let source = "(eval-when (progn (eval-when (:execute) (defmacro m () 1))) 2)";
        let parsed = tree(source);
        let mut spans = Vec::new();
        for_each_subview(&parsed.root_view(), |view| {
            if list_head(view).is_some_and(|found| found == "eval-when") {
                spans.push(view.span);
            }
        });
        assert_eq!(spans.len(), 2, "expected an outer and an inner eval-when");
        // The outer one is a root child and is top level.
        assert!(is_top_level_form(&parsed, spans[0]));
        // The inner one sits in the outer's *situations* list, which is child 1
        // and below the body start of 2.
        assert!(
            !is_top_level_form(&parsed, spans[1]),
            "the body-start position test is not being applied"
        );
    }

    #[test]
    fn a_quoted_form_is_data_and_never_top_level() {
        for source in [
            "'(eval-when (:execute) 1)",
            "`(eval-when (:execute) 1)",
            "(quote (eval-when (:execute) 1))",
            "'(progn (eval-when (:execute) 1))",
            "(progn '(eval-when (:execute) 1))",
        ] {
            assert!(
                !top_level_at(source, "eval-when"),
                "quoted data read as a top level form: {source}"
            );
        }
    }

    /// The binary search over the top level must select the same root child a
    /// linear scan would, including for the last form and for a file of one.
    #[test]
    fn the_root_child_search_answers_what_a_linear_scan_would() {
        for source in [
            "(a) (b) (c)",
            "(only)",
            "(a)\n\n(eval-when (:execute) 1)\n\n(c)",
            "'(a) `(b ,(c)) #'d",
            "(defsystem \"x\" #+sbcl :serial t)",
        ] {
            let parsed = tree(source);
            let root = parsed.root_view();
            for (index, child) in root.children.iter().enumerate() {
                let found = root_child_containing(&parsed, child.span).expect("a containing child");
                assert_eq!(
                    found.span, child.span,
                    "{source}: wrong root child for index {index}"
                );
            }
        }
    }

    /// The cost regression this descent exists to avoid. `is_top_level_form` is
    /// called once per candidate, and starting it from `tree.root_view()` would
    /// make a file of T candidates cost T×T. The budget is deliberately hundreds
    /// of times the linear cost, so only an asymptotic regression can trip it.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(eval-when (:execute) (defun f{index} () 1))\n"))
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
            assert!(is_top_level_form(&parsed, span));
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "4000 lookups took {elapsed:?}; the descent is scanning the top level again"
        );
    }

    // --- definitions and the byte-scan guard

    #[test]
    fn definition_forms_are_recognized() {
        for source in [
            "(defun f () 1)",
            "(defmacro m () 1)",
            "(defconstant +c+ 1)",
            "(defvar *v* 1)",
            "(defclass c () ())",
            "(define-condition e (error) ())",
        ] {
            let parsed = tree(source);
            assert!(
                is_definition_form(&parsed.root_view().children[0]),
                "not recognized as a definition: {source}"
            );
        }
    }

    #[test]
    fn a_call_is_not_a_definition_form() {
        for source in ["(format t \"hi\")", "(setf *x* 1)", "(1+ 2)"] {
            let parsed = tree(source);
            assert!(
                !is_definition_form(&parsed.root_view().children[0]),
                "wrongly a definition: {source}"
            );
        }
    }

    #[test]
    fn the_mention_guard_answers_yes_only_for_the_spelling() {
        assert!(mentions("(eval-when (:execute) 1)", "eval-when"));
        assert!(mentions("(EVAL-WHEN (:EXECUTE) 1)", "eval-when"));
        assert!(!mentions("(progn 1)", "eval-when"));
        assert!(!mentions("", "eval-when"));
        assert!(!mentions("eval-whe", "eval-when"));
    }

    #[test]
    fn the_reader_conditional_guard_sees_both_polarities() {
        assert!(carries_reader_conditional("#+sbcl :execute"));
        assert!(carries_reader_conditional("#-sbcl :execute"));
        assert!(!carries_reader_conditional(":execute"));
        assert!(!carries_reader_conditional("#'f"));
    }
}
