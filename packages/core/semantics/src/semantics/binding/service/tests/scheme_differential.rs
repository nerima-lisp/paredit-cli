//! The proof that inverting the Scheme knowledge did not change it.
//!
//! The Common Lisp differential test's argument, run over Scheme fixtures:
//! `lexical_scope`'s query answers "where is `x` referenced, accounting for
//! shadowing?" and the table answers "what does the name at this position
//! mean?". Over the same file the two must partition the same atoms.
//!
//! Scheme makes the oracle simpler in one way and harder in another. Simpler,
//! because it is a Lisp-1: there is no function/value namespace split, so the
//! one known divergence the Common Lisp test has to carve out
//! (`a_local_function_never_shadows_a_variable_reference`) cannot arise --
//! `(letrec ((m ...)) m)` resolves to the same binding in head and argument
//! position alike. Harder, because more of a Scheme form is *syntax*: a
//! `define-record-type` is nothing but names, and a `syntax-rules` template is
//! never evaluated at all.

use std::collections::HashSet;

use crate::lexical_scope::collect_unshadowed_symbol_references;
use crate::semantics::NodeKey;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::scheme::{SchemeDefinitionForm, SchemeOperator};
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, SymbolName, SyntaxTree,
};

use super::super::build_binding_table;

/// One fixture per Scheme binding form, plus the shadowing cases the
/// `lexical_scope` tests pin, so no branch of the dispatch goes unexercised.
const FIXTURES: &[&str] = &[
    // Every `let` flavour, in both delimiter spellings.
    "(let ((a 1) (b a)) (list a b))",
    "(let ([a 1] [b 2]) (list a b))",
    "(let* ((a 1) (b a)) (list a b))",
    "(letrec ((f (lambda (n) (g n))) (g (lambda (n) (f n)))) (f 1))",
    "(letrec* ((a 1) (b a)) (list a b))",
    "(let-syntax ((m (syntax-rules () ((_) 1)))) (m))",
    "(letrec-syntax ((m (syntax-rules () ((_) 1)))) (m))",
    // Named let, both spellings of the loop variable's use.
    "(let loop ((i 0)) (loop i))",
    "(let* loop ((i 0) (j i)) (loop j))",
    // Multiple values.
    "(let-values (((a b) (values 1 2))) (list a b))",
    "(let*-values (((a) (one)) ((b) a)) (list a b))",
    "(define-values (a b) (values 1 2))",
    // Iteration.
    "(do ((i 0 (+ i 1)) (acc '())) ((= i 10) acc) (display i))",
    // Procedures, in all three formals shapes.
    "(lambda (a b) (list a b))",
    "(lambda (a . rest) (list a rest))",
    "(lambda args args)",
    "(case-lambda ((x) x) ((x y) (list x y)))",
    "(define (f a b) (+ a b))",
    "(define (f a . rest) (list a rest))",
    "(define ((adder n) x) (+ n x))",
    "(define answer 42)",
    // Shadowing across nesting.
    "(list x (lambda (x) x) x)",
    "(list x (let ((x 1)) (let ((x 2)) x)) x)",
    "(let ((x 1)) (set! x 2) x)",
    // Forms that bind nothing but look as though they might.
    "(parameterize ((p 1)) (p))",
    "(fluid-let ((x 1)) x)",
    "(guard (e (#t e)) (raise x))",
    // Syntax and records, which hold names rather than references.
    "(define-syntax m (syntax-rules () ((_ a) a)))",
    "(define-syntax-rule (swap a b) (list a b))",
    "(define-record-type point (make-point x y) point? (x point-x))",
    // Quotation.
    "(list x 'x `(hold ,x) x)",
    // An unknown head, which must stay opaque without breaking the partition.
    "(let ((x 1)) (my-unknown-macro (rebinds x)))",
];

fn build(input: &str) -> crate::semantics::binding::model::BindingTable {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("parse");
    build_binding_table(Dialect::Scheme, &tree, input)
}

#[test]
fn the_query_and_the_table_partition_every_live_scheme_symbol() {
    for input in FIXTURES {
        assert_partition(input);
    }
}

fn assert_partition(input: &str) {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme)
        .unwrap_or_else(|error| panic!("{input:?} must parse: {error}"));
    let table = build(input);
    let root = tree.root_view();
    let live = live_occurrences(&root);
    let syntax = syntax_spans(&root);
    let skipped = skipped_regions(&root);
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

        // Grouped by exact text: R7RS 2.1 makes identifiers case-sensitive,
        // so unlike the Common Lisp oracle there is no folding to mirror.
        let occurrences: HashSet<ByteSpan> = live
            .iter()
            .filter(|occurrence| occurrence.name == name)
            .map(|occurrence| occurrence.span)
            .collect();

        let free = free_occurrences(&root, &symbol, input);
        let bound: HashSet<ByteSpan> = occurrences
            .iter()
            .copied()
            .filter(|span| table.resolve(NodeKey::atom(*span)).is_some())
            .collect();
        let other: HashSet<ByteSpan> = occurrences
            .iter()
            .copied()
            .filter(|span| {
                definitions.contains(span)
                    || syntax.contains(span)
                    || skipped
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

#[test]
fn a_lisp_1_resolves_a_local_procedure_in_head_position() {
    // The divergence the Common Lisp test has to carve out cannot arise here.
    // `(letrec ((m (lambda (x) x))) (m m))` reads *one* binding: Scheme has a
    // single namespace, so both the head and the argument resolve to it.
    let input = "(letrec ((m (lambda (x) x))) (m m))";
    let table = build(input);

    let (id, _) = table
        .bindings()
        .find(|(_, binding)| binding.name().as_str() == "m")
        .expect("the letrec binding");

    let references = super::reference_labels(&table, id, input);
    assert_eq!(references, vec!["m@30", "m@32"]);
}

#[test]
fn set_bang_is_recorded_as_an_assignment() {
    // `assignment_form(Dialect::Scheme, "set!")` was already in the policy
    // table; nothing could reach it while the builder refused Scheme outright.
    let input = "(let ((x 1)) (set! x 2) x)";
    let table = build(input);

    let (_, binding) = table
        .bindings()
        .find(|(_, binding)| binding.name().as_str() == "x")
        .expect("the let binding");

    assert_eq!(binding.assignments().len(), 1);
}

#[test]
fn a_letrec_initializer_resolves_its_sibling() {
    // The mutual recursion `letrec` exists for. Under `Sequential` visibility
    // the `g` inside `f`'s lambda would be free.
    let input = "(letrec ((f (lambda (n) (g n))) (g (lambda (n) (f n)))) (f 1))";
    let table = build(input);

    let (id, _) = table
        .bindings()
        .find(|(_, binding)| binding.name().as_str() == "g")
        .expect("the g binding");

    // The reference inside `f`'s body, written *before* `g` is bound.
    assert!(
        !super::reference_labels(&table, id, input).is_empty(),
        "a letrec initializer must see its siblings"
    );
}

/// One live symbol occurrence: an atom the reader rules say is real code.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Occurrence {
    name: String,
    span: ByteSpan,
}

fn live_occurrences(root: &ExpressionView) -> Vec<Occurrence> {
    let mut occurrences = Vec::new();
    collect_live(root, &mut occurrences, Context::Code);
    occurrences
}

/// Whether the position being walked holds code, and how deeply quoted it is.
///
/// Derived from the reader rules alone rather than from the builder, so it
/// cannot agree with a bug by construction.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Evaluated. Atoms here are references.
    Code,
    /// Inside `'…`. Nothing below is ever evaluated, and no unquote escapes.
    Data,
    /// Inside `` `… ``, at the given nesting depth. An unquote at depth 1
    /// drops back to [`Self::Code`]; `` `(a `(b ,,x)) `` needs two.
    Template(usize),
}

impl Context {
    fn descend(self, view: &ExpressionView) -> Self {
        use paredit_core_syntax::sexpr::ReaderPrefix;

        let mut context = self;
        for prefix in &view.reader_prefixes {
            context = match (context, prefix) {
                (Self::Data, _) => Self::Data,
                (_, ReaderPrefix::Quote) => Self::Data,
                (Self::Code, ReaderPrefix::Quasiquote) => Self::Template(1),
                (Self::Template(depth), ReaderPrefix::Quasiquote) => Self::Template(depth + 1),
                (Self::Template(1), ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing) => {
                    Self::Code
                }
                (Self::Template(depth), ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing) => {
                    Self::Template(depth - 1)
                }
                (context, _) => context,
            };
        }
        context
    }

    const fn is_code(self) -> bool {
        matches!(self, Self::Code)
    }
}

fn collect_live(view: &ExpressionView, occurrences: &mut Vec<Occurrence>, context: Context) {
    let context = context.descend(view);

    if view.kind == ExpressionKind::Atom {
        if !context.is_code() {
            return;
        }
        if let (Some(name), Some(span)) = (atom_symbol_text(view), atom_symbol_span(view)) {
            // The `.` of an improper formals list is punctuation. Neither
            // layer reads it as a symbol, so counting it here would demand a
            // partition entry that can never exist.
            if name == "." {
                return;
            }
            occurrences.push(Occurrence {
                name: name.to_owned(),
                span,
            });
        }
        return;
    }

    // The explicit spellings, which carry no reader prefix.
    let context = match view.children.first().and_then(atom_symbol_text) {
        Some("quote") => Context::Data,
        Some("quasiquote") if context.is_code() => Context::Template(1),
        _ => context,
    };

    for child in &view.children {
        collect_live(child, occurrences, context);
    }
}

fn free_occurrences(root: &ExpressionView, symbol: &SymbolName, input: &str) -> HashSet<ByteSpan> {
    let mut spans = Vec::new();
    for form in &root.children {
        collect_unshadowed_symbol_references(Dialect::Scheme, form, symbol, input, &mut spans);
    }
    spans.into_iter().collect()
}

/// Atoms that spell syntax rather than a reference.
///
/// The head of a *special form* is syntax; the head of an ordinary call is
/// not, because Scheme is a Lisp-1 and `(f x)` reads the binding `f`. A
/// definition's own name is syntax too: it names a global, and this table is
/// one file's lexical context.
fn syntax_spans(view: &ExpressionView) -> HashSet<ByteSpan> {
    let mut spans = HashSet::new();
    collect_syntax_spans(view, &mut spans);
    spans
}

fn collect_syntax_spans(view: &ExpressionView, spans: &mut HashSet<ByteSpan>) {
    if let Some(operator) = view
        .children
        .first()
        .and_then(atom_symbol_text)
        .and_then(SchemeOperator::from_head)
    {
        if operator.is_binder() || operator.definition_form().is_some() {
            if let Some(span) = view.children.first().and_then(atom_symbol_span) {
                spans.insert(span);
            }
        }

        // `(define (f x) ...)`: `f` names a global. `(define-values (a b) ...)`
        // and `(define-record-type name ...)`: likewise.
        if let Some(definition) = operator.definition_form() {
            match definition {
                SchemeDefinitionForm::Define | SchemeDefinitionForm::DefineContract => {
                    spans.extend(definition_name_spans(view.children.get(1)));
                }
                SchemeDefinitionForm::DefineValues => {
                    spans.extend(definition_name_spans(view.children.get(1)));
                }
                _ => {}
            }
        }

        // A named `let`'s loop variable is a binding, not syntax, so it is
        // deliberately absent here.
    }

    for child in &view.children {
        collect_syntax_spans(child, spans);
    }
}

/// Every atom span in a definition's name position, one level deep.
///
/// `(define answer ...)` has one; `(define (f x) ...)` has the name plus the
/// parameters, and the parameters *are* bindings -- so only the head of the
/// list is taken, and `(define-values (a b) ...)` takes all of them because
/// none is registered.
fn definition_name_spans(target: Option<&ExpressionView>) -> Vec<ByteSpan> {
    let Some(target) = target else {
        return Vec::new();
    };

    if target.kind == ExpressionKind::Atom {
        return atom_symbol_span(target).into_iter().collect();
    }

    // A procedure `define`: the leftmost spine down to the name.
    let mut head = target;
    while head.kind == ExpressionKind::List {
        let Some(first) = head.children.first() else {
            return Vec::new();
        };
        head = first;
    }
    atom_symbol_span(head).into_iter().collect()
}

/// Source regions that *neither* layer reads as code.
///
/// A `syntax-rules` template is substituted rather than evaluated, and a
/// record declaration is a list of constructor, predicate, field and accessor
/// *names*. Both layers walk past them, so they are the residue the partition
/// must be allowed to leave over.
fn skipped_regions(view: &ExpressionView) -> Vec<ByteSpan> {
    let mut regions = Vec::new();
    collect_skipped_regions(view, &mut regions);
    regions
}

fn collect_skipped_regions(view: &ExpressionView, regions: &mut Vec<ByteSpan>) {
    if let Some(definition) = view
        .children
        .first()
        .and_then(atom_symbol_text)
        .and_then(SchemeOperator::from_head)
        .and_then(SchemeOperator::definition_form)
    {
        match definition {
            SchemeDefinitionForm::DefineSyntax | SchemeDefinitionForm::DefineSyntaxRule => {
                regions.extend(view.children.iter().skip(1).map(|form| form.span));
            }
            SchemeDefinitionForm::DefineRecordType
            | SchemeDefinitionForm::Struct
            | SchemeDefinitionForm::DefineStruct => {
                regions.extend(view.children.iter().skip(1).map(|form| form.span));
            }
            SchemeDefinitionForm::DefineValues => {
                regions.extend(view.children.get(1).map(|formals| formals.span));
            }
            _ => {}
        }
    }

    // `let-syntax` initializers are transformer specs, not expressions.
    if view
        .children
        .first()
        .and_then(atom_symbol_text)
        .is_some_and(|head| matches!(head, "let-syntax" | "letrec-syntax"))
    {
        if let Some(bindings) = view.children.get(1) {
            regions.extend(
                bindings
                    .children
                    .iter()
                    .filter_map(|entry| entry.children.get(1))
                    .map(|spec| spec.span),
            );
        }
    }

    for child in &view.children {
        collect_skipped_regions(child, regions);
    }
}
