//! Standard functions, which provably cannot reassign a caller's binding.
//!
//! The binding table flags a scope opaque when it meets a head whose semantics
//! are unregistered, because a *macro* can expand into `(setq x …)` and rewrite
//! a binding with nothing in the source to show for it. Applied to every
//! unregistered head that rule is sound but useless: Common Lisp has no
//! registry of ordinary function heads, so `(let ((z 0)) (/ x z))` — the case
//! the value layer exists to solve — would be opaque because of the `/`.
//!
//! A function call is different in kind. A function receives its arguments'
//! *values*; it has no access to the caller's lexical environment and cannot
//! assign to it. So a head known to name a standard function is transparent,
//! while an unknown head stays opaque because it might be a macro.
//!
//! The CLHS forbids defining or redefining anything in the `COMMON-LISP`
//! package, which is what makes this table safe to trust: a program cannot
//! turn `length` into a macro. The table is deliberately partial — a name it
//! omits is treated as opaque, which loses deductions rather than inventing
//! them.

/// Standard functions whose arguments are evaluated normally and which cannot
/// touch the caller's bindings.
///
/// Standard *macros* that assign (`setf`, `push`, `incf`, `rotatef`, …) are
/// deliberately absent: those really do write to a place, and the assignment
/// collector records them. So are the control macros (`when`, `loop`, `dolist`)
/// — they are already registered as forms with known semantics.
const PURE_STANDARD_FUNCTIONS: &[&str] = &[
    // Arithmetic and numeric predicates.
    "+",
    "-",
    "*",
    "/",
    "1+",
    "1-",
    "abs",
    "min",
    "max",
    "mod",
    "rem",
    "floor",
    "ceiling",
    "truncate",
    "round",
    "expt",
    "sqrt",
    "isqrt",
    "gcd",
    "lcm",
    "signum",
    "float",
    "=",
    "/=",
    "<",
    ">",
    "<=",
    ">=",
    "zerop",
    "plusp",
    "minusp",
    "evenp",
    "oddp",
    "numberp",
    "integerp",
    "floatp",
    "rationalp",
    "realp",
    "complexp",
    // Equality and type predicates.
    "eq",
    "eql",
    "equal",
    "equalp",
    "not",
    "null",
    "atom",
    "consp",
    "listp",
    "symbolp",
    "keywordp",
    "characterp",
    "stringp",
    "vectorp",
    "arrayp",
    "functionp",
    "hash-table-p",
    "typep",
    "subtypep",
    // Lists and sequences, reading only.
    "car",
    "cdr",
    "caar",
    "cadr",
    "cdar",
    "cddr",
    "first",
    "second",
    "third",
    "rest",
    "last",
    "butlast",
    "nth",
    "nthcdr",
    "cons",
    "list",
    "list*",
    "append",
    "reverse",
    "length",
    "elt",
    "subseq",
    "member",
    "assoc",
    "find",
    "position",
    "count",
    "remove",
    "mapcar",
    "mapc",
    "reduce",
    // Input, output, and conversion. Destructive functions belong here too:
    // the risk this table guards against is a *binding* being reassigned, and
    // a function can only mutate the objects it was handed.
    "format",
    "print",
    "princ",
    "prin1",
    "write",
    "write-string",
    "terpri",
    "error",
    "warn",
    "parse-integer",
    "coerce",
    "concatenate",
    "apply",
    "funcall",
    "values",
    "values-list",
    "gethash",
    "aref",
    "svref",
    "sort",
    "stable-sort",
    "nreverse",
    "make-array",
    "make-string",
    "make-hash-table",
    "make-list",
    "hash-table-count",
    "search",
    "mismatch",
    "every",
    "some",
    "notany",
    "notevery",
    "remove-if",
    "remove-if-not",
    "find-if",
    "position-if",
    "count-if",
    "getf",
    "get",
    "intern",
    "gensym",
    "list-length",
    "array-total-size",
    "array-rank",
    // Strings, characters, and symbols.
    "string",
    "string=",
    "string-upcase",
    "string-downcase",
    "char",
    "schar",
    "char-code",
    "code-char",
    "char=",
    "symbol-name",
    "identity",
];

/// Whether `head` names a standard function, and so cannot reassign anything
/// in the caller's scope.
pub fn is_pure_standard_function(head: &str) -> bool {
    PURE_STANDARD_FUNCTIONS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_calls_are_transparent() {
        for head in ["/", "+", "length", "CAR", "mapcar"] {
            assert!(is_pure_standard_function(head), "{head}");
        }
    }

    #[test]
    fn assigning_macros_are_not_listed_as_pure_functions() {
        // These really do write to a place; the assignment collector owns them.
        for head in [
            "setf", "setq", "push", "pop", "incf", "decf", "rotatef", "shiftf",
        ] {
            assert!(!is_pure_standard_function(head), "{head}");
        }
    }

    #[test]
    fn a_user_defined_head_stays_opaque() {
        // It could be a macro that expands into an assignment, and nothing in
        // the source says otherwise.
        for head in ["my-with-thing", "with-open-file", "run", "app:helper"] {
            assert!(!is_pure_standard_function(head), "{head}");
        }
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut names = PURE_STANDARD_FUNCTIONS.to_vec();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }
}
