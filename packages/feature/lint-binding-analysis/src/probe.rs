//! A probe of what `RuleContext::binding_table()` actually provides.
//!
//! Not a rule. This exists to answer, with evidence rather than assumption,
//! whether the binding table resolves *references* to their binding or merely
//! lists binding *sites* — the question that decides whether a scope-based rule
//! is expressible at all in a feature package.

use std::path::Path;

use paredit_core_lint_engine::engine::RuleContext;
use paredit_core_semantics::semantics::binding::{BindingKind, BindingTable};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// One line of the dump: what was bound, by what, and what resolved to it.
#[derive(Debug)]
struct Row {
    name: String,
    kind: BindingKind,
    binder: String,
    definition: String,
    init: Option<String>,
    references: Vec<String>,
    assignments: Vec<String>,
    special: bool,
    opaque: bool,
}

fn dump(source: &str) -> (Vec<Row>, usize) {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let context = RuleContext::new(Path::new("probe.lisp"), Dialect::CommonLisp, &tree, source);
    let table: &BindingTable = context.binding_table();
    let rows = table
        .bindings()
        .map(|(_, binding)| Row {
            name: binding.name().as_str().to_owned(),
            kind: binding.kind(),
            binder: binding.binder_head().unwrap_or("<none>").to_owned(),
            definition: context.slice(binding.definition()).to_owned(),
            init: binding.init_form().map(|s| context.slice(s).to_owned()),
            references: binding
                .references()
                .iter()
                .map(|s| context.slice(*s).to_owned())
                .collect(),
            assignments: binding
                .assignments()
                .iter()
                .map(|s| context.slice(*s).to_owned())
                .collect(),
            special: !binding.special().is_lexical(),
            opaque: !binding.opacity().is_transparent(),
        })
        .collect();
    (rows, table.scope_count())
}

fn row<'a>(rows: &'a [Row], name: &str, kind: BindingKind) -> &'a Row {
    rows.iter()
        .find(|row| row.name == name && row.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} binding named {name} in {rows:#?}"))
}

/// Q1: sites only, or references resolved to their binding?
#[test]
fn probe_records_references_resolved_to_their_binding() {
    let (rows, _) = dump("(defun f (n) (let ((x (g n))) (+ x x n)))");
    let x = row(&rows, "x", BindingKind::Variable);
    assert_eq!(x.binder, "let");
    assert_eq!(x.init.as_deref(), Some("(g n)"));
    // `definition` is the atom that spells the name, not the whole clause.
    assert_eq!(x.definition, "x");
    // Two references to `x` in the body, and the defining occurrence is not one.
    assert_eq!(x.references.len(), 2, "{x:#?}");

    // `n` is referenced from the init form *and* the body: resolution crosses
    // the let boundary back out to the parameter.
    let n = row(&rows, "n", BindingKind::Variable);
    assert_eq!(n.binder, "defun");
    assert_eq!(n.references.len(), 2, "{n:#?}");
}

/// Q1b: an unused binding is distinguishable from a used one.
#[test]
fn probe_an_unreferenced_binding_has_an_empty_reference_list() {
    let (rows, _) = dump("(defun f () (let ((x 1) (y 2)) (list y)))");
    assert!(row(&rows, "x", BindingKind::Variable).references.is_empty());
    assert_eq!(row(&rows, "y", BindingKind::Variable).references.len(), 1);
}

/// Q2: is the function namespace modelled (CL is a Lisp-2)?
#[test]
fn probe_models_the_function_namespace_separately() {
    let (rows, _) = dump("(defun f () (flet ((g (a) a)) (list (g 1) #'g)))");
    let g = row(&rows, "g", BindingKind::Function);
    assert_eq!(g.binder, "flet");
    assert!(!g.references.is_empty(), "{g:#?}");

    // A variable of the same name must be a *separate* binding, and a value
    // reference must not resolve to the function.
    let (rows, _) = dump("(defun f () (flet ((x () 1)) (let ((x 2)) x)))");
    let fun = row(&rows, "x", BindingKind::Function);
    let var = row(&rows, "x", BindingKind::Variable);
    assert!(
        fun.references.is_empty(),
        "the value reference must not resolve to the flet: {fun:#?}"
    );
    assert_eq!(var.references.len(), 1, "{var:#?}");
}

#[test]
fn probe_models_macrolet_and_symbol_macrolet_as_their_own_kinds() {
    let (rows, _) = dump("(macrolet ((m () 1)) (m))");
    assert_eq!(row(&rows, "m", BindingKind::Macro).references.len(), 1);

    let (rows, _) = dump("(symbol-macrolet ((s 1)) s)");
    assert_eq!(
        row(&rows, "s", BindingKind::SymbolMacro).references.len(),
        1
    );
}

/// Q3: lambda-list sections.
#[test]
fn probe_models_every_lambda_list_section() {
    let (rows, _) = dump(
        "(defun f (a &optional (b 1 b-p) &rest r &key (c 2) &aux (d 3)) \
         (list a b b-p r c d))",
    );
    for name in ["a", "b", "b-p", "r", "c", "d"] {
        let found = row(&rows, name, BindingKind::Variable);
        assert_eq!(found.references.len(), 1, "{name}: {found:#?}");
    }
    // The `&aux` and `&optional` defaults are recorded as init forms.
    assert_eq!(
        row(&rows, "d", BindingKind::Variable).init.as_deref(),
        Some("3")
    );
    assert_eq!(
        row(&rows, "b", BindingKind::Variable).init.as_deref(),
        Some("1")
    );
    // A bare required parameter and `&rest` have no init form.
    assert_eq!(row(&rows, "a", BindingKind::Variable).init, None);
    assert_eq!(row(&rows, "r", BindingKind::Variable).init, None);
}

#[test]
fn probe_models_destructuring_lambda_lists() {
    let (rows, _) = dump("(destructuring-bind (a (b . c)) form (list a b c))");
    for name in ["a", "b", "c"] {
        assert_eq!(
            row(&rows, name, BindingKind::Variable).references.len(),
            1,
            "{name}"
        );
    }
}

/// Q3b: THE false-positive trap. `(declare (ignore x))` is skipped as a
/// declaration, so an ignored binding looks exactly like an unused one.
#[test]
fn probe_declare_ignore_leaves_the_binding_looking_unused() {
    let (rows, _) = dump("(defun f (x) (declare (ignore x)) 1)");
    let x = row(&rows, "x", BindingKind::Variable);
    assert!(
        x.references.is_empty(),
        "a declared-ignore binding is indistinguishable from an unused one \
         through references() alone, so a rule MUST read the declaration \
         itself: {x:#?}"
    );
}

/// Q4: operator position versus argument position.
#[test]
fn probe_distinguishes_operator_position_from_argument_position() {
    // `x` in head position reads the function namespace, so it does not
    // resolve to the `let` variable.
    let (rows, _) = dump("(defun f () (let ((x 1)) (x x)))");
    let x = row(&rows, "x", BindingKind::Variable);
    assert_eq!(
        x.references.len(),
        1,
        "only the argument occurrence is a variable reference: {x:#?}"
    );
}

/// Q5: scoped or flat?
#[test]
fn probe_is_scoped_and_shadowing_resolves_innermost() {
    let source = "(defun f () (let ((x 1)) (let ((x 2)) x)))";
    let (rows, scopes) = dump(source);
    assert!(scopes >= 3, "file + defun + two lets, got {scopes}");
    let mut xs: Vec<_> = rows
        .iter()
        .filter(|row| row.name == "x" && row.kind == BindingKind::Variable)
        .collect();
    assert_eq!(xs.len(), 2, "two distinct bindings named x");
    xs.sort_by_key(|row| row.init.clone());
    // The outer (init `1`) is shadowed and unreferenced; the inner (init `2`)
    // takes the reference. That is scope resolution, not a flat name map.
    assert!(xs[0].references.is_empty(), "outer x: {:#?}", xs[0]);
    assert_eq!(xs[1].references.len(), 1, "inner x: {:#?}", xs[1]);
}

/// The `let*` read-before-binding shape: does the init form's reference
/// resolve out to the enclosing binding rather than to the later one?
#[test]
fn probe_let_star_init_resolves_outward_not_to_the_later_binding() {
    let source = "(defun f () (let ((x 1)) (let* ((y x) (x 2)) (list x y))))";
    let (rows, _) = dump(source);
    let mut xs: Vec<_> = rows
        .iter()
        .filter(|row| row.name == "x" && row.kind == BindingKind::Variable)
        .collect();
    xs.sort_by_key(|row| row.init.clone());
    let outer = xs[0];
    let inner = xs[1];
    assert_eq!(outer.init.as_deref(), Some("1"));
    assert_eq!(inner.init.as_deref(), Some("2"));
    // `(y x)` reads the OUTER x — one reference on the outer binding.
    assert_eq!(
        outer.references.len(),
        1,
        "the let* init reads the outer x: {outer:#?}"
    );
    assert_eq!(
        inner.references.len(),
        1,
        "the body reads the inner x: {inner:#?}"
    );
}

/// Reassignment is tracked separately from reference.
#[test]
fn probe_tracks_assignments_separately_from_references() {
    let (rows, _) = dump("(defun f () (let ((x 1)) (setq x 2) (incf x) x))");
    let x = row(&rows, "x", BindingKind::Variable);
    assert!(!x.assignments.is_empty(), "{x:#?}");
    assert!(!x.references.is_empty(), "{x:#?}");
}

/// Opacity: an unknown macro in scope is flagged, which is what a rule must
/// gate on before concluding "never referenced".
#[test]
fn probe_marks_a_scope_containing_an_unknown_macro_opaque() {
    let (rows, _) = dump("(defun f () (let ((x 1)) (my-unknown-macro)))");
    let x = row(&rows, "x", BindingKind::Variable);
    assert!(
        x.opaque,
        "an unknown head in scope must cost transparency: {x:#?}"
    );

    let (rows, _) = dump("(defun f () (let ((x 1)) (list x)))");
    assert!(!row(&rows, "x", BindingKind::Variable).opaque);
}

/// A `defvar`-declared special is distinguished from a lexical binding
/// *without* using the earmuff convention.
#[test]
fn probe_marks_a_declared_special_binding() {
    let (rows, _) = dump("(defvar *x* 1)\n(defun f () (let ((*x* 2)) (g)))");
    let bound = rows
        .iter()
        .filter(|row| row.name == "*x*" && row.binder == "let")
        .collect::<Vec<_>>();
    assert_eq!(bound.len(), 1);
    assert!(bound[0].special, "{:#?}", bound[0]);
}

/// `dolist`/`dotimes` are modelled binders; `loop` is NOT.
#[test]
fn probe_models_dolist_and_dotimes_but_not_loop() {
    let (rows, _) = dump("(dolist (item items) (print item))");
    assert_eq!(
        row(&rows, "item", BindingKind::Variable).references.len(),
        1
    );

    let (rows, _) = dump("(dotimes (i 10) (print i))");
    assert_eq!(row(&rows, "i", BindingKind::Variable).references.len(), 1);

    // `loop` binds nothing this layer can see: no binding named `j` exists.
    let (rows, _) = dump("(loop for j from 1 to 10 do (print j))");
    assert!(
        !rows
            .iter()
            .any(|row| row.name == "j" && row.kind == BindingKind::Variable),
        "loop is not a modelled binder, so a loop-variable rule cannot use \
         the table: {rows:#?}"
    );
}

/// Non-Common-Lisp dialects get an empty table, so every rule here must be
/// Common Lisp only.
#[test]
fn probe_a_non_analysed_dialect_yields_an_empty_table() {
    let source = "(let [x 1] x)";
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
    let context = RuleContext::new(Path::new("p.clj"), Dialect::Clojure, &tree, source);
    assert_eq!(context.binding_table().bindings().len(), 0);
}

/// A special declared *outside* the file is NOT marked: the `specials` scan
/// only reads this document. `(let ((*standard-output* s)) …)` therefore looks
/// like an ordinary lexical binding with no references — a false-positive
/// factory for any unused-binding rule that trusts `special()` alone.
#[test]
fn probe_a_special_declared_outside_the_file_is_not_marked() {
    let (rows, _) = dump("(defun f (s) (let ((*standard-output* s)) (print 1)))");
    let bound = row(&rows, "*standard-output*", BindingKind::Variable);
    assert!(
        !bound.special,
        "the file declares nothing, so the rebinding reads as lexical: {bound:#?}"
    );
    assert!(
        bound.references.is_empty(),
        "and it has no textual reference: {bound:#?}"
    );
}

/// An unknown macro in scope still has its *arguments* walked, so a reference
/// passed to one is recorded. Only a macro that injects a reference out of
/// nowhere (an anaphoric one) is invisible — which is what opacity marks.
#[test]
fn probe_an_unknown_macros_arguments_are_still_resolved() {
    let (rows, _) = dump("(defun f () (let ((x 1)) (my-unknown-macro x)))");
    let x = row(&rows, "x", BindingKind::Variable);
    assert_eq!(x.references.len(), 1, "{x:#?}");
    assert!(x.opaque, "but the scope is still opaque: {x:#?}");
}

/// A binder inside a macro template is not registered at all, so quasiquoted
/// code cannot produce a finding.
#[test]
fn probe_a_binder_inside_a_quasiquote_is_not_registered() {
    let (rows, _) = dump("(defmacro m () `(let ((x 1)) (list 2)))");
    assert!(
        !rows.iter().any(|row| row.name == "x"),
        "macro templates are outside the table: {rows:#?}"
    );
}

/// Can a binding have assignments but no references? If not, the `Assigned`
/// guard in `unused_local_binding` is unreachable.
#[test]
fn probe_an_assignment_also_records_a_reference() {
    for source in [
        "(defun f () (let ((x 1)) (setq x 2)))",
        "(defun f () (let ((x 1)) (setf x 2)))",
        "(defun f () (let ((x 0)) (incf x)))",
        "(defun f () (let ((x nil)) (push 1 x)))",
    ] {
        let (rows, _) = dump(source);
        let x = row(&rows, "x", BindingKind::Variable);
        assert!(!x.assignments.is_empty(), "{source}: {x:#?}");
        assert!(
            !x.references.is_empty(),
            "{source}: an assignment with NO reference would make the \
             Assigned guard reachable: {x:#?}"
        );
    }
}
