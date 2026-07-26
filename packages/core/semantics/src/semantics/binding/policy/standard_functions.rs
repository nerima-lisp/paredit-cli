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

use std::collections::HashSet;
use std::sync::LazyLock;

use super::head_index::contains_folded;

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
    // Everything below was added after measuring a 38-library corpus: each
    // name was a head that flagged real scopes opaque, and each is a function
    // in the `COMMON-LISP` package rather than a macro. Standard *macros* that
    // turned up in the same ranking (`with-standard-io-syntax`, `nth-value`)
    // went to the control-form table instead, because the argument for them is
    // "evaluates its subforms in place", not "cannot reach the caller".
    //
    // Objects. A user method on `initialize-instance` runs arbitrary code, but
    // it runs in its own lexical environment: like any function it never sees
    // the caller's bindings.
    "make-instance",
    "slot-value",
    "slot-boundp",
    "slot-exists-p",
    "class-of",
    "class-name",
    "find-class",
    "type-of",
    // Packages and symbols.
    "find-package",
    "package-name",
    "find-symbol",
    "symbol-package",
    "symbol-value",
    "symbol-plist",
    "make-symbol",
    "boundp",
    "fboundp",
    "constantp",
    "macro-function",
    "special-operator-p",
    // Lists and sequences.
    "copy-seq",
    "copy-list",
    "copy-tree",
    "nconc",
    "endp",
    "map",
    "mapcan",
    "mapcon",
    "maplist",
    "make-sequence",
    "replace",
    "fill",
    "merge",
    "adjoin",
    "union",
    "intersection",
    "set-difference",
    "subsetp",
    "remove-duplicates",
    "delete",
    "delete-if",
    "delete-if-not",
    "delete-duplicates",
    "substitute",
    "nsubstitute",
    "find-if-not",
    "position-if-not",
    "count-if-not",
    "assoc-if",
    "rassoc",
    "acons",
    "pairlis",
    "subst",
    "sublis",
    "tree-equal",
    "rplaca",
    "rplacd",
    // Hash tables and arrays.
    "maphash",
    "remhash",
    "clrhash",
    "sxhash",
    "hash-table-test",
    "vector",
    "array-dimension",
    "array-dimensions",
    "array-element-type",
    "row-major-aref",
    // Numbers.
    "random",
    "numerator",
    "denominator",
    "exp",
    "log",
    "sin",
    "cos",
    "tan",
    "atan",
    "ash",
    "logand",
    "logior",
    "logxor",
    "lognot",
    "logbitp",
    "logcount",
    "logtest",
    "integer-length",
    // Characters and string comparison.
    "character",
    "char-upcase",
    "char-downcase",
    "char-equal",
    "char/=",
    "char<",
    "char>",
    "string/=",
    "string<",
    "string>",
    "string<=",
    "string>=",
    "string-equal",
    "string-trim",
    "string-left-trim",
    "string-right-trim",
    "string-capitalize",
    "alpha-char-p",
    "digit-char-p",
    "alphanumericp",
    "upper-case-p",
    "lower-case-p",
    // Streams and pathnames.
    "read",
    "read-line",
    "read-char",
    "peek-char",
    "read-from-string",
    "write-char",
    "write-line",
    "write-to-string",
    "princ-to-string",
    "prin1-to-string",
    "streamp",
    "make-pathname",
    "merge-pathnames",
    "pathname",
    "pathname-name",
    "pathname-type",
    "pathname-directory",
    "namestring",
    "probe-file",
    "truename",
    // Functions as values.
    "complement",
    "constantly",
];

static INDEX: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| PURE_STANDARD_FUNCTIONS.iter().copied().collect());

/// Whether `head` names a standard function, and so cannot reassign anything
/// in the caller's scope.
pub fn is_pure_standard_function(head: &str) -> bool {
    contains_folded(&INDEX, head)
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
    fn every_name_is_reachable_through_the_folded_index() {
        // An uppercase or over-long entry would be unreachable rather
        // than wrong, which shows up as a missing deduction and nothing
        // else. See `head_index`.
        assert!(super::super::head_index::is_lookupable(
            PURE_STANDARD_FUNCTIONS
        ));
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
