//! What the rules here share: which parts of a file are *code*, how a symbol is
//! spelled once normalized, and what the file's own `define-condition` forms say
//! about each other.
//!
//! # Copied, deliberately
//!
//! The [`QuoteState`] quote model and [`is_unevaluated_at`] are copied from
//! `paredit-feature-lint-condition-system::support`, which is what the other
//! lint packages do with it. A consolidation of that helper into
//! `packages/core` is in flight; when it lands this module should be deleted
//! and the shared one used.
//!
//! The model is **two counters, not a depth**. `'` and `` ` `` are not the same
//! thing: a comma inside `'(…)` is a comma character in a literal list, so
//! `hard` never clears, while a comma inside `` `(…) `` escapes back to code, so
//! `quasi` counts up and down. A single `i32` depth counter is wrong and has
//! shipped as a false-positive source twice.
//!
//! Nothing here is called per visited node. The `clean/forms/*` benchmarks lint
//! files with zero findings, so the per-file cost of a rule that matches nothing
//! is exactly what they measure — and [`is_unevaluated_at`] reaches
//! `root_view()`, so a rule must call it only once it already has a candidate.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters; see the module documentation for why one is not
/// enough.
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
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none of
    /// them turns code into data.
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

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(signal 'x)) `` has a quasiquoted ancestor
/// and an evaluated target.
///
/// The root's own span is never consulted: a file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
///
/// **Reaches `root_view()`.** Call it only once a rule already has a finding.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state.is_data();
        }
    }
}

/// Calls `visit` on every node of `root` reachable as evaluated code.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
pub fn for_each_evaluated_subview(root: &ExpressionView, visit: impl FnMut(&ExpressionView)) {
    for_each_evaluated_subview_where(root, |_| true, visit);
}

/// [`for_each_evaluated_subview`], with a say in where the walk stops.
///
/// `descend` is asked about each visited node *before* its children are queued;
/// answering `false` visits that node and nothing under it. That is how
/// `unwind-protect-cleanup-can-signal` stops at a `handler-case` inside a
/// cleanup: an `error` that the cleanup itself already handles is not one that
/// escapes, and reporting it would be a false positive on the exact spelling the
/// rule recommends.
///
/// A pruned node's subtree is skipped *including* any quoted data in it, which
/// is correct here and would not be if this were used to find data.
pub fn for_each_evaluated_subview_where(
    root: &ExpressionView,
    mut descend: impl FnMut(&ExpressionView) -> bool,
    mut visit: impl FnMut(&ExpressionView),
) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
            if !descend(view) {
                continue;
            }
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

/// An atom's symbol text, past any reader prefix, lowercased and stripped of its
/// package qualifier — the spelling every comparison here is written in.
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

/// `'foo` or `(quote foo)` read as `foo`. `None` for anything else, including an
/// unquoted symbol, a quoted *list*, and a string.
#[must_use]
pub fn quoted_symbol(view: &ExpressionView) -> Option<String> {
    if is_quote_form(view) {
        return view.children.get(1).and_then(symbol_name);
    }
    if !view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return None;
    }
    symbol_name(view)
}

/// Whether an atom is a string literal.
///
/// The reader keeps the delimiters, so a leading `"` is what distinguishes
/// `"abc"` from the symbol `abc`.
///
/// # This also excludes reader conditionals, and that is load-bearing
///
/// A CL reader conditional and the form after it fold into a **single Atom**,
/// and `atom_text` returns the text *with* the prefix attached:
///
/// ```text
/// (error 'my-error #+sbcl "boom")
///   child[2] kind=Atom text="#+sbcl \"boom\"" prefixes=[]
/// ```
///
/// So `#+sbcl "boom"` does not start with `"` and is not a string literal here.
/// A separate `is_reader_conditional` guard was written for this, measured by
/// mutation testing, and found to kill **no** test — it was unreachable for
/// exactly this reason, which is the trap this repository has hit before. The
/// knowledge lives here, where it is reachable, rather than in a guard that
/// never runs. See `a_reader_conditional_is_never_a_string_literal`.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// One `define-condition` in the file being linted.
#[derive(Debug, Clone)]
pub struct ConditionClass {
    /// The condition's name, normalized.
    pub name: String,
    /// Its declared direct supertypes, normalized.
    pub supertypes: Vec<String>,
}

/// The standard hierarchy, as the edges that decide whether a type is an error
/// and whether it is a warning.
///
/// Not the whole of CLHS figure 9-1 — only what the rules here depend on.
const STANDARD_SUPERTYPES: &[(&str, &str)] = &[
    ("simple-condition", "condition"),
    ("serious-condition", "condition"),
    ("warning", "condition"),
    ("simple-warning", "warning"),
    ("style-warning", "warning"),
    ("error", "serious-condition"),
    ("storage-condition", "serious-condition"),
    ("arithmetic-error", "error"),
    ("cell-error", "error"),
    ("control-error", "error"),
    ("file-error", "error"),
    ("package-error", "error"),
    ("parse-error", "error"),
    ("print-not-readable", "error"),
    ("program-error", "error"),
    ("simple-error", "error"),
    ("stream-error", "error"),
    ("type-error", "error"),
    ("division-by-zero", "arithmetic-error"),
    ("floating-point-inexact", "arithmetic-error"),
    ("floating-point-invalid-operation", "arithmetic-error"),
    ("floating-point-overflow", "arithmetic-error"),
    ("floating-point-underflow", "arithmetic-error"),
    ("unbound-slot", "cell-error"),
    ("unbound-variable", "cell-error"),
    ("undefined-function", "cell-error"),
    ("end-of-file", "stream-error"),
    ("reader-error", "parse-error"),
    ("simple-type-error", "type-error"),
];

fn standard_supertype(name: &str) -> Option<&'static str> {
    STANDARD_SUPERTYPES
        .iter()
        .find(|(subtype, _)| *subtype == name)
        .map(|(_, supertype)| *supertype)
}

/// Reads one `(define-condition name (supertype*) (slot*) option*)`.
///
/// `None` for anything that is not one, and for a form too short to have a
/// supertype list: guessing at a malformed definition's shape would make every
/// consumer of the hierarchy depend on that guess.
#[must_use]
pub fn read_define_condition(view: &ExpressionView) -> Option<ConditionClass> {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_is(head, "define-condition"))
    {
        return None;
    }
    let name = view.children.get(1).and_then(symbol_name)?;
    let list = view.children.get(2)?;
    if !is_paren_list(list) {
        return None;
    }
    Some(ConditionClass {
        name,
        supertypes: list.children.iter().filter_map(symbol_name).collect(),
    })
}

/// The `define-condition` forms of one file, and what they say about each other.
///
/// Same-file only, on purpose. A supertype defined in another file is a name
/// this analysis knows nothing about, and inventing an answer for it would make
/// every rule that consults the hierarchy report on faith.
#[derive(Debug, Default)]
pub struct ConditionHierarchy {
    classes: Vec<ConditionClass>,
}

/// The spelling every `define-condition` form contains, whatever package
/// qualifier or case it is written in.
const DEFINE_CONDITION: &str = "define-condition";

/// Whether the file's bytes contain the spelling at all.
///
/// A file whose source never spells `define-condition` cannot contain one, and
/// its hierarchy is empty without looking at a single node. The converse does
/// not hold — a mention inside a string answers `true` — which is the harmless
/// direction: the walk then runs and finds nothing.
fn mentions_define_condition(source: &str) -> bool {
    let needle = DEFINE_CONDITION.as_bytes();
    source
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

impl ConditionHierarchy {
    /// Reads every `define-condition` reachable as code in one file, guarded by
    /// [`mentions_define_condition`] so a file that defines none pays one byte
    /// scan instead of a walk.
    #[must_use]
    pub fn collect(tree: &SyntaxTree) -> Self {
        if !mentions_define_condition(tree.source()) {
            return Self::default();
        }
        let mut classes = Vec::new();
        for_each_evaluated_subview(&tree.root_view(), |view| {
            if let Some(class) = read_define_condition(view) {
                classes.push(class);
            }
        });
        Self { classes }
    }

    #[must_use]
    pub fn class(&self, name: &str) -> Option<&ConditionClass> {
        self.classes.iter().find(|class| class.name == name)
    }

    /// Whether this file defines `name` at all. The precondition for saying
    /// anything about it.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.class(name).is_some()
    }

    /// `name` and every type it transitively inherits from, following this
    /// file's own definitions and the standard hierarchy both.
    ///
    /// Reflexive and cycle-safe: a definition that names itself, directly or
    /// through a loop, terminates instead of hanging.
    #[must_use]
    pub fn ancestry(&self, name: &str) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut frontier = vec![name.to_ascii_lowercase()];
        while let Some(current) = frontier.pop() {
            if seen.contains(&current) {
                continue;
            }
            if let Some(class) = self.class(&current) {
                frontier.extend(class.supertypes.iter().cloned());
            }
            if let Some(standard) = standard_supertype(&current) {
                frontier.push(standard.to_owned());
            }
            seen.push(current);
        }
        seen
    }

    /// Whether `name` reaches `warning` **and not** `serious-condition`.
    ///
    /// Both halves are load-bearing. A class may inherit from `warning` *and*
    /// from `error` — CLHS permits it, since condition classes are CLOS classes
    /// with multiple inheritance — and such a class is a serious condition, so
    /// signalling it with `error` is correct. Asking only about `warning` would
    /// report it.
    #[must_use]
    pub fn is_warning_and_not_serious(&self, name: &str) -> bool {
        let ancestry = self.ancestry(name);
        ancestry.iter().any(|ancestor| ancestor == "warning")
            && !ancestry
                .iter()
                .any(|ancestor| ancestor == "error" || ancestor == "serious-condition")
    }
}

/// A [`ConditionHierarchy`] read from the file only if someone asks.
///
/// Constructing one costs nothing, so a rule creates it unconditionally at the
/// top of `check()` and never calls [`LazyHierarchy::get`] on a node that turns
/// out not to matter.
///
/// # What this does *not* memoize
///
/// The `OnceCell` spans one `check()` call, so a file with N nodes that reach
/// the hierarchy builds it N times. Removing that needs a cache outliving a
/// single `check()`, and the only such slot is `RuleContext::scratch_cache` —
/// one type-erased slot per file's pass, already claimed by
/// `paredit-feature-lint-repl-debug`, which panics by construction if a second
/// type is stored in it during the same pass. The
/// [`ConditionHierarchy::collect`] guard makes each of those builds a byte scan
/// rather than a tree walk, which is what that shape actually costs.
#[derive(Debug)]
pub struct LazyHierarchy<'a> {
    tree: &'a SyntaxTree,
    hierarchy: std::cell::OnceCell<ConditionHierarchy>,
}

impl<'a> LazyHierarchy<'a> {
    #[must_use]
    pub const fn new(tree: &'a SyntaxTree) -> Self {
        Self {
            tree,
            hierarchy: std::cell::OnceCell::new(),
        }
    }

    /// Reads the file's `define-condition` forms, once.
    pub fn get(&self) -> &ConditionHierarchy {
        self.hierarchy
            .get_or_init(|| ConditionHierarchy::collect(self.tree))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

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

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(unwind-protect (foo))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(unwind-protect (foo))").is_empty());
    }

    /// The two-counter model's whole point: a comma re-enters code under `` ` ``
    /// and does not under `'`.
    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(unwind-protect (foo)))"),
            vec!["unwind-protect", "foo"]
        );
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(unwind-protect (foo)))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(unwind-protect (foo))\")"), vec!["f"]);
    }

    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("'(error 'x)", "error"));
        assert!(unevaluated_at_first_head("(quote (error 'x))", "error"));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f () (error 'x))",
            "error"
        ));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head("`(a ,(error 'x))", "error"));
    }

    #[test]
    fn a_quoted_symbol_is_read_through_both_spellings() {
        let parsed = tree("(error 'my-error)");
        let call = &parsed.root_view().children[0];
        assert_eq!(
            quoted_symbol(&call.children[1]).as_deref(),
            Some("my-error")
        );

        let parsed = tree("(error (quote my-error))");
        let call = &parsed.root_view().children[0];
        assert_eq!(
            quoted_symbol(&call.children[1]).as_deref(),
            Some("my-error")
        );
    }

    #[test]
    fn a_package_qualified_type_normalizes_to_its_name() {
        let parsed = tree("(error 'app::my-error)");
        let call = &parsed.root_view().children[0];
        assert_eq!(
            quoted_symbol(&call.children[1]).as_deref(),
            Some("my-error")
        );
    }

    #[test]
    fn an_unquoted_symbol_is_not_a_quoted_symbol() {
        let parsed = tree("(error my-error)");
        let call = &parsed.root_view().children[0];
        assert_eq!(quoted_symbol(&call.children[1]), None);
    }

    #[test]
    fn a_string_literal_is_recognised_by_its_delimiter() {
        let parsed = tree("(error \"boom\" foo)");
        let call = &parsed.root_view().children[0];
        assert!(is_string_literal(&call.children[1]));
        assert!(!is_string_literal(&call.children[2]));
    }

    /// The trap: `#+sbcl "boom"` is one atom whose text carries the prefix, so
    /// it is not a string literal and no separate guard is needed — or possible.
    #[test]
    fn a_reader_conditional_is_never_a_string_literal() {
        let parsed = tree("(error 'my-error #+sbcl \"boom\")");
        let call = &parsed.root_view().children[0];
        let argument = &call.children[2];
        assert_eq!(
            atom_text(argument),
            Some("#+sbcl \"boom\""),
            "the reader conditional and the form after it fold into one atom, \
             prefix attached"
        );
        assert!(!is_string_literal(argument));
        // The control: the same argument without the reader conditional.
        let parsed = tree("(error 'my-error \"boom\")");
        let call = &parsed.root_view().children[0];
        assert!(is_string_literal(&call.children[2]));
    }

    #[test]
    fn a_warning_subtype_is_a_warning_and_not_serious() {
        let parsed = tree("(define-condition my-note (warning) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.is_warning_and_not_serious("my-note"));
    }

    #[test]
    fn a_style_warning_subtype_is_reached_through_two_standard_edges() {
        let parsed = tree("(define-condition my-note (style-warning) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.is_warning_and_not_serious("my-note"));
    }

    #[test]
    fn an_error_subtype_is_not_a_warning() {
        let parsed = tree("(define-condition my-error (error) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.is_warning_and_not_serious("my-error"));
    }

    /// Multiple inheritance is legal for condition classes, and a class that is
    /// both is a serious condition. Asking only about `warning` would report it.
    #[test]
    fn a_class_inheriting_from_both_warning_and_error_is_not_reported() {
        let parsed = tree("(define-condition odd (warning error) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.is_warning_and_not_serious("odd"));
    }

    #[test]
    fn inheritance_follows_the_files_own_definitions() {
        let parsed =
            tree("(define-condition base (warning) ())\n(define-condition leaf (base) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(hierarchy.is_warning_and_not_serious("leaf"));
    }

    #[test]
    fn a_cyclic_definition_terminates_instead_of_hanging() {
        let parsed = tree("(define-condition a (b) ())\n(define-condition b (a) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        let mut ancestry = hierarchy.ancestry("a");
        ancestry.sort_unstable();
        assert_eq!(ancestry, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn a_definition_inside_quoted_data_is_not_part_of_the_hierarchy() {
        let parsed = tree("'(define-condition my-note (warning) ())");
        let hierarchy = ConditionHierarchy::collect(&parsed);
        assert!(!hierarchy.declares("my-note"));
    }

    #[test]
    fn the_walk_guard_answers_yes_only_for_the_spelling() {
        assert!(mentions_define_condition("(define-condition e (error) ())"));
        assert!(mentions_define_condition("(DEFINE-CONDITION e (error) ())"));
        assert!(mentions_define_condition(
            "(cl:define-condition e (error) ())"
        ));
        assert!(!mentions_define_condition("(signal 'e)"));
        assert!(!mentions_define_condition(""));
    }

    #[test]
    fn a_lazy_hierarchy_reads_the_file_once_and_reuses_it() {
        let parsed = tree("(define-condition my-note (warning) ())");
        let lazy = LazyHierarchy::new(&parsed);
        let first: *const ConditionHierarchy = lazy.get();
        let second: *const ConditionHierarchy = lazy.get();
        assert!(std::ptr::eq(first, second));
        assert!(lazy.get().declares("my-note"));
    }
}
