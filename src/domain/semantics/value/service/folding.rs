//! Constant evaluation of an expression.

use crate::domain::common_lisp::{
    common_lisp_reader_conditional_kind, common_lisp_reader_label_kind,
    common_lisp_symbol_reference_needle,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use crate::domain::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix, SymbolName};
use crate::domain::view_query::list_head;

use crate::domain::semantics::NodeKey;
use crate::domain::semantics::binding::BindingTable;

use super::super::model::{LiteralValue, Value, ValueTable};
use super::super::policy::{
    FoldableControlForm, FoldableOperation, foldable_control_form, foldable_operation,
};
use super::literal_reader::literal_value;

/// What `view` provably evaluates to.
///
/// Every path that cannot be proved returns [`Value::Unknown`]: arithmetic that
/// overflows, division that is not exact, and any form whose head is not on the
/// fold whitelist. That last case is the important one — an unregistered head
/// may be a macro, and a macro can do anything with what it encloses, so the
/// subtree is not entered at all.
pub fn evaluate_constant(
    dialect: Dialect,
    view: &ExpressionView,
    bindings: &BindingTable,
    values: &ValueTable,
) -> Value {
    if is_read_time_or_quoted(view) {
        return Value::Unknown;
    }

    match view.kind {
        ExpressionKind::Atom => evaluate_atom(dialect, view, bindings, values),
        ExpressionKind::List => evaluate_list(dialect, view, bindings, values),
        ExpressionKind::Root => Value::Unknown,
    }
}

/// Whether the form's value depends on something no static analysis sees.
///
/// Quoted and quasiquoted data denote themselves rather than the value of what
/// they enclose; `#.` runs code at read time; a reader conditional's presence
/// depends on a feature profile; a reader label preserves object identity. All
/// four make what is inside unknowable here.
fn is_read_time_or_quoted(view: &ExpressionView) -> bool {
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

/// An atom is either a literal or a reference to something with a value.
fn evaluate_atom(
    dialect: Dialect,
    view: &ExpressionView,
    bindings: &BindingTable,
    values: &ValueTable,
) -> Value {
    literal_value(dialect, view)
        .map_or_else(|| resolve_reference(view, bindings, values), Value::Known)
}

/// The value of a symbol occurrence, resolved by *position* rather than by
/// name.
///
/// Two `x` bindings in sibling scopes are different things, and only the
/// binding table knows which one an occurrence means — a name lookup would
/// substitute the wrong value under shadowing. The name-keyed fallback is
/// reached only when position resolution fails, which is the case for a
/// file-level `defconstant` that no binding form introduced.
fn resolve_reference(view: &ExpressionView, bindings: &BindingTable, values: &ValueTable) -> Value {
    let Some(span) = atom_symbol_span(view) else {
        return Value::Unknown;
    };
    let resolved = bindings
        .resolve(NodeKey::atom(span))
        .and_then(|binding| values.binding_value(binding));
    if let Some(value) = resolved {
        return Value::Known(value.clone().into());
    }

    atom_symbol_text(view)
        .and_then(constant_key)
        .and_then(|name| values.constant_value(&name).cloned())
        .map_or(Value::Unknown, |value| Value::Known(value.into()))
}

/// The key a file-level constant is stored and looked up under.
///
/// Folded the way the reader folds a symbol, so `+limit+` and `+LIMIT+` are
/// one constant — they name the same symbol, and keying on the raw spelling
/// made a reference in the other case fail to resolve. It is also what lets a
/// constant arriving from the project table, whose names are already folded,
/// be found by a reference written in any case.
///
/// Only Common Lisp populates the constant map at all, so folding here cannot
/// affect a dialect whose symbols are case-sensitive: their map is empty and
/// every lookup misses either way.
pub fn constant_key(text: &str) -> Option<SymbolName> {
    SymbolName::new(common_lisp_symbol_reference_needle(text)).ok()
}

fn evaluate_list(
    dialect: Dialect,
    view: &ExpressionView,
    bindings: &BindingTable,
    values: &ValueTable,
) -> Value {
    let Some(head) = list_head(view) else {
        return Value::Unknown;
    };
    let arguments = &view.children[1..];

    if let Some(form) = foldable_control_form(dialect, head) {
        return evaluate_control_form(dialect, form, arguments, bindings, values);
    }
    let Some(operation) = foldable_operation(dialect, head) else {
        // An unregistered head is opaque: it may be a macro, and this layer
        // never assumes what a macro expands to.
        return Value::Unknown;
    };
    evaluate_operation(dialect, operation, arguments, bindings, values)
}

/// A branching form's value is the branch that runs — provided this layer can
/// tell which branch that is.
fn evaluate_control_form(
    dialect: Dialect,
    form: FoldableControlForm,
    arguments: &[ExpressionView],
    bindings: &BindingTable,
    values: &ValueTable,
) -> Value {
    let evaluate = |view: &ExpressionView| evaluate_constant(dialect, view, bindings, values);
    // The value of an empty body, and of a `when` whose test fails.
    let nothing = Value::Known(LiteralValue::Nil);

    match form {
        FoldableControlForm::If => match branch_taken(dialect, arguments, bindings, values) {
            // `(if nil a)` with no else branch is nil, not an error.
            Some(taken) => taken.map_or(nothing, evaluate),
            None => Value::Unknown,
        },
        FoldableControlForm::When | FoldableControlForm::Unless => {
            let Some(test) = arguments.first() else {
                return Value::Unknown;
            };
            let Some(holds) = evaluate(test).truthiness(dialect) else {
                return Value::Unknown;
            };
            if holds != matches!(form, FoldableControlForm::When) {
                return nothing;
            }
            arguments
                .get(1..)
                .and_then(<[ExpressionView]>::last)
                .map_or(nothing, evaluate)
        }
        FoldableControlForm::And => {
            evaluate_short_circuit(dialect, arguments, true, bindings, values)
        }
        FoldableControlForm::Or => {
            evaluate_short_circuit(dialect, arguments, false, bindings, values)
        }
    }
}

/// Which branch of an `if` runs: `Some(Some(view))` for a branch that exists,
/// `Some(None)` for a missing else, `None` when the test is not provable.
fn branch_taken<'a>(
    dialect: Dialect,
    arguments: &'a [ExpressionView],
    bindings: &BindingTable,
    values: &ValueTable,
) -> Option<Option<&'a ExpressionView>> {
    let test = arguments.first()?;
    let holds = evaluate_constant(dialect, test, bindings, values).truthiness(dialect)?;
    Some(if holds {
        Some(arguments.get(1)?)
    } else {
        arguments.get(2)
    })
}

/// `and` and `or`, which differ only in which truthiness stops them.
///
/// An operand that is not provable stops the fold even when a later one would
/// have decided the result: whether the earlier operand short-circuits is
/// exactly what is unknown.
fn evaluate_short_circuit(
    dialect: Dialect,
    arguments: &[ExpressionView],
    conjunction: bool,
    bindings: &BindingTable,
    values: &ValueTable,
) -> Value {
    // `(and)` is t and `(or)` is nil — each operation's identity.
    let Some((last, leading)) = arguments.split_last() else {
        return Value::Known(if conjunction {
            LiteralValue::Boolean(true)
        } else {
            LiteralValue::Nil
        });
    };

    for argument in leading {
        let value = evaluate_constant(dialect, argument, bindings, values);
        let Some(holds) = value.truthiness(dialect) else {
            return Value::Unknown;
        };
        if holds != conjunction {
            return value;
        }
    }
    evaluate_constant(dialect, last, bindings, values)
}

fn evaluate_operation(
    dialect: Dialect,
    operation: FoldableOperation,
    arguments: &[ExpressionView],
    bindings: &BindingTable,
    values: &ValueTable,
) -> Value {
    let evaluated: Vec<Value> = arguments
        .iter()
        .map(|argument| evaluate_constant(dialect, argument, bindings, values))
        .collect();

    if matches!(operation, FoldableOperation::Not) {
        let [only] = evaluated.as_slice() else {
            return Value::Unknown;
        };
        return only
            .truthiness(dialect)
            .map_or(Value::Unknown, |holds| boolean(!holds));
    }

    // Everything else is integer arithmetic; a float or non-number operand
    // takes the whole form out of reach.
    let Some(operands) = evaluated
        .iter()
        .map(Value::as_integer)
        .collect::<Option<Vec<i128>>>()
    else {
        return Value::Unknown;
    };
    fold_integers(operation, &operands)
}

fn fold_integers(operation: FoldableOperation, operands: &[i128]) -> Value {
    use FoldableOperation as Operation;

    match operation {
        Operation::Add => reduce(0, operands, i128::checked_add),
        Operation::Multiply => reduce(1, operands, i128::checked_mul),
        Operation::Subtract => match operands {
            [] => Value::Unknown,
            [only] => only.checked_neg().map_or(Value::Unknown, integer),
            [first, rest @ ..] => reduce(*first, rest, i128::checked_sub),
        },
        Operation::Divide => match operands {
            [] => Value::Unknown,
            [only] => exact_quotient(1, *only).map_or(Value::Unknown, integer),
            [first, rest @ ..] => reduce(*first, rest, exact_quotient),
        },
        Operation::Increment => single(operands, |value| value.checked_add(1)),
        Operation::Decrement => single(operands, |value| value.checked_sub(1)),
        Operation::Abs => single(operands, i128::checked_abs),
        Operation::Min => extremum(operands, i128::min),
        Operation::Max => extremum(operands, i128::max),
        Operation::NumericEqual => chain(operands, |left, right| left == right),
        Operation::Less => chain(operands, |left, right| left < right),
        Operation::Greater => chain(operands, |left, right| left > right),
        Operation::LessOrEqual => chain(operands, |left, right| left <= right),
        Operation::GreaterOrEqual => chain(operands, |left, right| left >= right),
        Operation::Zerop => predicate(operands, |value| value == 0),
        Operation::Plusp => predicate(operands, |value| value > 0),
        Operation::Minusp => predicate(operands, |value| value < 0),
        Operation::Evenp => predicate(operands, |value| value % 2 == 0),
        Operation::Oddp => predicate(operands, |value| value % 2 != 0),
        Operation::Not => Value::Unknown,
    }
}

/// `numerator / divisor`, but only when the division comes out exact.
///
/// Common Lisp's `/` yields a rational otherwise, and rationals are outside
/// this model — reporting the truncated quotient would be a different number.
fn exact_quotient(numerator: i128, divisor: i128) -> Option<i128> {
    if divisor == 0 || numerator % divisor != 0 {
        return None;
    }
    numerator.checked_div(divisor)
}

fn reduce(start: i128, operands: &[i128], combine: fn(i128, i128) -> Option<i128>) -> Value {
    operands
        .iter()
        .try_fold(start, |total, operand| combine(total, *operand))
        .map_or(Value::Unknown, integer)
}

fn single(operands: &[i128], apply: fn(i128) -> Option<i128>) -> Value {
    match operands {
        [only] => apply(*only).map_or(Value::Unknown, integer),
        _ => Value::Unknown,
    }
}

fn extremum(operands: &[i128], choose: fn(i128, i128) -> i128) -> Value {
    match operands {
        [] => Value::Unknown,
        [first, rest @ ..] => integer(rest.iter().fold(*first, |best, next| choose(best, *next))),
    }
}

/// A comparison chain: `(< a b c)` holds when each adjacent pair does. A single
/// argument is trivially true, which is what the CLHS specifies.
fn chain(operands: &[i128], holds: fn(i128, i128) -> bool) -> Value {
    if operands.is_empty() {
        return Value::Unknown;
    }
    boolean(operands.windows(2).all(|pair| holds(pair[0], pair[1])))
}

fn predicate(operands: &[i128], holds: fn(i128) -> bool) -> Value {
    match operands {
        [only] => boolean(holds(*only)),
        _ => Value::Unknown,
    }
}

fn integer(value: i128) -> Value {
    Value::Known(LiteralValue::Integer(value))
}

/// A Common Lisp predicate answers `t` or `nil`, not `t` or `false`.
fn boolean(holds: bool) -> Value {
    Value::Known(if holds {
        LiteralValue::Boolean(true)
    } else {
        LiteralValue::Nil
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::semantics::binding::build_binding_table;
    use crate::domain::sexpr::{Path, SyntaxTree};

    fn evaluate(input: &str) -> Value {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let bindings = build_binding_table(Dialect::CommonLisp, &tree, input);
        let values = ValueTable::default();
        let view = tree.select_path(&Path::root_child(0)).expect("form").view();
        evaluate_constant(Dialect::CommonLisp, &view, &bindings, &values)
    }

    fn integer_of(input: &str) -> Option<i128> {
        evaluate(input).as_integer()
    }

    fn truth_of(input: &str) -> Option<bool> {
        evaluate(input).truthiness(Dialect::CommonLisp)
    }

    #[test]
    fn folds_integer_arithmetic() {
        assert_eq!(integer_of("(+ 1 2 3)"), Some(6));
        assert_eq!(integer_of("(- 10 3)"), Some(7));
        assert_eq!(integer_of("(- 5)"), Some(-5));
        assert_eq!(integer_of("(* 2 3 4)"), Some(24));
        assert_eq!(integer_of("(1+ 9)"), Some(10));
        assert_eq!(integer_of("(1- 9)"), Some(8));
        assert_eq!(integer_of("(abs -7)"), Some(7));
        assert_eq!(integer_of("(min 3 1 2)"), Some(1));
        assert_eq!(integer_of("(max 3 1 2)"), Some(3));
        assert_eq!(integer_of("(+)"), Some(0));
        assert_eq!(integer_of("(*)"), Some(1));
    }

    #[test]
    fn folds_nested_arithmetic() {
        assert_eq!(integer_of("(- 1 1)"), Some(0));
        assert_eq!(integer_of("(* (+ 1 1) (- 5 2))"), Some(6));
    }

    #[test]
    fn division_folds_only_when_it_is_exact() {
        assert_eq!(integer_of("(/ 6 3)"), Some(2));
        assert_eq!(integer_of("(/ 1)"), Some(1));
        // 7/3 is a rational, which this model does not represent.
        assert_eq!(evaluate("(/ 7 3)"), Value::Unknown);
    }

    #[test]
    fn dividing_by_zero_is_unknown_not_an_answer() {
        // Whether this is a *bug* is the zero-divisor rule's business; this
        // layer only declines to produce a value.
        assert_eq!(evaluate("(/ 1 0)"), Value::Unknown);
    }

    #[test]
    fn arithmetic_that_overflows_the_model_is_unknown() {
        let max = i128::MAX.to_string();
        assert_eq!(evaluate(&format!("(+ {max} 1)")), Value::Unknown);
        assert_eq!(evaluate(&format!("(* {max} 2)")), Value::Unknown);
    }

    #[test]
    fn folds_comparisons_and_numeric_predicates() {
        assert_eq!(truth_of("(= 1 1)"), Some(true));
        assert_eq!(truth_of("(< 1 2 3)"), Some(true));
        assert_eq!(truth_of("(< 1 3 2)"), Some(false));
        assert_eq!(truth_of("(>= 3 3)"), Some(true));
        assert_eq!(truth_of("(zerop 0)"), Some(true));
        assert_eq!(truth_of("(plusp -1)"), Some(false));
        assert_eq!(truth_of("(evenp 4)"), Some(true));
        assert_eq!(truth_of("(oddp 4)"), Some(false));
    }

    #[test]
    fn a_false_predicate_answers_nil_the_way_common_lisp_does() {
        assert_eq!(evaluate("(zerop 1)"), Value::Known(LiteralValue::Nil));
    }

    #[test]
    fn folds_negation_over_any_value() {
        assert_eq!(truth_of("(not nil)"), Some(true));
        assert_eq!(truth_of("(null nil)"), Some(true));
        assert_eq!(truth_of("(not 0)"), Some(false));
        assert_eq!(truth_of("(not t)"), Some(false));
    }

    #[test]
    fn a_decided_conditional_folds_to_the_branch_that_runs() {
        assert_eq!(integer_of("(if t 1 2)"), Some(1));
        assert_eq!(integer_of("(if nil 1 2)"), Some(2));
        assert_eq!(evaluate("(if nil 1)"), Value::Known(LiteralValue::Nil));
        assert_eq!(integer_of("(when t 1 2)"), Some(2));
        assert_eq!(evaluate("(when nil 1)"), Value::Known(LiteralValue::Nil));
        assert_eq!(integer_of("(unless nil 7)"), Some(7));
        assert_eq!(evaluate("(unless t 7)"), Value::Known(LiteralValue::Nil));
    }

    #[test]
    fn an_undecidable_test_leaves_the_whole_conditional_unknown() {
        // Both branches being constant does not help: which one runs is the
        // unknown.
        assert_eq!(evaluate("(if (f) 1 1)"), Value::Unknown);
    }

    #[test]
    fn and_or_short_circuit_the_way_common_lisp_does() {
        assert_eq!(integer_of("(and 1 2 3)"), Some(3));
        assert_eq!(evaluate("(and 1 nil 3)"), Value::Known(LiteralValue::Nil));
        assert_eq!(integer_of("(or nil 2 3)"), Some(2));
        assert_eq!(truth_of("(and)"), Some(true));
        assert_eq!(truth_of("(or)"), Some(false));
    }

    #[test]
    fn an_unprovable_operand_stops_a_short_circuit_fold() {
        // Whether `(f)` decides the result is exactly what is unknown.
        assert_eq!(evaluate("(and (f) nil)"), Value::Unknown);
    }

    #[test]
    fn an_unregistered_head_is_never_entered() {
        // `my-macro` could expand to anything, including something that never
        // evaluates its argument.
        assert_eq!(evaluate("(my-macro (+ 1 2))"), Value::Unknown);
        assert_eq!(evaluate("(expt 2 3)"), Value::Unknown);
    }

    #[test]
    fn quoted_and_read_time_forms_are_unknown() {
        assert_eq!(evaluate("'(+ 1 2)"), Value::Unknown);
        assert_eq!(evaluate("`(+ 1 2)"), Value::Unknown);
        assert_eq!(evaluate("#.(+ 1 2)"), Value::Unknown);
        assert_eq!(evaluate("'5"), Value::Unknown);
    }

    #[test]
    fn a_float_operand_takes_the_form_out_of_reach() {
        // Reproducing an implementation's rounding is a risk with no payoff.
        assert_eq!(evaluate("(+ 1 2.0)"), Value::Unknown);
    }

    #[test]
    fn a_free_variable_has_no_value() {
        assert_eq!(evaluate("(+ x 1)"), Value::Unknown);
    }
}
