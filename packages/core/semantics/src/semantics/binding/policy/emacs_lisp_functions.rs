//! Standard Emacs Lisp functions, which cannot reassign a caller's binding.
//!
//! The same argument as [`super::standard_functions`], with one difference in
//! its footing. The CLHS *forbids* redefining anything in the `COMMON-LISP`
//! package, so that table rests on a language guarantee. Emacs Lisp has no
//! such rule — `(defun car (x) …)` is legal and would break Emacs, but it is
//! legal. What the table rests on here is weaker and still sufficient: these
//! names are subrs implemented in C, a package that redefines one has already
//! broken far more than this analysis, and the alternative — treating every
//! call opaque — makes the value layer useless for `.el` files.
//!
//! Membership is exact rather than case-folded, because Emacs Lisp reads
//! symbols case-sensitively. `Length` is a name a package may define; it is
//! not `length`.
//!
//! Only *pure* names are listed. Anything that mutates a place (`setq`,
//! `push`, `cl-incf`) belongs to the assignment table, anything that mutates
//! the editor (`insert`, `goto-char`, `set-buffer`) is omitted because it is
//! not needed here, and every special form and macro already comes from
//! `EmacsLispOperator`.

use std::collections::HashSet;
use std::sync::LazyLock;

const PURE_STANDARD_EMACS_LISP_FUNCTIONS: &[&str] = &[
    // Arithmetic and numeric predicates.
    "+",
    "-",
    "*",
    "/",
    "%",
    "1+",
    "1-",
    "=",
    "/=",
    "<",
    "<=",
    ">",
    ">=",
    "abs",
    "ceiling",
    "cl-evenp",
    "cl-oddp",
    "cl-plusp",
    "cl-minusp",
    "expt",
    "float",
    "floor",
    "ftruncate",
    "isnan",
    "log",
    "logand",
    "logior",
    "lognot",
    "logxor",
    "ash",
    "lsh",
    "max",
    "min",
    "mod",
    "natnump",
    "number-to-string",
    "numberp",
    "integerp",
    "integer-or-marker-p",
    "floatp",
    "round",
    "sqrt",
    "string-to-number",
    "truncate",
    "zerop",
    // Predicates and type tests.
    "arrayp",
    "atom",
    "booleanp",
    "bufferp",
    "characterp",
    "consp",
    "eq",
    "eql",
    "equal",
    "fboundp",
    "framep",
    "functionp",
    "hash-table-p",
    "keymapp",
    "keywordp",
    "listp",
    "markerp",
    "not",
    "null",
    "nlistp",
    "overlayp",
    "processp",
    "proper-list-p",
    "sequencep",
    "stringp",
    "symbolp",
    "vectorp",
    "windowp",
    "wholenump",
    // List access. The destructive counterparts (`nconc`, `setcar`, `nreverse`)
    // are absent: they mutate the structure a binding points at, and while
    // that does not rebind the binding itself, listing them here would claim
    // a purity they do not have.
    "append",
    "assoc",
    "assoc-default",
    "assoc-string",
    "assq",
    "butlast",
    "caar",
    "cadr",
    "car",
    "car-safe",
    "cdar",
    "cddr",
    "cdr",
    "cdr-safe",
    "cl-first",
    "cl-second",
    "cl-third",
    "cl-remove-if",
    "cl-remove-if-not",
    "cl-find",
    "cl-find-if",
    "cl-position",
    "cl-reduce",
    "cl-remove-duplicates",
    "cl-set-difference",
    "cl-subseq",
    "cl-union",
    "cons",
    "copy-alist",
    "copy-sequence",
    "copy-tree",
    "elt",
    "flatten-tree",
    "last",
    "length",
    "length<",
    "length=",
    "length>",
    "list",
    "make-list",
    "member",
    "memq",
    "memql",
    "nth",
    "nthcdr",
    "number-sequence",
    "rassoc",
    "rassq",
    "remove",
    "remq",
    "reverse",
    "safe-length",
    "take",
    // Sequences via `seq.el`, which is pure by design.
    "seq-contains-p",
    "seq-difference",
    "seq-drop",
    "seq-elt",
    "seq-empty-p",
    "seq-filter",
    "seq-find",
    "seq-first",
    "seq-into",
    "seq-intersection",
    "seq-length",
    "seq-map",
    "seq-mapn",
    "seq-max",
    "seq-min",
    "seq-partition",
    "seq-position",
    "seq-reduce",
    "seq-remove",
    "seq-rest",
    "seq-reverse",
    "seq-some",
    "seq-sort",
    "seq-subseq",
    "seq-take",
    "seq-union",
    "seq-uniq",
    // Higher-order application.
    "apply",
    "funcall",
    "identity",
    "ignore",
    "mapcan",
    "mapcar",
    "mapconcat",
    // Strings and characters.
    "capitalize",
    "char-equal",
    "char-to-string",
    "compare-strings",
    "downcase",
    "file-name-absolute-p",
    "file-name-as-directory",
    "file-name-base",
    "file-name-directory",
    "file-name-extension",
    "file-name-nondirectory",
    "file-name-sans-extension",
    "format",
    "format-message",
    "int-to-string",
    "prin1-to-string",
    "regexp-quote",
    "split-string",
    "string",
    "string-empty-p",
    "string-equal",
    "string-join",
    "string-lessp",
    "string-match-p",
    "string-prefix-p",
    "string-remove-prefix",
    "string-remove-suffix",
    "string-replace",
    "string-reverse",
    "string-search",
    "string-suffix-p",
    "string-to-char",
    "string-to-list",
    "string-to-syntax",
    "string-trim",
    "string-trim-left",
    "string-trim-right",
    "string<",
    "string=",
    "string>",
    "substring",
    "substring-no-properties",
    "symbol-name",
    "upcase",
    "upcase-initials",
    // Symbols and their cells. Reading a cell cannot rebind anything.
    "boundp",
    "default-value",
    "get",
    "indirect-function",
    "intern",
    "intern-soft",
    "make-symbol",
    "symbol-function",
    "symbol-value",
    // Hash tables and vectors, read side only.
    "gethash",
    "hash-table-count",
    "hash-table-keys",
    "hash-table-values",
    "make-hash-table",
    "make-string",
    "make-vector",
    "vconcat",
    "vector",
    // Buffers, windows, and files, read side only.
    "buffer-file-name",
    "buffer-live-p",
    "buffer-name",
    "buffer-size",
    "buffer-string",
    "buffer-substring",
    "buffer-substring-no-properties",
    "bufferpos-to-filepos",
    "current-buffer",
    "expand-file-name",
    "file-directory-p",
    "file-exists-p",
    "file-readable-p",
    "file-writable-p",
    "get-buffer",
    "line-beginning-position",
    "line-end-position",
    "point",
    "point-max",
    "point-min",
    "pos-bol",
    "pos-eol",
    "selected-window",
    "window-buffer",
    // Miscellaneous pure helpers.
    "always",
    "concat",
    "featurep",
    "message",
    "plist-get",
    "plist-member",
    "alist-get",
    "float-time",
    "current-time",
    "time-convert",
    "type-of",
];

static INDEX: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| PURE_STANDARD_EMACS_LISP_FUNCTIONS.iter().copied().collect());

/// Whether `head` names a standard Emacs Lisp function.
///
/// Matched exactly: Emacs Lisp reads symbols case-sensitively, so `Length` is
/// not `length` and must stay opaque.
#[must_use]
pub fn is_pure_standard_emacs_lisp_function(head: &str) -> bool {
    INDEX.contains(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_calls_are_transparent() {
        for head in ["length", "mapcar", "string-prefix-p", "seq-filter", "+"] {
            assert!(is_pure_standard_emacs_lisp_function(head), "{head}");
        }
    }

    #[test]
    fn lookup_is_case_sensitive() {
        // Unlike Common Lisp, a `.el` file may define `Length`, and a call to
        // it must not be mistaken for the subr.
        assert!(!is_pure_standard_emacs_lisp_function("Length"));
        assert!(!is_pure_standard_emacs_lisp_function("MAPCAR"));
    }

    #[test]
    fn assigning_and_mutating_forms_are_absent() {
        // `setq` and friends belong to the assignment table; the destructive
        // list operators would claim a purity they do not have.
        for head in [
            "setq",
            "setq-local",
            "push",
            "pop",
            "cl-incf",
            "nconc",
            "setcar",
            "nreverse",
            "sort",
        ] {
            assert!(!is_pure_standard_emacs_lisp_function(head), "{head}");
        }
    }

    #[test]
    fn a_package_helper_stays_opaque() {
        for head in ["magit-status", "my-helper", "org-agenda"] {
            assert!(!is_pure_standard_emacs_lisp_function(head), "{head}");
        }
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut names = PURE_STANDARD_EMACS_LISP_FUNCTIONS.to_vec();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }

    #[test]
    fn every_name_is_lowercase_as_emacs_lisp_writes_them() {
        for name in PURE_STANDARD_EMACS_LISP_FUNCTIONS {
            assert!(
                !name.chars().any(|character| character.is_ascii_uppercase()),
                "{name}"
            );
        }
    }
}
