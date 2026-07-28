//! Which Common Lisp operators do something observable, and which provably do
//! not.
//!
//! Two tables, and the gap between them is the point. A head on neither list is
//! *unknown*, not pure — it may be a macro, and a macro can expand into
//! anything. Defaulting the unknown case to pure would make this report claim
//! safety it cannot prove, which is the one failure mode that matters: a caller
//! uses this to decide whether a rewrite is legal.
//!
//! The pure list is therefore deliberately short and grows only by evidence.
//! The effectful list may be over-inclusive without harm — a false "effectful"
//! costs a refactor that would have been safe, a false "pure" costs
//! correctness.

/// What one operator does, when this table knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadEffect {
    /// Observably changes something: a variable, a place, a stream, the file
    /// system, or the control state of the program.
    Effectful,
    /// A standard function whose result depends only on its arguments and
    /// which mutates nothing.
    Pure,
}

/// Operators that observably do something.
///
/// Grouped by why, because the groups have different reasons to be trusted and
/// different reasons to grow.
const EFFECTFUL: [&str; 100] = [
    // Assignment and place mutation.
    "set",
    "setq",
    "psetq",
    "setf",
    "psetf",
    "incf",
    "decf",
    "push",
    "pushnew",
    "pop",
    "remf",
    "rotatef",
    "shiftf",
    "fill",
    "replace",
    "map-into",
    // Destructive sequence and list operations. `sort` and `stable-sort` are
    // here because CLHS permits them to destroy their argument, whatever a
    // given implementation does.
    "nreverse",
    "nconc",
    "nsubstitute",
    "nsubstitute-if",
    "nsubstitute-if-not",
    "nunion",
    "nintersection",
    "nset-difference",
    "nset-exclusive-or",
    "nbutlast",
    "nreconc",
    "nstring-upcase",
    "nstring-downcase",
    "nstring-capitalize",
    "delete",
    "delete-if",
    "delete-if-not",
    "delete-duplicates",
    "sort",
    "stable-sort",
    "merge",
    "vector-push",
    "vector-push-extend",
    "vector-pop",
    "adjust-array",
    // Hash tables and property lists.
    "remhash",
    "clrhash",
    "setf-gethash",
    "remprop",
    // Output and interaction.
    "print",
    "princ",
    "prin1",
    "pprint",
    "write",
    "write-line",
    "write-string",
    "write-char",
    "write-byte",
    "write-sequence",
    "terpri",
    "fresh-line",
    "finish-output",
    "force-output",
    "clear-output",
    "y-or-n-p",
    "yes-or-no-p",
    // Input, which consumes from a stream.
    "read",
    "read-line",
    "read-char",
    "read-byte",
    "read-sequence",
    "read-from-string",
    "unread-char",
    "clear-input",
    // Conditions and non-local exit. These do not return normally, which is
    // observable however pure the surrounding arithmetic is.
    "error",
    "cerror",
    "warn",
    "signal",
    "abort",
    "invoke-restart",
    "invoke-debugger",
    "throw",
    "break",
    // Streams, files, and the outside world.
    "open",
    "close",
    "with-open-file",
    "with-open-stream",
    "delete-file",
    "rename-file",
    "ensure-directories-exist",
    "load",
    "compile-file",
    "require",
    "provide",
    "run-program",
    "sleep",
    "get-universal-time",
    "get-internal-real-time",
    "get-internal-run-time",
    "random",
    "make-random-state",
    "gensym",
    "gentemp",
    "eval",
];

/// Operators whose result depends only on their arguments.
///
/// Short on purpose. Every entry is a standard function that CLHS defines as
/// returning a value and specifies no side effect for; nothing that takes a
/// function argument is here, because its purity is its argument's, which this
/// table cannot see.
const PURE: [&str; 126] = [
    // Arithmetic and numeric predicates.
    "+",
    "-",
    "*",
    "/",
    "1+",
    "1-",
    "abs",
    "signum",
    "min",
    "max",
    "mod",
    "rem",
    "floor",
    "ceiling",
    "truncate",
    "round",
    "ffloor",
    "fceiling",
    "ftruncate",
    "fround",
    "gcd",
    "lcm",
    "expt",
    "exp",
    "log",
    "sqrt",
    "isqrt",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "numerator",
    "denominator",
    "realpart",
    "imagpart",
    "float",
    "rational",
    "rationalize",
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
    // Type predicates and coercion.
    "null",
    "not",
    "atom",
    "consp",
    "listp",
    "symbolp",
    "keywordp",
    "numberp",
    "integerp",
    "floatp",
    "rationalp",
    "realp",
    "complexp",
    "characterp",
    "stringp",
    "vectorp",
    "arrayp",
    "functionp",
    "hash-table-p",
    "typep",
    "subtypep",
    "type-of",
    "coerce",
    "identity",
    // Equality.
    "eq",
    "eql",
    "equal",
    "equalp",
    // List and sequence access. Readers only; every constructor below returns
    // fresh structure and mutates nothing reachable by the caller.
    "car",
    "cdr",
    "caar",
    "cadr",
    "cdar",
    "cddr",
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "rest",
    "last",
    "butlast",
    "nth",
    "nthcdr",
    "elt",
    "length",
    "list",
    "list*",
    "cons",
    "append",
    "reverse",
    "subseq",
    "copy-seq",
    "copy-list",
    "copy-tree",
    "make-list",
    "list-length",
    // Strings and characters.
    "string",
    "string=",
    "string<",
    "string>",
    "string-equal",
    "string-upcase",
    "string-downcase",
    "string-capitalize",
    "string-trim",
    "concatenate",
    "char",
    "schar",
    "char-code",
    "code-char",
    "char=",
    "char-equal",
    "char-upcase",
    "char-downcase",
];

/// What this table knows about `head`, or `None` when it knows nothing.
///
/// Case-insensitive, because the Common Lisp reader folds symbols: `SETF` and
/// `setf` are the same operator, and a case-sensitive table would silently
/// classify half of a shouting codebase as unknown.
#[must_use]
pub fn head_effect(head: &str) -> Option<HeadEffect> {
    if EFFECTFUL
        .iter()
        .any(|known| known.eq_ignore_ascii_case(head))
    {
        return Some(HeadEffect::Effectful);
    }
    PURE.iter()
        .any(|known| known.eq_ignore_ascii_case(head))
        .then_some(HeadEffect::Pure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_is_effectful() {
        assert_eq!(head_effect("setf"), Some(HeadEffect::Effectful));
        assert_eq!(head_effect("SETQ"), Some(HeadEffect::Effectful));
    }

    #[test]
    fn arithmetic_is_pure() {
        assert_eq!(head_effect("+"), Some(HeadEffect::Pure));
        assert_eq!(head_effect("CAR"), Some(HeadEffect::Pure));
    }

    #[test]
    fn an_unregistered_head_is_unknown_rather_than_pure() {
        assert_eq!(head_effect("my-macro"), None);
    }

    #[test]
    fn a_destructive_operation_is_effectful_even_though_it_returns_a_value() {
        for head in ["nreverse", "nconc", "sort", "delete"] {
            assert_eq!(head_effect(head), Some(HeadEffect::Effectful), "{head}");
        }
    }

    #[test]
    fn the_two_tables_do_not_overlap() {
        for head in EFFECTFUL {
            assert!(
                !PURE.iter().any(|pure| pure.eq_ignore_ascii_case(head)),
                "{head} is in both tables"
            );
        }
    }

    #[test]
    fn neither_table_repeats_an_entry() {
        for table in [EFFECTFUL.as_slice(), PURE.as_slice()] {
            let mut sorted = table.to_vec();
            sorted.sort_unstable();
            let count = sorted.len();
            sorted.dedup();
            assert_eq!(sorted.len(), count, "a table repeats an entry");
        }
    }
}
