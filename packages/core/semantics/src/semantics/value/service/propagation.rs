//! Deciding which bindings carry a constant value.

use std::collections::{HashMap, HashSet};

use crate::semantics::project::model::PackageId;
use crate::semantics::project::service::{FilePackages, PackageRegion};
use crate::semantics::project::{GlobalTable, QualifiedSymbol};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, SymbolName, SyntaxTree,
};
use paredit_core_syntax::view_query::list_head;

use crate::semantics::binding::{BindingId, BindingKind, BindingTable};

use super::super::model::{ValueTable, ValueTableBuilder};
use super::super::policy::supports_value_propagation;
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
/// Common Lisp and Emacs Lisp are analysed; every other dialect gets an
/// empty table rather than one built from borrowed semantics.
#[must_use]
pub fn build_value_table(
    dialect: Dialect,
    tree: &SyntaxTree,
    bindings: &BindingTable,
) -> ValueTable {
    build_value_table_in_project(dialect, tree, bindings, None)
}

/// The same, with a project context to fall back on for constants this file
/// does not define.
///
/// A `defconstant` is the only definition whose value provably cannot differ
/// between where it is written and where it is read, which is what lets it
/// cross a file boundary at all. `project` supplies the ones the file's own
/// package makes visible; everything else about the file is unchanged.
///
/// `None` reproduces [`build_value_table`] exactly, which is what every
/// single-file caller passes.
#[must_use]
pub fn build_value_table_in_project(
    dialect: Dialect,
    tree: &SyntaxTree,
    bindings: &BindingTable,
    project: Option<&ProjectConstants<'_>>,
) -> ValueTable {
    let mut builder = ValueTableBuilder::new();
    if !supports_value_propagation(dialect) {
        return builder.finish();
    }

    let roots = root_forms(tree);
    // File-level constants first: a binding's initial form may reference one.
    collect_constants(dialect, &roots, bindings, &mut builder);

    // Then the project's, and only into the gaps. The file's own answer is
    // the more local and more certain one, and a project constant that
    // overwrote it — or that arrived first and made the file's own
    // `defconstant` look like a duplicate — would be strictly worse.
    if let Some(project) = project {
        project.fill(&mut builder);
    }

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

/// The project's constants, seen from inside one file.
///
/// Visibility is deliberately the narrowest rule that is provable: a constant
/// is filled in only when its home package is one this file is *in*. A file
/// with no `in-package` is in no package this layer can name and receives
/// nothing, which is exactly the behaviour it had before the project layer
/// existed.
///
/// `use-package` inheritance is not modelled. A name inherited from another
/// package is genuinely visible unqualified, but proving that needs the
/// `defpackage` graph, and a constant this misses stays `Unknown` — a lost
/// deduction rather than a wrong one.
#[derive(Debug, Clone, Copy)]
pub struct ProjectConstants<'a> {
    globals: &'a GlobalTable,
    packages: &'a FilePackages,
}

impl<'a> ProjectConstants<'a> {
    #[must_use]
    pub const fn new(globals: &'a GlobalTable, packages: &'a FilePackages) -> Self {
        Self { globals, packages }
    }

    fn fill(&self, builder: &mut ValueTableBuilder) {
        let visible: HashSet<&PackageId> = self
            .packages
            .regions()
            .iter()
            .map(PackageRegion::package)
            .collect();
        if visible.is_empty() {
            return;
        }

        for (symbol, value) in self.globals.constants() {
            if visible.contains(symbol.package()) {
                builder.fill_missing_constant(symbol.name().clone(), value.clone());
            }
        }
    }
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
/// The four conditions [`crate::semantics::binding::Binding`] checks —
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

/// Whether `head` names the dialect's own file-level constant form:
/// `defconstant` for Common Lisp, `defconst`/`defcustom` for Emacs Lisp.
///
/// `defcustom` sits beside `defconst` deliberately: both declare a name and
/// an initial-value expression at the same child position, and a user option
/// is exactly as constant as any other name until something reassigns it —
/// which is the same provability question a plain `defconst` answers, and
/// [`crate::semantics::binding`] already excludes a reassigned name from
/// [`propagation_targets`] the same way it does for `defconstant`.
fn is_constant_definition_head(dialect: Dialect, head: &str) -> bool {
    match dialect {
        Dialect::CommonLisp => common_lisp_operator_head_eq(head, "defconstant"),
        Dialect::EmacsLisp => matches!(head, "defconst" | "defcustom"),
        _ => false,
    }
}

/// Records the file's dialect-appropriate file-level constants.
///
/// A definition whose value is not provably constant poisons the name instead
/// of being skipped: leaving it merely absent would let a *second* definition
/// of the same name look like the only one and be trusted.
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
        if !is_constant_definition_head(dialect, head) {
            continue;
        }
        let Some(name) = root
            .children
            .get(1)
            .and_then(atom_symbol_text)
            .and_then(|text| super::folding::constant_key(dialect, text))
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
    use crate::semantics::NodeKey;
    use crate::semantics::binding::build_binding_table;
    use crate::semantics::value::model::{LiteralValue, PropagatableValue, Value};

    struct Analysis {
        tree: SyntaxTree,
        bindings: BindingTable,
        values: ValueTable,
        source: String,
    }

    fn analyze(input: &str) -> Analysis {
        analyze_as(input, Dialect::CommonLisp)
    }

    fn analyze_as(input: &str, dialect: Dialect) -> Analysis {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let bindings = build_binding_table(dialect, &tree, input);
        let values = build_value_table(dialect, &tree, &bindings);
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
            paredit_core_syntax::sexpr::ByteOffset::new(offset),
            paredit_core_syntax::sexpr::ByteOffset::new(offset + symbol.len()),
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
                .constant_value(&SymbolName::new("+LIMIT+").expect("symbol")),
            Some(&PropagatableValue::Integer(10))
        );
    }

    #[test]
    fn a_constant_referenced_in_another_case_is_the_same_constant() {
        // The reader folds a symbol's case, so these name one constant.
        // Keying the table on the raw spelling made the reference miss.
        let analysis = analyze("(defconstant +limit+ 10)(let ((n +LIMIT+)) (list n))");
        assert_eq!(
            value_at_last_use(&analysis, "n"),
            Value::Known(LiteralValue::Integer(10))
        );
    }

    #[test]
    fn two_definitions_of_one_constant_resolve_to_nothing() {
        let analysis = analyze("(defconstant +limit+ 10)(defconstant +limit+ 20)");
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("+LIMIT+").expect("symbol")),
            None
        );
    }

    #[test]
    fn a_constant_with_an_unprovable_value_resolves_to_nothing() {
        let analysis = analyze("(defconstant +limit+ (read))");
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("+LIMIT+").expect("symbol")),
            None
        );
    }

    #[test]
    fn an_emacs_lisp_defconst_resolves() {
        let analysis = analyze_as("(defconst my-limit 10)", Dialect::EmacsLisp);
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("my-limit").expect("symbol")),
            Some(&PropagatableValue::Integer(10))
        );
    }

    #[test]
    fn an_emacs_lisp_defcustom_resolves_its_initial_value() {
        let analysis = analyze_as(
            "(defcustom my-limit 10 \"doc\" :type 'integer)",
            Dialect::EmacsLisp,
        );
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("my-limit").expect("symbol")),
            Some(&PropagatableValue::Integer(10))
        );
    }

    #[test]
    fn emacs_lisp_constant_lookup_is_case_sensitive() {
        // Unlike Common Lisp, `my-limit` and `MY-LIMIT` are two different
        // Emacs Lisp symbols, so the raw spelling is the key.
        let analysis = analyze_as("(defconst my-limit 10)", Dialect::EmacsLisp);
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("MY-LIMIT").expect("symbol")),
            None
        );
    }

    #[test]
    fn two_emacs_lisp_definitions_of_one_name_resolve_to_nothing() {
        let analysis = analyze_as(
            "(defconst my-limit 10)(defcustom my-limit 20 \"doc\")",
            Dialect::EmacsLisp,
        );
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("my-limit").expect("symbol")),
            None
        );
    }

    #[test]
    fn an_emacs_lisp_constant_with_an_unprovable_value_resolves_to_nothing() {
        let analysis = analyze_as("(defconst my-limit (some-call))", Dialect::EmacsLisp);
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("my-limit").expect("symbol")),
            None
        );
    }

    #[test]
    fn an_emacs_lisp_constant_is_visible_through_a_lexical_binding() {
        // `let` binds dynamically without a `lexical-binding: t` header, and
        // a dynamic binding is not propagatable — see `Binding::is_propagatable`.
        let analysis = analyze_as(
            ";;; -*- lexical-binding: t -*-\n(defconst my-limit 10)(let ((n my-limit)) (list n))",
            Dialect::EmacsLisp,
        );
        assert_eq!(
            value_at_last_use(&analysis, "n"),
            Value::Known(LiteralValue::Integer(10))
        );
    }

    /// Builds a project table from `sources`, then the value table of the
    /// file at `index` with that project behind it.
    fn analyze_in_project(sources: &[&str], index: usize) -> Analysis {
        use crate::semantics::binding::build_binding_table;
        use crate::semantics::project::service::{
            ProjectFile, build_global_table, resolve_file_packages,
        };

        let trees: Vec<SyntaxTree> = sources
            .iter()
            .map(|text| SyntaxTree::parse_with_dialect(text, Dialect::CommonLisp).expect("parse"))
            .collect();
        let bindings: Vec<_> = trees
            .iter()
            .zip(sources)
            .map(|(tree, text)| build_binding_table(Dialect::CommonLisp, tree, text))
            .collect();
        let packages: Vec<_> = trees
            .iter()
            .map(|tree| resolve_file_packages(Dialect::CommonLisp, tree))
            .collect();
        let values: Vec<_> = trees
            .iter()
            .zip(&bindings)
            .map(|(tree, binding)| build_value_table(Dialect::CommonLisp, tree, binding))
            .collect();

        let files: Vec<ProjectFile<'_>> = (0..trees.len())
            .map(|i| ProjectFile::new(&trees[i], &packages[i], &values[i]))
            .collect();
        let globals = build_global_table(Dialect::CommonLisp, &files);

        let project = ProjectConstants::new(&globals, &packages[index]);
        let table = build_value_table_in_project(
            Dialect::CommonLisp,
            &trees[index],
            &bindings[index],
            Some(&project),
        );

        Analysis {
            tree: SyntaxTree::parse_with_dialect(sources[index], Dialect::CommonLisp)
                .expect("parse"),
            bindings: build_binding_table(Dialect::CommonLisp, &trees[index], sources[index]),
            values: table,
            source: sources[index].to_owned(),
        }
    }

    const DEFINES_LIMIT: &str = "(in-package :app)\n(defconstant +limit+ 10)\n";

    #[test]
    fn a_constant_defined_in_another_file_of_the_same_package_resolves() {
        let analysis = analyze_in_project(
            &[
                DEFINES_LIMIT,
                "(in-package :app)\n(let ((n +limit+)) (list n))\n",
            ],
            1,
        );
        assert_eq!(
            value_at_last_use(&analysis, "n"),
            Value::Known(LiteralValue::Integer(10))
        );
    }

    #[test]
    fn a_constant_two_files_define_resolves_to_nothing() {
        // The project table only carries a constant defined exactly once
        // project-wide, so two files disagreeing leaves nothing to fill in.
        let analysis = analyze_in_project(
            &[
                DEFINES_LIMIT,
                "(in-package :app)\n(defconstant +limit+ 20)\n",
                "(in-package :app)\n(let ((n +limit+)) (list n))\n",
            ],
            2,
        );
        assert_eq!(value_at_last_use(&analysis, "n"), Value::Unknown);
    }

    #[test]
    fn a_constant_from_another_package_is_not_visible_unqualified() {
        // `use-package` inheritance is not modelled, so an unqualified name
        // reaches only its own package. A lost deduction, never a wrong one.
        let analysis = analyze_in_project(
            &[
                DEFINES_LIMIT,
                "(in-package :other)\n(let ((n +limit+)) (list n))\n",
            ],
            1,
        );
        assert_eq!(value_at_last_use(&analysis, "n"), Value::Unknown);
    }

    #[test]
    fn a_file_with_no_in_package_receives_nothing_from_the_project() {
        // It is in no package this layer can name, so filling it in would be
        // a guess. This is exactly its behaviour before the project layer.
        let analysis = analyze_in_project(&[DEFINES_LIMIT, "(let ((n +limit+)) (list n))\n"], 1);
        assert_eq!(value_at_last_use(&analysis, "n"), Value::Unknown);
    }

    #[test]
    fn a_file_that_defines_the_constant_itself_keeps_its_own_answer() {
        // Filling must never overwrite or poison what the file settled. The
        // project's copy of `+limit+` *is* this file's definition, so filling
        // first would have made it look like a second definition of the same
        // name and retracted both.
        let analysis = analyze_in_project(&[DEFINES_LIMIT], 0);
        assert_eq!(
            analysis
                .values
                .constant_value(&SymbolName::new("+LIMIT+").expect("symbol")),
            Some(&PropagatableValue::Integer(10))
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
