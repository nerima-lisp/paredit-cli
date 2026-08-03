# paredit-feature-lint-elisp-idiom

Emacs Lisp lint rules for defects that the byte compiler does not report.

Every rule here is `EMACS_LISP_ONLY` and anchored on `HeadFilter::Heads`. The
default `dialect_scope()` is `COMMON_LISP_ONLY`, so a rule in this crate that
forgets the override never runs on the dialect it was written for.

| rule | what it reports |
| --- | --- |
| `elisp-keymap-binds-non-command` | a key bound to a same-file `defun` whose body has no `(interactive)`, so pressing the key signals `commandp` failure |
| `elisp-interactive-arity-mismatch` | a command whose `(interactive)` spec supplies fewer arguments than its lambda list requires, so `call-interactively` signals `wrong-number-of-arguments` |
| `elisp-hook-lambda` | a `lambda` passed to `add-hook`/`remove-hook`, against the explicit advice in `add-hook`'s own docstring |
| `elisp-save-excursion-set-buffer` | `save-excursion` wrapping `set-buffer`, which the byte compiler also warns about and `with-current-buffer` says better |
| `elisp-require-obsolete-cl` | `(require 'cl)` / `(require 'cl-compat)`, both of which live in `lisp/obsolete/` and print a deprecation notice when loaded |

## What this crate is not

`elisp-defcustom-missing-type` already exists in `paredit-feature-emacs-lisp`
and is not duplicated here. Neither is the `lexical-binding` cookie, which
`elisp-missing-lexical-binding` and `elisp-unreachable-lexical-binding` cover,
nor the unprefixed `cl.el` macro names, which `elisp-obsolete-cl-alias` covers
— `elisp-require-obsolete-cl` is the `require` half that rule does not reach.

`elisp-hook-lambda` deliberately steps aside for a *quote-prefixed* lambda:
`(add-hook 'h '(lambda () …))` is already reported by `elisp-quoted-lambda`
with a strictly stronger complaint, and two findings on one form is noise.

## Cost

Every rule is `Heads`-filtered, so a file that spells none of the anchor heads
pays one hash lookup per node and nothing else. The two rules that need more
than their own node — `elisp-keymap-binds-non-command`, which resolves a bound
symbol against the file's own definitions — build that index lazily and only
once a candidate binding has already been found, so a keymap-free file never
walks the tree twice. `is_unevaluated_at` is likewise consulted only when a
finding is otherwise ready to report: it costs the node's depth, not the
file's size.
