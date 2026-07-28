# paredit-feature-lint-emacs-lisp

Lint rules for Emacs Lisp.

## Responsibilities

The rules here are the ones whose subject is Emacs Lisp itself rather than
S-expression shape, so none of them has a Common Lisp counterpart to share:

- **File-level conventions.** `lexical-binding` on line 1 and the
  `;;;###autoload` cookie are comments that change what the code around them
  means. Neither is visible to a rule that only looks at the tree, and both
  fail *silently* in Emacs — a misplaced cookie is ignored, a missing header
  turns every `let` in the file dynamic.
- **Customization forms.** `defcustom` without `:type` degrades the
  Customize UI to a raw sexp editor; without `:group` the option is
  unreachable from the group tree. Both are accepted by the evaluator.
- **Obsolete `cl.el` spellings.** `flet`, `loop`, `case`, and the rest were
  removed in Emacs 27. Code using them stops loading, which is a compile
  error rather than a style opinion.

Every rule declares `Dialect::EmacsLisp` and nothing else, so it costs a
Common Lisp run nothing: the dispatcher skips a rule whose scope excludes the
file's dialect before walking anything.
