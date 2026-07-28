//! R7RS procedures, which provably cannot reassign a caller's binding.
//!
//! The Scheme counterpart of [`super::standard_functions`], and it exists for
//! the same reason: the table flags a scope opaque when it meets a head whose
//! semantics are unregistered, because a macro can expand into `(set! x …)`
//! with nothing in the source to show for it. Applied to every unregistered
//! head that rule is sound but useless -- `(let ((z 0)) (/ x z))` would be
//! opaque because of the `/`.
//!
//! A procedure call is different in kind: it receives its arguments' *values*
//! and has no access to the caller's environment. So a head known to name a
//! standard procedure is transparent, while an unknown head stays opaque.
//!
//! Two things separate this table from the Common Lisp one. Lookup is exact,
//! because R7RS 2.1 makes identifiers case-sensitive and `CAR` is a different
//! name from `car`. And the guarantee is weaker: R7RS 5.5 lets a program
//! redefine any standard binding, where the CLHS forbids it. The table is
//! still worth having -- shadowing `car` is pathological, and a program that
//! does it gets a wrong opacity flag rather than a wrong binding -- but it is
//! deliberately confined to procedures whose redefinition would be perverse.

use std::collections::HashSet;
use std::sync::LazyLock;

/// R7RS procedures that evaluate their arguments and touch no caller binding.
///
/// Syntax is deliberately absent: `let`, `if`, `set!` and the rest are
/// registered in `SchemeOperator`, which is consulted first.
const PURE_STANDARD_SCHEME_PROCEDURES: &[&str] = &[
    // Equivalence and numeric predicates.
    "eq?", "eqv?", "equal?", "=", "<", ">", "<=", ">=", "zero?", "positive?", "negative?", "odd?",
    "even?", "number?", "integer?", "rational?", "real?", "complex?", "exact?", "inexact?",
    "exact-integer?", "nan?", "infinite?", "finite?",
    // Arithmetic.
    "+", "-", "*", "/", "abs", "min", "max", "quotient", "remainder", "modulo", "gcd", "lcm",
    "floor", "ceiling", "truncate", "round", "expt", "exp", "log", "sin", "cos", "tan", "asin",
    "acos", "atan", "sqrt", "exact-integer-sqrt", "square", "numerator", "denominator", "exact",
    "inexact", "number->string", "string->number",
    // Booleans.
    "not", "boolean?", "boolean=?",
    // Pairs and lists.
    "cons", "car", "cdr", "caar", "cadr", "cdar", "cddr", "pair?", "null?", "list?", "list",
    "length", "append", "reverse", "list-tail", "list-ref", "memq", "memv", "member", "assq",
    "assv", "assoc", "list-copy", "cons*",
    // Symbols and characters.
    "symbol?", "symbol->string", "string->symbol", "symbol=?", "char?", "char=?", "char<?",
    "char>?", "char<=?", "char>=?", "char->integer", "integer->char", "char-upcase",
    "char-downcase", "char-alphabetic?", "char-numeric?", "char-whitespace?",
    // Strings.
    "string?", "make-string", "string", "string-length", "string-ref", "string=?", "string<?",
    "string>?", "string<=?", "string>=?", "substring", "string-append", "string->list",
    "list->string", "string-copy", "string-upcase", "string-downcase", "string-contains",
    "string-join", "string-split",
    // Vectors and bytevectors.
    "vector?", "make-vector", "vector", "vector-length", "vector-ref", "vector->list",
    "list->vector", "vector->string", "string->vector", "vector-copy", "vector-append",
    "bytevector?", "make-bytevector", "bytevector", "bytevector-length", "bytevector-u8-ref",
    "utf8->string", "string->utf8",
    // Control. `apply`, `map` and friends call a procedure the caller supplies,
    // but that procedure closes over its *own* environment, not this one, so
    // the caller's bindings stay unreachable.
    "procedure?", "apply", "map", "for-each", "vector-map", "vector-for-each", "string-map",
    "string-for-each", "call-with-current-continuation", "call/cc", "values",
    "call-with-values", "dynamic-wind", "force", "make-parameter", "error", "raise",
    "raise-continuable", "error-object?", "error-object-message", "error-object-irritants",
    // Ports and output. These mutate the world, not the caller's environment.
    "display", "write", "write-string", "write-char", "newline", "read", "read-char",
    "read-line", "read-string", "peek-char", "eof-object", "eof-object?", "current-input-port",
    "current-output-port", "current-error-port", "open-input-string", "open-output-string",
    "get-output-string", "close-port", "flush-output-port",
];

static INDEX: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| PURE_STANDARD_SCHEME_PROCEDURES.iter().copied().collect());

/// Whether `head` names an R7RS procedure that cannot assign a caller binding.
#[must_use]
pub fn is_pure_standard_scheme_procedure(head: &str) -> bool {
    INDEX.contains(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_procedures_are_registered() {
        for head in ["car", "vector-ref", "string=?", "map", "+"] {
            assert!(is_pure_standard_scheme_procedure(head), "{head}");
        }
    }

    #[test]
    fn lookup_is_case_sensitive_because_scheme_is() {
        // Unlike the Common Lisp table, which folds: R7RS 2.1 makes `CAR` a
        // different identifier from `car`, and a program is free to bind it.
        assert!(!is_pure_standard_scheme_procedure("CAR"));
        assert!(!is_pure_standard_scheme_procedure("Vector-Ref"));
    }

    #[test]
    fn a_user_defined_head_stays_opaque() {
        for head in ["my-helper", "with-database", "run!"] {
            assert!(!is_pure_standard_scheme_procedure(head), "{head}");
        }
    }

    #[test]
    fn syntax_is_left_to_the_operator_table() {
        // Registering these here as well would be harmless but misleading:
        // `SchemeOperator` is consulted first and owns them.
        for head in ["let", "if", "set!", "lambda", "define"] {
            assert!(!is_pure_standard_scheme_procedure(head), "{head}");
        }
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut names = PURE_STANDARD_SCHEME_PROCEDURES.to_vec();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }

    #[test]
    fn every_name_is_lowercase_as_r7rs_writes_them() {
        // An uppercase entry would simply be unreachable, since every lookup
        // is exact and real Scheme spells these in lowercase.
        for name in PURE_STANDARD_SCHEME_PROCEDURES {
            assert_eq!(*name, name.to_ascii_lowercase(), "{name}");
        }
    }
}
