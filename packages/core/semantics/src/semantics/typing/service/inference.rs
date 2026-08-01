//! Building the type table for one file.
//!
//! One recursive descent over the tree, in the same style as
//! [`crate::semantics::value::service::folding`]: [`infer_view`]
//! dispatches on node kind, and every source described in the module doc —
//! literals, `the`, `check-type`, `declare`/`declaim`, standard-function
//! returns, flow narrowing, and branch merges — feeds the same builder by
//! *meeting* ([`combine`]) whatever it finds against whatever the other
//! sources already proved, rather than one replacing another.

use std::collections::HashMap;

use paredit_core_syntax::common_lisp::{
    common_lisp_operator_head_eq, common_lisp_reader_conditional_kind,
    common_lisp_reader_label_kind, is_common_lisp_declaration_form,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

use crate::semantics::NodeKey;
use crate::semantics::binding::BindingTable;
use crate::semantics::value::{LiteralValue, ValueTable, evaluate_constant};

use super::super::model::{Ty, Type, TypeTable, TypeTableBuilder, meet};
use super::super::policy::supports_type_inference;
use super::narrowing::{self, Narrowing};
use super::{calls, declarations, emacs_lisp_declarations, scheme_declarations};

/// Everything a call into any source needs, bundled once rather than threaded
/// argument by argument.
pub(super) struct Ctx<'a> {
    pub(super) dialect: Dialect,
    pub(super) bindings: &'a BindingTable,
    pub(super) values: &'a ValueTable,
    /// The file's own `(declaim (ftype (function (…) RETURN) name…))`
    /// declarations, keyed by lowercased function name. Consulted by
    /// [`calls`] the same way it consults the standard-function table.
    pub(super) declared_returns: &'a HashMap<String, Ty>,
}

/// Builds the type table for one parsed file, on top of its binding and value
/// tables.
#[must_use]
pub fn build_type_table(
    dialect: Dialect,
    tree: &SyntaxTree,
    bindings: &BindingTable,
    values: &ValueTable,
) -> TypeTable {
    let mut builder = TypeTableBuilder::new();
    if !supports_type_inference(dialect) {
        return builder.finish();
    }

    // File-level declared-return tables first: a call earlier in the file may
    // be to a function declared later, and narrowing a binding is meet-based
    // and so order-independent, but a name-keyed lookup table like this one
    // is not — it has to exist before any call consults it. Each dialect
    // reads its own declaration syntax: Common Lisp's `declaim`/`proclaim`
    // `ftype`, Emacs Lisp's `cl-defstruct` slots, Scheme's/Racket's
    // `define-record-type` predicates and Racket's own Typed Racket `:`
    // function annotations.
    let declared_returns = match dialect {
        Dialect::EmacsLisp => emacs_lisp_declarations::collect_declared_returns(tree),
        Dialect::Scheme | Dialect::Racket => {
            scheme_declarations::collect_declared_returns(dialect, tree)
        }
        _ => declarations::collect_declared_returns(dialect, tree),
    };
    let ctx = Ctx {
        dialect,
        bindings,
        values,
        declared_returns: &declared_returns,
    };
    let narrowing = Narrowing::default();

    for root in root_forms(tree) {
        infer_view(&ctx, &narrowing, &root, &mut builder);
    }

    builder.finish()
}

/// The top-level forms, as owned views.
fn root_forms(tree: &SyntaxTree) -> Vec<ExpressionView> {
    (0..tree.root_children().len())
        .filter_map(|index| {
            tree.select_path(&Path::root_child(index))
                .ok()
                .map(|selection| selection.view())
        })
        .collect()
}

/// The type of `view`, recording it (and anything found underneath) into
/// `builder` along the way.
///
/// Quoted, quasiquoted, and read-time forms are opaque here for the same
/// reason they are opaque to [`evaluate_constant`]: `'x` denotes the symbol
/// `x`, not whatever `x` would evaluate to, and a reader conditional's
/// contents depend on a feature profile this layer does not have.
pub(super) fn infer_view(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    if is_opaque_to_evaluation(view) {
        return Type::Unknown;
    }
    match view.kind {
        ExpressionKind::Atom => infer_atom(ctx, narrowing, view, builder),
        ExpressionKind::List => infer_list(ctx, narrowing, view, builder),
        ExpressionKind::Root => Type::Unknown,
    }
}

/// Mirrors `value::service::folding::is_read_time_or_quoted`, duplicated
/// locally rather than shared: the type layer must not reach into another
/// layer's private helpers, and the check is three lines.
fn is_opaque_to_evaluation(view: &ExpressionView) -> bool {
    view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::Quote
                | ReaderPrefix::Quasiquote
                | ReaderPrefix::ReadEval
                | ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
        )
    }) || common_lisp_reader_conditional_kind(view).is_some()
        || common_lisp_reader_label_kind(view).is_some()
}

/// An atom's type is the meet of every source that applies to it: a literal
/// or propagated constant (via the value layer), the binding it references
/// (as narrowed by `declare`/`check-type` so far), and any narrowing active
/// on this path from an enclosing `if`/`when`/`unless`/`cond`/`typecase`.
fn infer_atom(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    let literal_ty = evaluate_constant(ctx.dialect, view, ctx.bindings, ctx.values)
        .as_known()
        .map(ty_of_literal);

    let reference_ty = atom_symbol_span(view)
        .and_then(|span| ctx.bindings.resolve(NodeKey::atom(span)))
        .and_then(|binding| {
            combine(
                builder.binding_type(binding).as_known(),
                narrowing.get(binding),
            )
        });

    record(builder, view, combine(literal_ty, reference_ty))
}

fn infer_list(
    ctx: &Ctx,
    narrowing: &Narrowing,
    view: &ExpressionView,
    builder: &mut TypeTableBuilder,
) -> Type {
    // Covers folded arithmetic and decided `if`/`when`/`unless`/`and`/`or`:
    // whatever the value layer already proved a constant, this layer can
    // report the type of directly, on top of whatever else is found below.
    let folded = evaluate_constant(ctx.dialect, view, ctx.bindings, ctx.values)
        .as_known()
        .map(ty_of_literal);

    let Some(head) = list_head(view) else {
        return record(builder, view, folded);
    };

    // `declare`/`check-type`/`the` are Common Lisp's own declaration and
    // assertion forms. Emacs Lisp's `(declare …)` shares the head spelling
    // but declares unrelated metadata (`indent`, `debug`, …) rather than a
    // type — and it has neither `check-type` nor `the` at all — so these
    // three stay scoped to Common Lisp even though `supports_type_inference`
    // now also covers Emacs Lisp.
    if ctx.dialect == Dialect::CommonLisp {
        if is_common_lisp_declaration_form(head) {
            // `declare`/`declaim`/`proclaim` evaluate nothing: their contents
            // are decl-specs, not expressions, so nothing underneath is
            // walked.
            declarations::narrow_declared_bindings(view, ctx.bindings, builder);
            return record(builder, view, folded);
        }
        if common_lisp_operator_head_eq(head, "check-type") {
            declarations::narrow_check_type(view, ctx.bindings, builder);
            return record(builder, view, folded);
        }
        if common_lisp_operator_head_eq(head, "the") {
            let asserted = the_asserted_type(view);
            if let Some(form) = view.children.get(2) {
                infer_view(ctx, narrowing, form, builder);
            }
            return record(builder, view, combine(asserted, folded));
        }
    }
    if ctx.dialect == Dialect::EmacsLisp && head == "defcustom" {
        // Mirrors `the`, narrowing one form rather than a binding: a
        // `defcustom` name has no `BindingId` this layer's narrowing can
        // attach to (see `emacs_lisp_declarations`'s module doc), so the
        // `:type` option instead narrows the initial-value expression
        // directly.
        let asserted = emacs_lisp_declarations::defcustom_declared_type(view);
        if let Some(value) = view.children.get(2) {
            let inferred = infer_view(ctx, narrowing, value, builder).as_known();
            if let Some(ty) = combine(asserted, inferred) {
                record(builder, value, Some(ty));
            }
        }
        return record(builder, view, folded);
    }
    if let Some(form) = narrowing::classify(head) {
        let merged = narrowing::infer(ctx, narrowing, form, view, builder);
        return record(builder, view, combine(merged.as_known(), folded));
    }

    let called = calls::infer_call(ctx, narrowing, view, head, builder);
    record(builder, view, combine(called.as_known(), folded))
}

/// `(the TYPE form)`'s own asserted type, or `None` when `TYPE` is not a
/// bare, modelled type name — an unmodelled type must stay `Unknown`, not
/// widen to [`Ty::Top`].
fn the_asserted_type(view: &ExpressionView) -> Option<Ty> {
    let spec = view.children.get(1)?;
    Ty::from_name(atom_symbol_text(spec)?)
}

const fn ty_of_literal(value: &LiteralValue) -> Ty {
    match value {
        LiteralValue::Integer(_) => Ty::Integer,
        LiteralValue::Float(_) => Ty::Float,
        LiteralValue::Char(_) => Ty::Character,
        LiteralValue::Keyword(_) => Ty::Keyword,
        LiteralValue::Boolean(_) => Ty::Boolean,
        LiteralValue::Nil => Ty::Null,
        LiteralValue::Text(_) => Ty::String,
    }
}

/// Two independent facts about the same node, narrowed together rather than
/// one replacing the other. `None` means "this source says nothing", not
/// "this source proved `Unknown`" — there is no such proof to combine.
pub(super) fn combine(left: Option<Ty>, right: Option<Ty>) -> Option<Ty> {
    match (left, right) {
        (Some(left), Some(right)) => Some(meet(left, right)),
        (Some(ty), None) | (None, Some(ty)) => Some(ty),
        (None, None) => None,
    }
}

pub(super) fn record(
    builder: &mut TypeTableBuilder,
    view: &ExpressionView,
    ty: Option<Ty>,
) -> Type {
    match ty {
        Some(ty) => {
            if let Some(key) = NodeKey::of(view) {
                builder.set_expression(key, ty);
            }
            Type::Known(ty)
        }
        None => Type::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::binding::build_binding_table;
    use crate::semantics::value::service::build_value_table;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};
    use paredit_core_syntax::view_query::for_each_subview;

    struct Analysis {
        tree: SyntaxTree,
        source: String,
        types: TypeTable,
    }

    fn analyze(input: &str) -> Analysis {
        analyze_as(input, Dialect::CommonLisp)
    }

    fn analyze_as(input: &str, dialect: Dialect) -> Analysis {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let bindings = build_binding_table(dialect, &tree, input);
        let values = build_value_table(dialect, &tree, &bindings);
        let types = build_type_table(dialect, &tree, &bindings, &values);
        Analysis {
            tree,
            source: input.to_owned(),
            types,
        }
    }

    /// The type recorded at the node whose source text is exactly `text`,
    /// picking the rightmost such node when the text repeats — the use site
    /// after every binding and narrowing form has had a chance to run.
    fn type_of(analysis: &Analysis, text: &str) -> Type {
        let mut best: Option<(usize, NodeKey)> = None;
        for index in 0..analysis.tree.root_children().len() {
            let Ok(selection) = analysis.tree.select_path(&Path::root_child(index)) else {
                continue;
            };
            let root = selection.view();
            for_each_subview(&root, |view| {
                if analysis.source.get(view.span.as_range()) != Some(text) {
                    return;
                }
                let start = view.span.start().get();
                if best.is_none_or(|(best_start, _)| start > best_start) {
                    if let Some(key) = NodeKey::of(view) {
                        best = Some((start, key));
                    }
                }
            });
        }
        best.map_or(Type::Unknown, |(_, key)| {
            analysis.types.expression_type(key)
        })
    }

    /// The type at the `occurrence`-th (0-indexed) match of `needle`, for
    /// telling apart two textually identical atoms — e.g. an `if`'s `then`
    /// and `else` branch both reading `x`.
    fn atom_type_at(analysis: &Analysis, needle: &str, occurrence: usize) -> Type {
        let offset = analysis
            .source
            .match_indices(needle)
            .nth(occurrence)
            .map(|(offset, _)| offset)
            .unwrap_or_else(|| panic!("{needle} does not occur {occurrence} times"));
        let span = ByteSpan::new(
            ByteOffset::new(offset),
            ByteOffset::new(offset + needle.len()),
        );
        analysis.types.expression_type(NodeKey::atom(span))
    }

    #[test]
    fn a_dialect_without_type_inference_gets_an_empty_table() {
        let input = "(let [x 1] x)";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("parse");
        let bindings = build_binding_table(Dialect::Clojure, &tree, input);
        let values = build_value_table(Dialect::Clojure, &tree, &bindings);
        let types = build_type_table(Dialect::Clojure, &tree, &bindings, &values);
        assert_eq!(types.binding_count(), 0);
        assert_eq!(types.expression_count(), 0);
    }

    #[test]
    fn literal_atoms_carry_their_own_type() {
        let analysis = analyze("(list 42 #\\a :key t nil \"s\" 1.5)");
        assert_eq!(type_of(&analysis, "42"), Type::Known(Ty::Integer));
        assert_eq!(type_of(&analysis, "t"), Type::Known(Ty::Boolean));
        assert_eq!(type_of(&analysis, "nil"), Type::Known(Ty::Null));
        assert_eq!(type_of(&analysis, "1.5"), Type::Known(Ty::Float));
    }

    #[test]
    fn folded_arithmetic_reports_the_result_type() {
        // `standard_returns` omits arithmetic on purpose; this comes from the
        // value layer already having proved the constant.
        let analysis = analyze("(+ 1 2)");
        assert_eq!(type_of(&analysis, "(+ 1 2)"), Type::Known(Ty::Integer));
    }

    #[test]
    fn a_the_form_records_its_asserted_type() {
        let analysis = analyze("(defun f (x) (the string x))");
        assert_eq!(
            type_of(&analysis, "(the string x)"),
            Type::Known(Ty::String)
        );
    }

    #[test]
    fn an_unmodelled_the_type_stays_unknown_rather_than_widening_to_top() {
        let analysis = analyze("(defun f (x) (the simple-array x))");
        assert_eq!(type_of(&analysis, "(the simple-array x)"), Type::Unknown);
    }

    #[test]
    fn a_check_type_narrows_the_places_binding() {
        let analysis = analyze("(defun f (x) (check-type x integer) x)");
        assert_eq!(type_of(&analysis, "x"), Type::Known(Ty::Integer));
    }

    #[test]
    fn a_declare_type_narrows_every_listed_variable() {
        let analysis = analyze("(defun f (x y) (declare (type integer x y)) (list x y))");
        assert_eq!(type_of(&analysis, "x"), Type::Known(Ty::Integer));
        assert_eq!(type_of(&analysis, "y"), Type::Known(Ty::Integer));
    }

    #[test]
    fn the_declare_shorthand_is_equivalent_to_type() {
        let analysis = analyze("(defun f (x) (declare (string x)) x)");
        assert_eq!(type_of(&analysis, "x"), Type::Known(Ty::String));
    }

    #[test]
    fn contradictory_declarations_meet_at_the_bottom_type() {
        let analysis =
            analyze("(defun f (x) (declare (type integer x)) (declare (type string x)) x)");
        assert_eq!(type_of(&analysis, "x"), Type::Known(Ty::Bottom));
    }

    #[test]
    fn a_declaimed_ftype_return_is_known_at_the_call_site() {
        let analysis = analyze(
            "(declaim (ftype (function (integer) string) convert)) (defun f (x) (convert x))",
        );
        assert_eq!(type_of(&analysis, "(convert x)"), Type::Known(Ty::String));
    }

    #[test]
    fn a_standard_function_return_type_is_known() {
        let analysis = analyze("(defun f (s) (length s))");
        assert_eq!(type_of(&analysis, "(length s)"), Type::Known(Ty::Integer));
    }

    #[test]
    fn an_argument_dependent_function_stays_unknown() {
        let analysis = analyze("(defun f (x) (car x))");
        assert_eq!(type_of(&analysis, "(car x)"), Type::Unknown);
    }

    #[test]
    fn a_quoted_form_is_never_descended_into() {
        // `'(the integer 1)` denotes the list, not the integer 1 — inferring
        // a type from what is inside would be reading data as code.
        let analysis = analyze("(list '(the integer 1))");
        assert_eq!(type_of(&analysis, "(the integer 1)"), Type::Unknown);
    }

    #[test]
    fn an_unregistered_head_is_never_descended_into() {
        let analysis = analyze("(defun f (x) (my-macro (the integer x)))");
        assert_eq!(type_of(&analysis, "(the integer x)"), Type::Unknown);
    }

    #[test]
    fn if_narrows_the_then_branch_and_not_the_else_branch() {
        let analysis = analyze("(defun f (x) (if (stringp x) x x))");
        // `x` occurs 4 times: the parameter, the `stringp` test, the `then`
        // branch, and the `else` branch.
        assert_eq!(atom_type_at(&analysis, "x", 2), Type::Known(Ty::String));
        assert_eq!(atom_type_at(&analysis, "x", 3), Type::Unknown);
    }

    #[test]
    fn narrowing_does_not_leak_past_the_if() {
        let analysis = analyze("(defun f (x) (if (stringp x) x x) x)");
        assert_eq!(atom_type_at(&analysis, "x", 4), Type::Unknown);
    }

    #[test]
    fn if_merges_its_branches_at_their_join() {
        let analysis = analyze("(defun f (x) (if (stringp x) x 1))");
        assert_eq!(
            type_of(&analysis, "(if (stringp x) x 1)"),
            Type::Known(Ty::Top)
        );
    }

    #[test]
    fn cond_narrows_each_clause_and_joins_the_results() {
        let analysis = analyze("(defun f (x) (cond ((stringp x) x) ((integerp x) x)))");
        assert_eq!(
            type_of(&analysis, "(cond ((stringp x) x) ((integerp x) x))"),
            Type::Known(Ty::Top)
        );
    }

    #[test]
    fn typecase_narrows_each_clause_by_its_type_name() {
        let analysis = analyze("(defun f (x) (typecase x (string x) (integer x)))");
        assert_eq!(
            type_of(&analysis, "(typecase x (string x) (integer x))"),
            Type::Known(Ty::Top)
        );
    }

    #[test]
    fn emacs_lisp_is_no_longer_an_empty_table() {
        let analysis = analyze_as(
            "(defcustom my-count 0 \"doc\" :type 'integer)",
            Dialect::EmacsLisp,
        );
        assert!(analysis.types.expression_count() > 0);
    }

    #[test]
    fn a_cl_defstruct_accessor_return_type_is_known_at_the_call_site() {
        let analysis = analyze_as(
            "(cl-defstruct point (x 0 :type integer)) (defun f (p) (point-x p))",
            Dialect::EmacsLisp,
        );
        assert_eq!(type_of(&analysis, "(point-x p)"), Type::Known(Ty::Integer));
    }

    #[test]
    fn a_defcustom_type_narrows_an_otherwise_unknown_initial_value() {
        let analysis = analyze_as(
            "(defun compute () 1) (defcustom my-count (compute) \"doc\" :type 'integer)",
            Dialect::EmacsLisp,
        );
        assert_eq!(type_of(&analysis, "(compute)"), Type::Known(Ty::Integer));
    }

    #[test]
    fn a_defcustom_type_contradicting_a_known_literal_meets_at_the_bottom_type() {
        let analysis = analyze_as(
            "(defcustom my-count \"zero\" \"doc\" :type 'integer)",
            Dialect::EmacsLisp,
        );
        assert_eq!(type_of(&analysis, "\"zero\""), Type::Known(Ty::Bottom));
    }

    #[test]
    fn emacs_lisp_declare_is_not_read_as_a_common_lisp_type_declaration() {
        // Emacs Lisp's own `(declare (indent 1))` shares Common Lisp's
        // `declare` head spelling but declares indentation metadata, not a
        // type — `indent` must not be mistaken for a decl-spec head.
        let analysis = analyze_as(
            "(defmacro m (x) (declare (indent 1)) x)",
            Dialect::EmacsLisp,
        );
        assert_eq!(type_of(&analysis, "(declare (indent 1))"), Type::Unknown);
    }

    #[test]
    fn common_lisp_the_and_check_type_still_work_after_the_dialect_split() {
        let analysis = analyze("(defun f (x) (check-type x integer) (the integer x))");
        assert_eq!(
            type_of(&analysis, "(the integer x)"),
            Type::Known(Ty::Integer)
        );
    }

    #[test]
    fn a_scheme_record_predicate_return_type_is_known_at_the_call_site() {
        let analysis = analyze_as(
            "(define-record-type point (make-point x y) point? (x point-x)) \
             (define (f p) (point? p))",
            Dialect::Scheme,
        );
        assert_eq!(type_of(&analysis, "(point? p)"), Type::Known(Ty::Boolean));
    }

    #[test]
    fn a_typed_racket_annotation_return_type_is_known_at_the_call_site() {
        let analysis = analyze_as(
            "(: convert (-> Integer String)) (define (f x) (convert x))",
            Dialect::Racket,
        );
        assert_eq!(type_of(&analysis, "(convert x)"), Type::Known(Ty::String));
    }
}
