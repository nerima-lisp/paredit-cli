//! Which operators do something observable, and which provably do not.
//!
//! Two tables per dialect, and the gap between them is the point. A head on
//! neither list is *unknown*, not pure — it may be a macro, and a macro can
//! expand into anything. Defaulting the unknown case to pure would make this
//! report claim safety it cannot prove, which is the one failure mode that
//! matters: a caller uses this to decide whether a rewrite is legal.
//!
//! The pure lists are therefore deliberately short and grow only by evidence.
//! The effectful lists may be over-inclusive without harm — a false
//! "effectful" costs a refactor that would have been safe, a false "pure"
//! costs correctness.
//!
//! Common Lisp's tables are the original ones, and most of their entries
//! (`setq`, `print`, `push`, `sort`, `error`, `read`, …) are also valid Emacs
//! Lisp symbols with the same effect meaning, so Emacs Lisp falls through to
//! them once its own smaller, case-sensitive tables have had first say. That
//! order matters for `format`: Common Lisp's `format` writes to a stream
//! unless its destination is `nil`, which this table cannot see — [`head_effect`]
//! only ever sees the head symbol, not its arguments, so it is classified
//! effectful the same way `write`/`print` already are, and the
//! argument-sensitivity is a documented limitation rather than something this
//! layer attempts to resolve by inspecting arguments. Emacs Lisp's `format`
//! has no destination parameter at all — it always returns a string — so it
//! must be looked up in Emacs Lisp's own table *before* falling through to
//! Common Lisp's, or it would inherit the wrong verdict.

use paredit_core_syntax::dialect::Dialect;

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
const EFFECTFUL: [&str; 101] = [
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
    // `(format destination control-string args…)` writes to `destination`
    // whenever it is non-nil — a stream, `t` for `*standard-output*` — and
    // only returns a string when it is nil. This table is symbol-only, so
    // the nil-destination case cannot be told apart from the rest; treating
    // `format` as unconditionally effectful matches the other output
    // primitives below rather than risking a false "pure" on the common
    // case. See the module doc for Emacs Lisp's different, always-pure
    // `format`.
    "format",
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

/// Emacs Lisp operators that observably do something, and that Common Lisp's
/// own table above does not already name — buffer and window mutation,
/// hooks, and property-list access, none of which have a Common Lisp
/// counterpart in [`EFFECTFUL`].
const EMACS_LISP_EFFECTFUL: [&str; 14] = [
    "message",
    "insert",
    "insert-char",
    "delete-region",
    "erase-buffer",
    "kill-region",
    "kill-buffer",
    "goto-char",
    "set-buffer",
    "put",
    "run-hooks",
    "run-hook-with-args",
    "add-hook",
    "remove-hook",
];

/// Emacs Lisp operators whose result depends only on their arguments, that
/// are not also valid Common Lisp function names.
///
/// `format` is here rather than in [`PURE`] deliberately: Emacs Lisp's
/// `format` has no destination parameter and always returns a string, unlike
/// Common Lisp's — see the module doc. The rest are `TYPE-p` predicates on
/// their own argument: an object's membership in one of these types cannot
/// change without the object itself changing, so — unlike, say, `point` or
/// `buffer-name`, which read mutable state the argument list does not name —
/// they satisfy this table's "depends only on its arguments" bar.
const EMACS_LISP_PURE: [&str; 8] = [
    "format", "bufferp", "markerp", "framep", "windowp", "overlayp", "processp", "keymapp",
];

/// What this table knows about `head` in `dialect`, or `None` when it knows
/// nothing.
///
/// Common Lisp's own tables are consulted case-insensitively, because the
/// reader folds symbols: `SETF` and `setf` are the same operator, and a
/// case-sensitive table would silently classify half of a shouting codebase
/// as unknown. Emacs Lisp reads symbols case-sensitively and consults its own
/// tables first — `EMACS_LISP_EFFECTFUL`/`EMACS_LISP_PURE` override Common
/// Lisp's answer for a head that means something different in the two
/// dialects (`format`), then falls through to Common Lisp's tables for the
/// large shared vocabulary neither dialect's table repeats.
#[must_use]
pub fn head_effect(dialect: Dialect, head: &str) -> Option<HeadEffect> {
    if dialect == Dialect::EmacsLisp {
        if EMACS_LISP_EFFECTFUL.contains(&head) {
            return Some(HeadEffect::Effectful);
        }
        if EMACS_LISP_PURE.contains(&head) {
            return Some(HeadEffect::Pure);
        }
        if EFFECTFUL.contains(&head) {
            return Some(HeadEffect::Effectful);
        }
        return PURE.contains(&head).then_some(HeadEffect::Pure);
    }

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
        assert_eq!(
            head_effect(Dialect::CommonLisp, "setf"),
            Some(HeadEffect::Effectful)
        );
        assert_eq!(
            head_effect(Dialect::CommonLisp, "SETQ"),
            Some(HeadEffect::Effectful)
        );
    }

    #[test]
    fn arithmetic_is_pure() {
        assert_eq!(
            head_effect(Dialect::CommonLisp, "+"),
            Some(HeadEffect::Pure)
        );
        assert_eq!(
            head_effect(Dialect::CommonLisp, "CAR"),
            Some(HeadEffect::Pure)
        );
    }

    #[test]
    fn an_unregistered_head_is_unknown_rather_than_pure() {
        assert_eq!(head_effect(Dialect::CommonLisp, "my-macro"), None);
    }

    #[test]
    fn a_destructive_operation_is_effectful_even_though_it_returns_a_value() {
        for head in ["nreverse", "nconc", "sort", "delete"] {
            assert_eq!(
                head_effect(Dialect::CommonLisp, head),
                Some(HeadEffect::Effectful),
                "{head}"
            );
        }
    }

    /// The bug this step fixes: `format` was on neither table, so a call
    /// that writes to a stream — the common case, any non-nil destination —
    /// was reported as `unknown` rather than `effectful`.
    #[test]
    fn common_lisp_format_is_effectful() {
        assert_eq!(
            head_effect(Dialect::CommonLisp, "format"),
            Some(HeadEffect::Effectful)
        );
    }

    /// Emacs Lisp's `format` has no destination parameter at all — it always
    /// returns a string — so it must not inherit Common Lisp's verdict.
    #[test]
    fn emacs_lisp_format_is_pure() {
        assert_eq!(
            head_effect(Dialect::EmacsLisp, "format"),
            Some(HeadEffect::Pure)
        );
    }

    #[test]
    fn emacs_lisp_shares_most_of_common_lisps_vocabulary() {
        for head in ["setq", "print", "push", "sort", "error", "read"] {
            assert_eq!(
                head_effect(Dialect::EmacsLisp, head),
                head_effect(Dialect::CommonLisp, head),
                "{head}"
            );
        }
    }

    #[test]
    fn emacs_lisp_lookup_is_case_sensitive() {
        assert_eq!(head_effect(Dialect::EmacsLisp, "SETQ"), None);
    }

    #[test]
    fn emacs_lisp_has_its_own_buffer_and_hook_primitives() {
        for head in ["message", "insert", "add-hook"] {
            assert_eq!(
                head_effect(Dialect::EmacsLisp, head),
                Some(HeadEffect::Effectful),
                "{head}"
            );
        }
    }

    #[test]
    fn emacs_lisp_type_predicates_absent_from_common_lisp_are_pure() {
        assert_eq!(
            head_effect(Dialect::EmacsLisp, "bufferp"),
            Some(HeadEffect::Pure)
        );
    }

    #[test]
    fn the_tables_do_not_overlap_within_a_dialect() {
        for head in EFFECTFUL {
            assert!(
                !PURE.iter().any(|pure| pure.eq_ignore_ascii_case(head)),
                "{head} is in both Common Lisp tables"
            );
        }
        for head in EMACS_LISP_EFFECTFUL {
            assert!(
                !EMACS_LISP_PURE.contains(&head),
                "{head} is in both Emacs Lisp tables"
            );
        }
    }

    #[test]
    fn neither_table_repeats_an_entry() {
        for table in [
            EFFECTFUL.as_slice(),
            PURE.as_slice(),
            EMACS_LISP_EFFECTFUL.as_slice(),
            EMACS_LISP_PURE.as_slice(),
        ] {
            let mut sorted = table.to_vec();
            sorted.sort_unstable();
            let count = sorted.len();
            sorted.dedup();
            assert_eq!(sorted.len(), count, "a table repeats an entry");
        }
    }
}
