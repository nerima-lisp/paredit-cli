//! The proof that inverting the knowledge did not change it.
//!
//! `lexical_scope`'s query answers "where is `x` referenced, accounting for
//! shadowing?" and the table answers "what does the name at this position
//! mean?". Run over the same file the two must partition the same atoms: an
//! occurrence the query calls free is one the table must leave unresolved, and
//! an occurrence the table resolves is one the query must not report.
//!
//! The partition is asserted in full — `live = free ⊎ resolved ⊎ definitions`
//! — so a reference the builder silently drops fails just as loudly as one it
//! invents. The set of live atoms is derived here from the reader rules alone,
//! independently of the builder, so it cannot agree with a bug by
//! construction.

use std::collections::HashSet;

use crate::domain::common_lisp::{
    CommonLispOperator, common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
    is_common_lisp_declaration_form, normalize_common_lisp_operator_head,
};
use crate::domain::dialect::Dialect;
use crate::domain::lexical_scope::collect_unshadowed_symbol_references;
use crate::domain::semantics::NodeKey;
use crate::domain::sexpr::reader::{
    apply_reader_prefix_context, atom_symbol_span, atom_symbol_text,
};
use crate::domain::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SymbolName, SyntaxTree,
};

use super::build;

/// Common Lisp fixtures taken verbatim from `lexical_scope::tests`
/// (shadowing, boundaries, capture), plus one input per binder head so no
/// branch of the dispatch goes unexercised.
pub(super) const FIXTURES: &[&str] = &[
    // --- lexical_scope::tests::shadowing
    "(list x (lambda (x) x))",
    "(let* ((y x) (x 2)) (list x y))",
    "(let ((x 1) (y x)) (list x y))",
    "(let (rows) (helper rows))",
    "(let* (rows) (helper rows))",
    "(symbol-macrolet ((x outer) (y x)) (list x y outer))",
    "(list x (let ((cl-user:x x) (y cl-user:x)) (list x y)) x)",
    "(list x (let* ((cl-user:x x) (y x)) (list x y)) x)",
    "(list x (lambda (&optional (x x)) x))",
    "(list fallback (lambda (&optional (x (fallback y) supplied)) x))",
    "(list fallback (defun f (&optional (x (fallback y) supplied)) x))",
    "(list m (labels ((m (x) (list m x))) (m m)) m)",
    "(list m (labels ((cl-user:m (x) (list m x))) (m m)) m)",
    "(list m (macrolet ((m (x) (list m x))) (m m)) m)",
    "(list m (compiler-macrolet ((m (x) (list m x))) (m m)) m)",
    // --- lexical_scope::tests::boundaries
    "(list x (quote x) x)",
    "(list x 'x #'x `(hold ,x ,@rest) x rest)",
    "(list x (quasiquote (x (unquote x))) x)",
    "(list x `(outer `(inner ,x ,,x)) x)",
    "(list x `(setf (get 'y 'prop) ',x))",
    "(list y #'(lambda (x) y) y)",
    "(list y #'(foo y) y)",
    "(list fn (function fn) fn)",
    "(declaim (ftype (function (my-word) my-word) f))",
    "(list used (locally (declare (special used)) used) used)",
    "(list used (defun caller () (used)) used)",
    "(list outer (define-setf-expander slot (place) (list outer place)) outer)",
    "(list outer (define-compiler-macro render (place) (list outer place)) outer)",
    // --- lexical_scope::tests::capture
    "(let ((target external)) target)",
    "(let ((target external)) (lambda external target))",
    "(list x (defun f (x) (list x)) x)",
    // --- one per binder head
    "(let ((a 1) (b 2)) (list a b))",
    "(let* ((a 1) (b a)) (list a b))",
    "(flet ((f (n) (g n))) (f 1))",
    "(labels ((f (n) (f n))) (f 1))",
    "(macrolet ((m (n) n)) (m 1))",
    "(lambda (a &optional (b a) &rest r &key k &aux (z a)) (list a b r k z))",
    "(defun f (a &optional (b a)) (list a b))",
    "(defmacro m (a &body body) (list a body))",
    "(destructuring-bind (a (b c)) form (list a b c))",
    "(multiple-value-bind (q r) (floor 7 2) (list q r))",
    "(dolist (item items result) (collect item result))",
    "(dotimes (i 10 i) (print i))",
    "(do ((i 0 (1+ i)) (acc nil)) ((= i 10) acc) (report i acc))",
    "(do* ((i 0 (1+ i)) (j i)) ((= i 10)) (print j))",
    "(prog ((a 1) (b 2)) (return (list a b)))",
    "(prog* ((a 1) (b a)) (return b))",
    "(with-slots (x y) instance (list x y))",
    "(with-accessors ((a slot-a)) instance a)",
    "(handler-case (risky) (error (e) (report e)))",
    "(restart-case (risky) (retry (arg) (use arg)))",
    "(handler-bind ((error handler)) (risky))",
    "(restart-bind ((retry fn :report-function reporter)) (risky))",
    "(locally (declare (optimize speed)) (work))",
    "(defvar *special*)",
    "(let ((*special* 1)) (declare (special *special*)) *special*)",
    "(let ((x 1)) (setq x 2) (incf x) (push x acc))",
    "(let ((x 1)) (setf (car x) 3))",
    "(let ((x 1)) (my-unknown-macro (rebinds x)))",
    "(let ((x 1)) (let ((x 2)) (let ((x 3)) x)))",
];

/// One live symbol occurrence: an atom the reader rules say is real code.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Occurrence {
    name: String,
    span: ByteSpan,
}

#[test]
fn the_query_and_the_table_partition_every_live_symbol() {
    for input in FIXTURES {
        assert_partition(input);
    }
}

/// Asserts the partition for one input. Shared with the property test, which
/// runs it over generated nests of binding forms.
pub(super) fn assert_partition(input: &str) {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
        .unwrap_or_else(|error| panic!("{input:?} must parse: {error}"));
    let table = build(input);
    let root = tree.root_view();
    let live = live_occurrences(&root);
    let mut syntax = operator_head_spans(&root);
    // Lambda-list keywords are syntax for the same reason: the lambda-list
    // machine consumes `&optional` as a section marker and never reads it
    // as a symbol.
    syntax.extend(
        live.iter()
            .filter(|occurrence| occurrence.name.starts_with('&'))
            .map(|occurrence| occurrence.span),
    );
    let definitions: HashSet<ByteSpan> = table
        .bindings()
        .map(|(_, binding)| binding.definition())
        .collect();

    let names: HashSet<&str> = live
        .iter()
        .map(|occurrence| occurrence.name.as_str())
        .collect();

    for name in names {
        let Ok(symbol) = SymbolName::new(name.to_owned()) else {
            continue;
        };

        // Grouped by the dialect's own symbol equivalence, not by literal
        // text: the query answers a query for `x` with the `cl-user:x`
        // that references it, so the oracle must too.
        let occurrences: HashSet<ByteSpan> = live
            .iter()
            .filter(|occurrence| common_lisp_symbol_reference_eq(&occurrence.name, name))
            .map(|occurrence| occurrence.span)
            .collect();

        let free = free_occurrences(&root, &symbol, input);
        let bound: HashSet<ByteSpan> = occurrences
            .iter()
            .copied()
            .filter(|span| table.resolve(NodeKey::atom(*span)).is_some())
            .collect();
        // The one place the two layers genuinely differ. Inside a form
        // that binds NAME in the *function* namespace, a bare NAME in
        // value position is unresolved for the table (a local function is
        // not a variable) but shadowed for the query, which has no
        // namespaces. Neither calls it a reference; they disagree about
        // why. `a_local_function_never_shadows_a_variable_reference`
        // pins the exact spans.
        let mut absorbed = local_callable_regions(&root, name);
        absorbed.extend(skipped_regions(&root));
        let other: HashSet<ByteSpan> = occurrences
            .iter()
            .copied()
            .filter(|span| {
                definitions.contains(span)
                    || syntax.contains(span)
                    || absorbed
                        .iter()
                        .any(|region| region.start() <= span.start() && span.end() <= region.end())
            })
            .collect();

        for span in &free {
            assert!(
                occurrences.contains(span),
                "{input:?}: the query reported {name:?} at {span:?}, which \
                     this oracle does not count as live code"
            );
        }

        assert!(
            free.is_disjoint(&bound),
            "{input:?}: {name:?} at {:?} is both free per the query and \
                 resolved per the table",
            free.intersection(&bound).collect::<Vec<_>>()
        );

        let accounted: HashSet<ByteSpan> = free.union(&bound).copied().chain(other).collect();
        assert_eq!(
            accounted, occurrences,
            "{input:?}: occurrences of {name:?} are not partitioned into \
                 free / resolved / defining / syntax"
        );
    }
}

/// The one known divergence between the table and the reference query.
///
/// The query has no namespaces — it is built for renaming, where a name is a
/// name — so it treats *any* occurrence of `m` inside `(labels ((m ...)) ...)`
/// as shadowed. The table separates the namespaces, so a bare `m` in value
/// position is a *variable* reference, which is free here.
///
/// Both agree the occurrence is not a reference to the local function; they
/// disagree about whether it is a reference at all. The table is right by
/// CLHS 3.1.2.1.2, and the value layer needs it to be: resolving a bare `m` to
/// a local function would let a function definition propagate into a variable.
#[test]
fn a_local_function_never_shadows_a_variable_reference() {
    let input = "(labels ((m (x) (list m x))) (m m))";

    let table = build(input);
    let function = super::binding_at(&table, 10);

    // Only the two head positions read the function namespace.
    assert_eq!(
        super::reference_labels(&table, function, input),
        vec!["m@30"],
        "the `m` at 22 is inside the definition body and the `m` at 32 is an \
         argument; both are variable references, and both are free"
    );

    for offset in [22, 32] {
        assert_eq!(
            table.resolve(NodeKey::atom(span_at(input, offset, 1))),
            None,
            "the bare `m` at {offset} is a variable reference, not the local \
             function"
        );
    }
}

fn span_at(input: &str, start: usize, len: usize) -> ByteSpan {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
    let mut found = None;
    collect_spans(&tree.root_view(), &mut found, start, len);
    found.expect("an atom at that offset")
}

fn collect_spans(view: &ExpressionView, found: &mut Option<ByteSpan>, start: usize, len: usize) {
    if view.kind == ExpressionKind::Atom {
        if let Some(span) = atom_symbol_span(view) {
            if span.start().get() == start && span.end().get() == start + len {
                *found = Some(span);
            }
        }
    }
    for child in &view.children {
        collect_spans(child, found, start, len);
    }
}

#[test]
fn a_reference_list_agrees_with_the_resolution_map() {
    for input in FIXTURES {
        let table = build(input);
        for (id, binding) in table.bindings() {
            for span in binding.references() {
                assert_eq!(
                    table.resolve(NodeKey::atom(*span)),
                    Some(id),
                    "{input:?}: {:?} lists a reference at {span:?} the \
                     resolution map does not point back at",
                    binding.name()
                );
            }
        }
    }
}

#[test]
fn every_reference_lies_inside_the_form_that_opened_its_scope() {
    for input in FIXTURES {
        let table = build(input);
        for (_, binding) in table.bindings() {
            let Some(opener) = table.scope(binding.scope()).opener() else {
                continue;
            };
            for span in binding.references().iter().chain(binding.assignments()) {
                assert!(
                    opener.start() <= span.start() && span.end() <= opener.end(),
                    "{input:?}: {:?} at {span:?} is outside the scope opened \
                     at {opener:?}",
                    binding.name()
                );
            }
        }
    }
}

fn free_occurrences(root: &ExpressionView, symbol: &SymbolName, input: &str) -> HashSet<ByteSpan> {
    let mut spans = Vec::new();
    for form in &root.children {
        collect_unshadowed_symbol_references(Dialect::CommonLisp, form, symbol, input, &mut spans);
    }
    spans.into_iter().collect()
}

/// Source regions that *neither* layer reads as code.
///
/// Enumerating them is the point: these are the places where a symbol is
/// syntax belonging to some other language (a condition type, an accessor
/// name, a restart option keyword) or is code the compiler rather than this
/// file runs. The query walks past them and the table records nothing there,
/// so they are the residue the partition must be allowed to leave over.
fn skipped_regions(view: &ExpressionView) -> Vec<ByteSpan> {
    let mut regions = Vec::new();
    collect_skipped_regions(view, &mut regions);
    regions
}

fn collect_skipped_regions(view: &ExpressionView, regions: &mut Vec<ByteSpan>) {
    if let Some(operator) = view
        .children
        .first()
        .filter(|head| head.kind == ExpressionKind::Atom && head.reader_prefixes.is_empty())
        .and_then(|head| head.text.as_deref())
        .and_then(CommonLispOperator::from_head)
    {
        match operator {
            // Expander bodies are code the compiler runs.
            CommonLispOperator::DefineSetfExpander | CommonLispOperator::DefineCompilerMacro => {
                regions.extend(view.children.iter().skip(3).map(|body| body.span));
            }
            // The slot-spec list holds slot and accessor *names*, which name
            // parts of a class, not bindings in this file.
            operator if operator.is_slot_binding() => {
                if let Some(specs) = view.children.get(1) {
                    regions.push(specs.span);
                }
            }
            // A handler spec's first element is a condition *type*; a
            // restart-bind option keyword is a marker, not a value.
            operator if operator.is_handler_bind_binding() => {
                if let Some(binding_form) = view.children.get(1) {
                    for spec in &binding_form.children {
                        regions.extend(spec.children.first().map(|kind| kind.span));
                        regions.extend(
                            spec.children
                                .iter()
                                .enumerate()
                                .filter(|(index, _)| *index >= 2 && index % 2 == 0)
                                .map(|(_, option)| option.span),
                        );
                    }
                }
            }
            // A clause's first element is a condition type or a restart name.
            operator if operator.is_clause_binding() => {
                for clause in view.children.iter().skip(2) {
                    regions.extend(clause.children.first().map(|kind| kind.span));
                }
            }
            _ => {}
        }
    }

    for child in &view.children {
        collect_skipped_regions(child, regions);
    }
}

/// The spans of every `flet`/`labels`/`macrolet` form that binds `name` as a
/// local callable.
fn local_callable_regions(view: &ExpressionView, name: &str) -> Vec<ByteSpan> {
    let mut regions = Vec::new();
    collect_local_callable_regions(view, name, &mut regions);
    regions
}

fn collect_local_callable_regions(view: &ExpressionView, name: &str, regions: &mut Vec<ByteSpan>) {
    let binds = view
        .children
        .first()
        .filter(|head| head.kind == ExpressionKind::Atom && head.reader_prefixes.is_empty())
        .and_then(|head| head.text.as_deref())
        .and_then(CommonLispOperator::from_head)
        .is_some_and(CommonLispOperator::is_local_callable_binding)
        && view.children.get(1).is_some_and(|binding_form| {
            binding_form.children.iter().any(|spec| {
                spec.children
                    .first()
                    .and_then(|first| first.text.as_deref())
                    .is_some_and(|spelling| common_lisp_symbol_reference_eq(spelling, name))
            })
        });

    if binds {
        regions.push(view.span);
    }
    for child in &view.children {
        collect_local_callable_regions(child, name, regions);
    }
}

/// The head atoms that spell a special form.
///
/// These are syntax, not references. The query's binder dispatch consumes the
/// head before descending, so it never reports one; the table never resolves
/// one either. Both agree, and the partition would otherwise trip over the
/// `lambda` in `(lambda (x) x)`.
fn operator_head_spans(view: &ExpressionView) -> HashSet<ByteSpan> {
    let mut spans = HashSet::new();
    collect_operator_heads(view, &mut spans);
    spans
}

fn collect_operator_heads(view: &ExpressionView, spans: &mut HashSet<ByteSpan>) {
    if let Some(operator) = view
        .children
        .first()
        .filter(|head| head.kind == ExpressionKind::Atom && head.reader_prefixes.is_empty())
        .and_then(|head| head.text.as_deref())
        .and_then(CommonLispOperator::from_head)
    {
        if let Some(span) = view.children.first().and_then(atom_symbol_span) {
            spans.insert(span);
        }

        // A `defun`'s own name is consumed by the definition shape, so the
        // query never reports it. The table does not register it either: it
        // names a global, and this table is one file's *lexical* context.
        if operator.is_defun_like() {
            if let Some(span) = view.children.get(1).and_then(atom_symbol_span) {
                spans.insert(span);
            }
        }
    }
    for child in &view.children {
        collect_operator_heads(child, spans);
    }
}

fn live_occurrences(root: &ExpressionView) -> Vec<Occurrence> {
    let mut occurrences = Vec::new();
    for form in &root.children {
        collect_live(form, 0, &mut occurrences);
    }
    occurrences
}

/// Every atom that is real code, derived from the reader rules alone.
///
/// Deliberately independent of the builder: it knows quoting, quasiquotation,
/// function designators, and declarations, and nothing at all about binding
/// forms. That is what makes it usable as an oracle — it cannot agree with a
/// mistake in the scope walk.
fn collect_live(view: &ExpressionView, quasiquote_depth: usize, output: &mut Vec<Occurrence>) {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return;
    };

    if view.kind == ExpressionKind::Atom {
        if view.reader_prefixes.contains(&ReaderPrefix::Function) || quasiquote_depth > 0 {
            return;
        }
        if let (Some(name), Some(span)) = (atom_symbol_text(view), atom_symbol_span(view)) {
            output.push(Occurrence {
                name: name.to_owned(),
                span,
            });
        }
        return;
    }

    if let Some(head) = view
        .children
        .first()
        .filter(|child| child.kind == ExpressionKind::Atom && child.reader_prefixes.is_empty())
        .and_then(|child| child.text.as_deref())
    {
        // A declaration is not evaluated, so nothing in it is a reference —
        // which is why the query skips leading `(declare ...)` forms.
        if is_common_lisp_declaration_form(head) {
            return;
        }

        let normalized = normalize_common_lisp_operator_head(head);
        if view.children.len() >= 2 {
            if common_lisp_operator_head_eq(normalized, "quote") {
                return;
            }
            if common_lisp_operator_head_eq(normalized, "function")
                && view.children.len() == 2
                && view.children[1].kind == ExpressionKind::Atom
            {
                return;
            }
            match normalized.to_ascii_lowercase().as_str() {
                "quasiquote" => {
                    for child in &view.children[1..] {
                        collect_live(child, quasiquote_depth + 1, output);
                    }
                    return;
                }
                "unquote" | "unquote-splicing" if quasiquote_depth > 0 => {
                    for child in &view.children[1..] {
                        collect_live(child, quasiquote_depth - 1, output);
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    for child in &view.children {
        collect_live(child, quasiquote_depth, output);
    }
}
