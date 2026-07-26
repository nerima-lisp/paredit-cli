//! Deciding which bindings carry a constant value.

use std::collections::HashMap;

use crate::domain::common_lisp::common_lisp_operator_head_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SymbolName, SyntaxTree};
use crate::domain::view_query::list_head;

use crate::domain::semantics::binding::{BindingId, BindingKind, BindingTable};

use super::super::model::{ValueTable, ValueTableBuilder};
use super::folding::evaluate_constant;

/// How many times to re-scan before giving up on learning anything new.
///
/// Each round can only *add* values, so the fixpoint is monotone and reached
/// in as many rounds as the longest `let*` chain is deep. The cap is a
/// backstop against a pathological file, not an expected limit; stopping early
/// only loses deductions, it never produces a wrong one.
///
/// It is only a backstop: the loop already stops when a round learns nothing
/// or when every target is resolved, and a file needs a second round at all
/// only if one initial form depends on another.
const MAX_ROUNDS: usize = 8;

/// Builds the value table for one file, on top of its binding table.
///
/// Only Common Lisp is analysed. Another dialect gets an empty table rather
/// than one built from borrowed CLHS semantics.
pub fn build_value_table(
    dialect: Dialect,
    tree: &SyntaxTree,
    bindings: &BindingTable,
) -> ValueTable {
    let mut builder = ValueTableBuilder::new();
    if dialect != Dialect::CommonLisp {
        return builder.finish();
    }

    let roots = root_forms(tree);
    // File-level constants first: a binding's initial form may reference one.
    collect_constants(dialect, &roots, bindings, &mut builder);

    // A `let*` initial form can reference a binding resolved in an earlier
    // round, so keep re-scanning until a round proves nothing new.
    //
    // Two exits, and the cheap one matters most. A round costs a full walk of
    // every root plus a clone of the table so far, so the loop stops the
    // moment there is nothing left to learn *about* — not one round later,
    // once a walk has confirmed it. Most files leave the loop before the
    // first walk (no propagatable binding at all) or after it (every target
    // resolved at once); only a genuine `let*` chain needs a second.
    let targets = propagation_targets(bindings);
    let mut resolved = 0;
    for _ in 0..MAX_ROUNDS {
        if resolved == targets.len() {
            break;
        }
        let before = builder.snapshot();
        let mut learned = 0;
        for root in &roots {
            learned += propagate_in(dialect, root, &targets, bindings, &before, &mut builder);
        }
        if learned == 0 {
            break;
        }
        resolved += learned;
    }

    builder.finish()
}

/// The top-level forms, as owned views.
fn root_forms(tree: &SyntaxTree) -> Vec<ExpressionView> {
    (0..tree.root_children().len())
        .filter_map(|index| {
            tree.select_path(&SexprPath::root_child(index))
                .ok()
                .map(|selection| selection.view())
        })
        .collect()
}

/// The initial-form span of every binding a value may travel through, mapped
/// to the binding it would reach.
///
/// The four conditions [`crate::domain::semantics::binding::Binding`] checks —
/// never reassigned, no opaque region in scope, lexical, and initialized — are
/// applied here rather than after evaluation, so an initial form that could
/// never be propagated is not even evaluated.
fn propagation_targets(bindings: &BindingTable) -> HashMap<ByteSpan, BindingId> {
    bindings
        .bindings()
        .filter(|(_, binding)| binding.kind() == BindingKind::Variable && binding.is_propagatable())
        .filter_map(|(id, binding)| binding.init_form().map(|span| (span, id)))
        .collect()
}

/// Records the value of every initial form in `view`'s subtree that resolves
/// to a target, returning how many bindings were newly resolved.
///
/// A count rather than a flag so the caller can tell "this round learned
/// something" from "there is nothing left to learn", which is what lets it
/// skip the confirming round entirely.
fn propagate_in(
    dialect: Dialect,
    view: &ExpressionView,
    targets: &HashMap<ByteSpan, BindingId>,
    bindings: &BindingTable,
    known: &ValueTable,
    builder: &mut ValueTableBuilder,
) -> usize {
    let mut learned = 0;
    let mut stack = vec![view];

    while let Some(node) = stack.pop() {
        let unresolved = targets
            .get(&node.span)
            .filter(|binding| known.binding_value(**binding).is_none());
        if let Some(binding) = unresolved {
            let value = evaluate_constant(dialect, node, bindings, known)
                .as_known()
                .and_then(super::super::model::LiteralValue::propagatable);
            if let Some(value) = value {
                builder.set_binding_value(*binding, value);
                learned += 1;
            }
        }
        stack.extend(node.children.iter().rev());
    }

    learned
}

/// Records the file's `defconstant`s.
///
/// A definition whose value is not provably constant poisons the name instead
/// of being skipped: leaving it merely absent would let a *second*
/// `defconstant` of the same name look like the only one and be trusted.
fn collect_constants(
    dialect: Dialect,
    roots: &[ExpressionView],
    bindings: &BindingTable,
    builder: &mut ValueTableBuilder,
) {
    let empty = ValueTable::default();
    for root in roots {
        let Some(head) = list_head(root) else {
            continue;
        };
        if !common_lisp_operator_head_eq(head, "defconstant") {
            continue;
        }
        let Some(name) = root
            .children
            .get(1)
            .and_then(atom_symbol_text)
            .and_then(|text| SymbolName::new(text).ok())
        else {
            continue;
        };
        let value = root
            .children
            .get(2)
            .map(|form| evaluate_constant(dialect, form, bindings, &empty))
            .and_then(|value| {
                value
                    .as_known()
                    .and_then(super::super::model::LiteralValue::propagatable)
            });
        match value {
            Some(value) => builder.define_constant(name, value),
            None => builder.poison_constant(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::semantics::NodeKey;
    use crate::domain::semantics::binding::build_binding_table;
    use crate::domain::semantics::value::model::{LiteralValue, PropagatableValue, Value};

    struct Analysis {
        tree: SyntaxTree,
        bindings: BindingTable,
        values: ValueTable,
        source: String,
    }

    fn analyze(input: &str) -> Analysis {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let bindings = build_binding_table(Dialect::CommonLisp, &tree, input);
        let values = build_value_table(Dialect::CommonLisp, &tree, &bindings);
        Analysis {
            tree,
            bindings,
            values,
            source: input.to_owned(),
        }
    }

    /// The value the analysis gives the occurrence of `symbol` starting at the
    /// LAST position it appears — the use site, after every binding form.
    fn value_at_last_use(analysis: &Analysis, symbol: &str) -> Value {
        let offset = analysis
            .source
            .rfind(symbol)
            .unwrap_or_else(|| panic!("{symbol} does not occur"));
        let span = ByteSpan::new(
            crate::domain::sexpr::ByteOffset::new(offset),
            crate::domain::sexpr::ByteOffset::new(offset + symbol.len()),
        );
        analysis
            .bindings
            .resolve(NodeKey::atom(span))
            .and_then(|binding| analysis.values.binding_value(binding).cloned())
            .map_or(Value::Unknown, |value| Value::Known(value.into()))
    }

    #[test]
    fn a_plain_let_binding_carries_its_constant() {
        let analysis = analyze("(let ((z 0)) (/ x z))");
        assert_eq!(
            value_at_last_use(&analysis, "z"),
            Value::Known(LiteralValue::Integer(0))
        );
    }

    #[test]
    fn a_folded_initial_form_carries_its_result() {
        let analysis = analyze("(let ((z (- 1 1))) (/ x z))");
        assert_eq!(
            value_at_last_use(&analysis, "z"),
            Value::Known(LiteralValue::Integer(0))
        );
    }

    #[test]
    fn a_reassigned_binding_carries_nothing() {
        // The value at the use site is whatever the `setq` last wrote, which
        // this layer does not track.
        let analysis = analyze("(let ((z 0)) (setq z 1) (/ x z))");
        assert_eq!(value_at_last_use(&analysis, "z"), Value::Unknown);
    }

    #[test]
    fn a_non_constant_initial_form_carries_nothing() {
        let analysis = analyze("(let ((z (read))) (/ x z))");
        assert_eq!(value_at_last_use(&analysis, "z"), Value::Unknown);
    }

    #[test]
    fn a_let_star_binding_sees_the_one_before_it() {
        let analysis = analyze("(let* ((a 2) (b (* a 3))) (list b))");
        assert_eq!(
            value_at_last_use(&analysis, "b"),
            Value::Known(LiteralValue::Integer(6))
        );
    }

    #[test]
    fn a_parallel_let_binding_does_not_see_its_sibling() {
        // `(let ((a 2) (b (* a 3))) …)` evaluates `(* a 3)` in the *outer*
        // scope, where `a` is whatever it was before — not 2.
        let analysis = analyze("(let ((a 2) (b (* a 3))) (list b))");
        assert_eq!(value_at_last_use(&analysis, "b"), Value::Unknown);
    }

    #[test]
    fn a_shadowing_binding_wins_at_its_own_use_site() {
        let analysis = analyze("(let ((z 1)) (let ((z 0)) (/ x z)))");
        assert_eq!(
            value_at_last_use(&analysis, "z"),
            Value::Known(LiteralValue::Integer(0))
        );
    }

    #[test]
    fn a_string_initial_form_is_read_but_not_propagated() {
        // A Common Lisp string is mutable through `(setf (char s 0) …)`, so
        // substituting its contents would lie about identity.
        let analysis = analyze(r#"(let ((s "text")) (list s))"#);
        assert_eq!(value_at_last_use(&analysis, "s"), Value::Unknown);
    }

    #[test]
    fn a_float_initial_form_is_read_but_not_propagated() {
        let analysis = analyze("(let ((f 1.5)) (list f))");
        assert_eq!(value_at_last_use(&analysis, "f"), Value::Unknown);
    }

    #[test]
    fn a_uniquely_defined_file_constant_resolves() {
        let analysis = analyze("(defconstant +limit+ 10)");
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("+limit+").expect("symbol")),
            Some(&PropagatableValue::Integer(10))
        );
    }

    #[test]
    fn two_definitions_of_one_constant_resolve_to_nothing() {
        let analysis = analyze("(defconstant +limit+ 10)(defconstant +limit+ 20)");
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("+limit+").expect("symbol")),
            None
        );
    }

    #[test]
    fn a_constant_with_an_unprovable_value_resolves_to_nothing() {
        let analysis = analyze("(defconstant +limit+ (read))");
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("+limit+").expect("symbol")),
            None
        );
    }

    #[test]
    fn a_non_common_lisp_file_gets_an_empty_table() {
        let input = "(let [x 1] x)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse");
        let bindings = build_binding_table(Dialect::Clojure, &tree, input);
        let values = build_value_table(Dialect::Clojure, &tree, &bindings);
        assert_eq!(values.binding_count(), 0);
        assert_eq!(values.constant_count(), 0);
    }

    #[test]
    fn the_tree_is_never_consulted_for_a_binding_that_cannot_propagate() {
        // A sanity check on the target index: an uninitialized binding has no
        // initial form, so it can never appear as a propagation target.
        let analysis = analyze("(let (z) (/ x z))");
        assert_eq!(analysis.values.binding_count(), 0);
        assert!(analysis.tree.root_children().len() == 1);
    }
}
