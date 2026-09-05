//! A reference-type operator applied to the wrong kind of reference.
//!
//! ```clojure
//! (let [counter (atom 0)]     (swap!  counter inc))   ; correct
//! (let [counter (ref 0)]      (swap!  counter inc))   ; ClassCastException
//! (let [counter (volatile! 0)] (swap! counter inc))   ; ClassCastException
//! ```
//!
//! # The premise, read off `clojure/core.clj`
//!
//! Clojure has four mutable reference containers and each has its own
//! operators, type-checked by the host rather than by a protocol dispatch:
//!
//! | constructor | operators | first parameter |
//! | --- | --- | --- |
//! | `(atom …)` | `swap!`, `swap-vals!`, `reset!`, `reset-vals!`, `compare-and-set!` | `clojure.lang.IAtom` |
//! | `(ref …)` | `alter`, `commute`, `ref-set`, `ensure` | `clojure.lang.Ref` |
//! | `(agent …)` | `send`, `send-off`, `send-via` | `clojure.lang.Agent` |
//! | `(volatile! …)` | `vswap!`, `vreset!` | `clojure.lang.Volatile` |
//!
//! Crossing them is a `ClassCastException` at the first call, not a wrong
//! answer — `(ref-set an-atom v)` cannot cast `clojure.lang.Atom` to
//! `clojure.lang.Ref`. Nothing rejects it earlier: the argument is untyped at
//! the call site and `clojure.core`'s own type hints are erased.
//!
//! `volatile!` is where this is least obvious. It looks like an atom, it is
//! spelled like an atom, it is what a stateful transducer holds — and
//! `swap!` on one throws.
//!
//! # What it looks at
//!
//! A **lexical binding** whose init expression is a literal call to one of the
//! four constructors, and then the body of that binding form. Nothing else:
//! a top-level `(def counter (atom 0))` used from another form would need a
//! per-file index correlating two top-level forms, which is the quadratic
//! shape this package refuses (see the README).
//!
//! # Shadowing
//!
//! The walk **prunes at any form that rebinds a tracked name**, including
//! destructuring positions and function parameters:
//!
//! ```clojure
//! (let [a (atom 0)]
//!   (let [a (ref 0)]
//!     (alter a inc)))     ; correct, and not reported
//! ```
//!
//! Pruning the whole subtree is coarser than tracking a shadowed name through
//! it, and coarser in the safe direction: a rebound name yields false
//! negatives, never a claim about the wrong binding. This is the guard the
//! sibling packages learned to write after an autofix deleted code under a
//! `flet` that shadowed a matched name.
//!
//! # What it does not attempt
//!
//! - **`send`, `send-off` and `send-via`.** They are the agent operators, but
//!   `send` is also an ordinary name for a user function over a connection or
//!   a socket, and this rule cannot tell them apart —
//!   `(let [conn (atom nil)] (send conn "hi"))` is correct code under any
//!   number of libraries. An agent is therefore only detected here as the
//!   *target* of an atom, ref or volatile operator. A deliberate false
//!   negative; see [`crate::support::REFERENCE_OPERATORS`].
//! - **`deref`/`@`, `add-watch`, `set-validator!`, `alter-meta!`** — every one
//!   of them works on every reference kind.
//! - **Later init expressions.** `(let [a (atom 0) b (alter a inc)] …)` binds
//!   `b` from a defect this rule does not look at; only the body is walked.
//! - **A reference reached through a function argument or a map.** Invisible.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    ReferenceKind, for_each_evaluated_subview, head_is, is_vector_literal, operator_reference_kind,
    symbol_name,
};

/// The heads [`examine_reference_bindings`] matches.
///
/// The lexical binding forms whose bindings are a `[name init …]` vector at
/// child 1. `binding` and `with-local-vars` are absent because they bind Vars,
/// not locals, and a Var holding an atom is reached through `var-get`.
pub const REFERENCE_BINDING_HEADS: &[&str] = &[
    "if-let",
    "if-some",
    "let",
    "let*",
    "loop",
    "when-let",
    "when-some",
];

/// Forms whose child 1 is a binding vector at whose **even** indices names are
/// introduced.
///
/// Used only by the shadowing check, which is why it is wider than
/// [`REFERENCE_BINDING_HEADS`]: a `doseq` cannot bind an atom usefully, but it
/// can certainly shadow one.
const SHADOWING_BINDING_VECTOR_HEADS: &[&str] = &[
    "binding",
    "doseq",
    "dotimes",
    "for",
    "if-let",
    "if-some",
    "let",
    "let*",
    "loop",
    "when-first",
    "when-let",
    "when-some",
    "with-local-vars",
    "with-open",
    "with-redefs",
];

/// Forms in which **every** vector child is a parameter list.
const SHADOWING_PARAMETER_HEADS: &[&str] = &[
    "defmacro",
    "defmethod",
    "defn",
    "defn-",
    "fn",
    "fn*",
    "letfn",
];

#[derive(Debug, Clone)]
pub struct ReferenceTypeOperatorMismatchItem {
    /// The span of the offending operator call.
    pub span: ByteSpan,
    /// The operator, normalized.
    pub operator: String,
    /// The local name it was applied to.
    pub name: String,
    /// The kind the local actually holds.
    pub actual: ReferenceKind,
    /// The kind the operator requires.
    pub required: ReferenceKind,
}

impl Finding for ReferenceTypeOperatorMismatchItem {
    fn kind(&self) -> &'static str {
        "reference-type-operator-mismatch"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("operator={}", self.operator),
            format!("name={}", self.name),
            format!("actual={}", self.actual.constructor()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("name", json!(self.name)),
            ("actual", json!(self.actual.constructor())),
            ("required", json!(self.required.constructor())),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} requires a {}, and {} is bound to ({} …); this throws ClassCastException — \
             the {} operators are {}",
            self.operator,
            self.required.constructor(),
            self.name,
            self.actual.constructor(),
            self.actual.constructor(),
            self.actual.operators()
        )
    }
}

/// A local name bound to a reference cell, in the innermost binding form that
/// introduced it.
#[derive(Debug, Clone)]
struct ReferenceBinding {
    name: String,
    kind: ReferenceKind,
}

/// Pushes every atom's symbol under `view` into `sink`.
///
/// Used on binding and parameter positions, where a name may be introduced
/// through arbitrarily nested destructuring — `{:keys [a b] :or {a 1}}`. The
/// `:keys`/`:or` keywords land in `sink` too, which costs nothing: a keyword
/// can never equal a tracked symbol name.
fn collect_symbols(view: &ExpressionView, sink: &mut Vec<String>) {
    if view.children.is_empty() {
        if let Some(name) = symbol_name(view) {
            sink.push(name);
        }
        return;
    }
    for child in &view.children {
        collect_symbols(child, sink);
    }
}

/// Every name `view` introduces, or an empty vector for a form that introduces
/// none.
/// A reader lambda needs no case of its own here, and an earlier revision's
/// `if is_reader_lambda(view) { return Vec::new(); }` was removed by mutation
/// testing: deleting it changed no test, because `#(…)` carries no parameter
/// vector at all — its own `list_head` is whatever its *body* starts with, so
/// it can only reach these branches when that body is itself a binding form,
/// and `#(let [a 1] (alter a %))` does shadow `a`. The guard was dead where it
/// was reachable and wrong where it was not.
fn names_bound_by(view: &ExpressionView) -> Vec<String> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    if symbol_in(head, SHADOWING_BINDING_VECTOR_HEADS) {
        if let Some(bindings) = view
            .children
            .get(1)
            .filter(|child| is_vector_literal(child))
        {
            // Even indices only: the odd ones are init *expressions*, and
            // reading them as names would prune every body that merely
            // mentions the reference.
            for (index, child) in bindings.children.iter().enumerate() {
                if index % 2 == 0 {
                    collect_symbols(child, &mut names);
                }
            }
        }
    } else if symbol_in(head, SHADOWING_PARAMETER_HEADS) {
        for child in view.children.iter().skip(1) {
            if is_vector_literal(child) {
                collect_symbols(child, &mut names);
            } else if child.children.iter().any(is_vector_literal) {
                // A multi-arity `(fn ([a] …) ([a b] …))` or a `letfn` entry.
                for grandchild in child.children.iter().filter(|g| is_vector_literal(g)) {
                    collect_symbols(grandchild, &mut names);
                }
            }
        }
    } else if symbol_in(head, &["catch"]) {
        // `(catch ExceptionType e & body)`.
        if let Some(binding) = view.children.get(2) {
            collect_symbols(binding, &mut names);
        }
    }
    names
}

/// The reference cells a binding vector introduces, last binding of each name
/// winning.
fn reference_bindings_of(bindings: &ExpressionView) -> Vec<ReferenceBinding> {
    let mut found: Vec<ReferenceBinding> = Vec::new();
    let mut index = 0;
    while index + 1 < bindings.children.len() {
        let name = &bindings.children[index];
        let init = &bindings.children[index + 1];
        index += 2;

        let (Some(name), Some(head)) = (symbol_name(name), list_head(init)) else {
            continue;
        };
        let Some(kind) = ReferenceKind::of_constructor(head) else {
            continue;
        };
        found.retain(|existing| existing.name != name);
        found.push(ReferenceBinding { name, kind });
    }
    found
}

pub fn examine_reference_bindings(
    view: &ExpressionView,
    reference_binding_count: &mut usize,
    violations: &mut Vec<ReferenceTypeOperatorMismatchItem>,
) {
    if !head_is(view, REFERENCE_BINDING_HEADS) {
        return;
    }
    // The cheap pre-filter, and the reason a head list this common is
    // affordable: a `let` with no reference constructor in its binding vector
    // costs one delimiter test plus one `list_head` per init and allocates
    // nothing.
    let Some(bindings) = view
        .children
        .get(1)
        .filter(|child| is_vector_literal(child))
    else {
        return;
    };
    let tracked = reference_bindings_of(bindings);
    if tracked.is_empty() {
        return;
    }
    *reference_binding_count += tracked.len();

    for body in view.children.iter().skip(2) {
        walk_body(body, &tracked, violations);
    }
}

fn walk_body(
    root: &ExpressionView,
    tracked: &[ReferenceBinding],
    violations: &mut Vec<ReferenceTypeOperatorMismatchItem>,
) {
    let mut stack = vec![root];
    while let Some(view) = stack.pop() {
        // A form that rebinds any tracked name takes its whole subtree out of
        // scope. Coarse on purpose: over-pruning is a false negative, and
        // under-pruning is a claim about the wrong binding.
        let shadowed = names_bound_by(view);
        if !shadowed.is_empty()
            && tracked
                .iter()
                .any(|binding| shadowed.contains(&binding.name))
        {
            continue;
        }

        if let Some(item) = mismatch_at(view, tracked) {
            violations.push(item);
        }

        for child in view.children.iter().rev() {
            if child.children.is_empty() {
                continue;
            }
            stack.push(child);
        }
    }
}

/// The finding `view` is, if it is an operator call on a tracked name of the
/// wrong kind.
fn mismatch_at(
    view: &ExpressionView,
    tracked: &[ReferenceBinding],
) -> Option<ReferenceTypeOperatorMismatchItem> {
    let head = list_head(view)?;
    let required = operator_reference_kind(head)?;
    let target = symbol_name(view.children.get(1)?)?;
    let binding = tracked.iter().find(|binding| binding.name == target)?;
    (binding.kind != required).then(|| ReferenceTypeOperatorMismatchItem {
        span: view.span,
        operator: head.to_owned(),
        name: target,
        actual: binding.kind,
        required,
    })
}

/// Collects every crossed reference operator in one file, with the number of
/// locally bound reference cells scanned as the denominator beside them.
pub fn build_reference_type_operator_mismatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ReferenceTypeOperatorMismatchItem>> {
    let mut reference_binding_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::Clojure {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_reference_bindings(view, &mut reference_binding_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::Clojure,
        tree.source(),
        violations,
        vec![("reference_binding_count", json!(reference_binding_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::REFERENCE_OPERATORS;

    fn report(input: &str) -> FileFindings<ReferenceTypeOperatorMismatchItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse input");
        build_reference_type_operator_mismatch_report(
            Path::new("test.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report")
    }

    fn operators(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.operator)
            .collect()
    }

    // --- the defect ----------------------------------------------------------

    #[test]
    fn flags_an_atom_operator_on_a_ref_and_the_reverse() {
        assert_eq!(operators("(let [r (ref 0)] (swap! r inc))"), vec!["swap!"]);
        assert_eq!(operators("(let [r (ref 0)] (reset! r 1))"), vec!["reset!"]);
        assert_eq!(
            operators("(let [a (atom 0)] (ref-set a 1))"),
            vec!["ref-set"]
        );
        assert_eq!(operators("(let [a (atom 0)] (alter a inc))"), vec!["alter"]);
        assert_eq!(
            operators("(let [a (atom 0)] (commute a inc))"),
            vec!["commute"]
        );
    }

    /// The case a reader is least likely to catch by eye: a volatile looks and
    /// reads like an atom.
    #[test]
    fn flags_an_atom_operator_on_a_volatile_and_the_reverse() {
        assert_eq!(
            operators("(let [v (volatile! 0)] (swap! v inc))"),
            vec!["swap!"]
        );
        assert_eq!(
            operators("(let [a (atom 0)] (vswap! a inc))"),
            vec!["vswap!"]
        );
        assert_eq!(
            operators("(let [a (atom 0)] (vreset! a 1))"),
            vec!["vreset!"]
        );
    }

    #[test]
    fn flags_an_atom_or_ref_operator_on_an_agent() {
        assert_eq!(
            operators("(let [a (agent 0)] (swap! a inc))"),
            vec!["swap!"]
        );
        assert_eq!(
            operators("(let [a (agent 0)] (alter a inc))"),
            vec!["alter"]
        );
    }

    #[test]
    fn flags_the_defect_in_every_lexical_binding_form() {
        for source in [
            "(let [r (ref 0)] (swap! r inc))",
            "(let* [r (ref 0)] (swap! r inc))",
            "(loop [r (ref 0)] (swap! r inc))",
            "(when-let [r (ref 0)] (swap! r inc))",
            "(if-let [r (ref 0)] (swap! r inc) nil)",
            "(when-some [r (ref 0)] (swap! r inc))",
            "(if-some [r (ref 0)] (swap! r inc) nil)",
        ] {
            assert_eq!(operators(source), vec!["swap!".to_owned()], "{source}");
        }
    }

    #[test]
    fn flags_the_defect_however_deeply_it_is_nested_in_the_body() {
        assert_eq!(
            operators("(let [r (ref 0)] (when ready? (doseq [x xs] (swap! r + x))))"),
            vec!["swap!"]
        );
        assert_eq!(
            operators("(let [r (ref 0)] (map #(swap! r + %) xs))"),
            vec!["swap!"]
        );
    }

    #[test]
    fn the_finding_names_both_kinds() {
        let finding = &report("(let [v (volatile! 0)] (swap! v inc))").findings[0];
        assert_eq!(finding.actual, ReferenceKind::Volatile);
        assert_eq!(finding.required, ReferenceKind::Atom);
        assert_eq!(finding.name, "v");
    }

    // --- realistic, correct Clojure that must stay silent --------------------

    #[test]
    fn does_not_flag_an_operator_matched_to_its_own_reference_kind() {
        for source in [
            "(let [a (atom 0)] (swap! a inc))",
            "(let [a (atom {})] (reset! a {:k 1}))",
            "(let [a (atom 0)] (compare-and-set! a 0 1))",
            "(let [a (atom 0)] (swap-vals! a inc))",
            "(let [r (ref 0)] (dosync (alter r inc)))",
            "(let [r (ref 0)] (dosync (ref-set r 1)))",
            "(let [r (ref 0)] (dosync (commute r + 1)))",
            "(let [r (ref 0)] (dosync (ensure r)))",
            "(let [v (volatile! 0)] (vswap! v inc))",
            "(let [v (volatile! nil)] (vreset! v 1))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
        }
    }

    /// Every reference kind answers `deref`, `add-watch` and friends, so none
    /// of them is an operator this rule knows about.
    #[test]
    fn does_not_flag_the_kind_agnostic_operations() {
        for source in [
            "(let [r (ref 0)] @r)",
            "(let [r (ref 0)] (deref r))",
            "(let [v (volatile! 0)] @v)",
            "(let [a (atom 0)] (add-watch a :k f))",
            "(let [a (atom 0)] (set-validator! a pos?))",
            "(let [a (atom 0)] (alter-meta! a assoc :k 1))",
            "(let [a (atom 0)] (alter-var-root #'x inc))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
        }
    }

    /// `send` is an ordinary name for a user function; an agent is only ever
    /// detected here as the *target* of another kind's operator. Pinned so the
    /// false negative is a decision, not a bug.
    #[test]
    fn the_agent_operators_are_a_documented_false_negative() {
        assert!(operators("(let [a (atom 0)] (send a inc))").is_empty());
        assert!(operators("(let [a (atom 0)] (send-off a inc))").is_empty());
        assert!(operators("(let [conn (atom nil)] (send conn \"hi\"))").is_empty());
    }

    #[test]
    fn does_not_flag_a_binding_whose_init_is_not_a_constructor_call() {
        for source in [
            "(let [a (make-counter)] (swap! a inc))",
            "(let [a state] (alter a inc))",
            "(let [a (:counter m)] (ref-set a 1))",
            "(let [a (delay 0)] (swap! a inc))",
            "(let [a (promise)] (swap! a inc))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
            assert_eq!(
                report(source).summary,
                vec![("reference_binding_count", json!(0))],
                "{source}"
            );
        }
    }

    #[test]
    fn does_not_flag_an_operator_on_an_untracked_name() {
        assert!(operators("(let [a (atom 0)] (alter other inc))").is_empty());
    }

    /// A binding position that is not a `[…]` vector is not a Clojure binding
    /// vector, and reading its children as `name init` pairs would invent
    /// bindings. The realistic source of one is Common Lisp pasted into a
    /// `.clj` file; a map literal is the other shape that pairs up cleanly.
    ///
    /// Found by mutation testing: dropping the `is_vector_literal` filter left
    /// every other test in this package green.
    #[test]
    fn a_binding_position_that_is_not_a_vector_binds_nothing() {
        for source in [
            "(let (a (atom 0)) (alter a inc))",
            "(let {a (atom 0)} (alter a inc))",
            "(when-let (a (atom 0)) (alter a inc))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
            assert_eq!(
                report(source).summary,
                vec![("reference_binding_count", json!(0))],
                "{source}"
            );
        }
    }

    // --- shadowing -----------------------------------------------------------

    /// The guard that keeps this rule from making a claim about the wrong
    /// binding.
    #[test]
    fn a_rebound_name_takes_its_whole_subtree_out_of_scope() {
        for source in [
            "(let [a (atom 0)] (let [a (ref 0)] (alter a inc)))",
            "(let [a (atom 0)] (fn [a] (alter a inc)))",
            "(let [a (atom 0)] (defn f [a] (alter a inc)))",
            "(let [a (atom 0)] (loop [a (ref 0)] (alter a inc)))",
            "(let [a (atom 0)] (doseq [a refs] (alter a inc)))",
            "(let [a (atom 0)] (for [a refs] (alter a inc)))",
            "(let [a (atom 0)] (letfn [(g [a] (alter a inc))] (g nil)))",
            "(let [a (atom 0)] (try (f) (catch Exception a (alter a inc))))",
            "(let [a (atom 0)] (let [{a :r} m] (alter a inc)))",
            "(let [a (atom 0)] (fn [[a b]] (alter a inc)))",
        ] {
            assert!(operators(source).is_empty(), "{source}");
        }
    }

    /// The other half: a *sibling* binding form that does not rebind the name
    /// must not silence the rule, and neither must a mention of the name in an
    /// inner `let`'s init expression.
    #[test]
    fn an_inner_binding_that_does_not_shadow_leaves_the_rule_alone() {
        assert_eq!(
            operators("(let [a (atom 0)] (let [b 1] (alter a b)))"),
            vec!["alter"]
        );
        assert_eq!(
            operators("(let [a (atom 0)] (let [b @a] (alter a inc)))"),
            vec!["alter"]
        );
        assert_eq!(
            operators("(let [a (atom 0)] (fn [x] (alter a x)))"),
            vec!["alter"]
        );
    }

    /// A reader lambda's parameters are `%`, `%1`, `%&`, so it can shadow
    /// nothing a `let` bound — but a binding form *inside* one still can, and
    /// an early `is_reader_lambda` return in `names_bound_by` got that wrong.
    /// Found by mutation testing; see that function's comment.
    #[test]
    fn a_reader_lambda_shadows_nothing_but_a_binding_form_inside_one_does() {
        assert_eq!(
            operators("(let [a (atom 0)] (map #(alter a %) xs))"),
            vec!["alter"]
        );
        assert!(
            operators("(let [a (atom 0)] (map #(let [a (ref 0)] (alter a %)) xs))").is_empty(),
            "an inner let inside a reader lambda still shadows"
        );
    }

    /// The last binding of a name wins, exactly as `let` does.
    #[test]
    fn a_name_rebound_within_one_binding_vector_takes_its_last_value() {
        assert!(operators("(let [a (ref 0) a (atom 0)] (swap! a inc))").is_empty());
        assert_eq!(
            operators("(let [a (atom 0) a (ref 0)] (swap! a inc))"),
            vec!["swap!"]
        );
        assert_eq!(
            report("(let [a (atom 0) a (ref 0)] (swap! a inc))").summary,
            vec![("reference_binding_count", json!(1))]
        );
    }

    // --- reader-syntax negatives ---------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(operators("'(let [r (ref 0)] (swap! r inc))").is_empty());
        assert!(operators("`(let [r (ref 0)] (swap! r inc))").is_empty());
        assert!(operators("(quote (let [r (ref 0)] (swap! r inc)))").is_empty());
    }

    #[test]
    fn a_comma_is_whitespace_in_clojure_so_the_form_stays_data() {
        assert!(operators("'(a ,(let [r (ref 0)] (swap! r inc)))").is_empty());
        assert!(operators("`(a ,(let [r (ref 0)] (swap! r inc)))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            operators("`(do ~(let [r (ref 0)] (swap! r inc)))"),
            vec!["swap!"]
        );
    }

    #[test]
    fn a_comment_body_is_never_flagged() {
        assert!(operators("(comment (let [r (ref 0)] (swap! r inc)))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(operators("(println \"(let [r (ref 0)] (swap! r inc))\")").is_empty());
    }

    // --- envelope ------------------------------------------------------------

    #[test]
    fn the_summary_counts_every_reference_cell_scanned() {
        let report = report(
            "(let [a (atom 0) r (ref 0) n 1] (swap! a inc) (dosync (alter r inc)))\n\
             (let [x 1] x)\n",
        );
        assert_eq!(report.summary, vec![("reference_binding_count", json!(2))]);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_columns() {
        let report =
            report("(defn tick [n]\n  (let [counter (volatile! 0)]\n    (swap! counter + n)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "reference-type-operator-mismatch");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("swap!")),
                ("name", json!("counter")),
                ("actual", json!("volatile!")),
                ("required", json!("atom")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "operator=swap!".to_owned(),
                "name=counter".to_owned(),
                "actual=volatile!".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "swap! requires a atom, and counter is bound to (volatile! …); \
             this throws ClassCastException — the volatile! operators are vswap!/vreset!"
        );
    }

    /// Every operator this rule reports must belong to a kind that has a
    /// constructor, or the message would name a repair that does not exist.
    #[test]
    fn every_operator_belongs_to_a_constructible_kind() {
        for (operator, kind) in REFERENCE_OPERATORS {
            assert_eq!(operator_reference_kind(operator), Some(*kind));
            assert_eq!(
                ReferenceKind::of_constructor(kind.constructor()),
                Some(*kind)
            );
        }
    }

    /// Every head this rule anchors on must also be a form the shadowing check
    /// understands, or an inner rebinding of the same shape would be missed.
    #[test]
    fn every_anchor_head_is_also_a_shadowing_head() {
        for head in REFERENCE_BINDING_HEADS {
            assert!(
                SHADOWING_BINDING_VECTOR_HEADS.contains(head),
                "{head} anchors the rule but cannot shadow"
            );
        }
    }

    #[test]
    fn a_non_clojure_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let ((a 1)) a)", Dialect::CommonLisp).expect("parse");
        let report = build_reference_type_operator_mismatch_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("reference_binding_count", json!(0))]);
    }

    #[test]
    fn a_clojure_file_is_reported_as_modelled() {
        assert!(report("(let [a (atom 0)] (swap! a inc))").dialect_modelled);
    }
}
