# paredit-feature-lint-elisp-depth

Emacs Lisp lint rules for the **runtime** surface — the process and timer state
that outlives the form that set it.

The Emacs Lisp rules that already ship are definition-shaped: they read
`defcustom`, `defun`, `interactive` and `require`. This package reads what
happens when the code runs.

| Rule | Category | Severity | Fixability | Heads |
| --- | --- | --- | --- | --- |
| `elisp-process-filter-assumes-whole-output` | `Suspicious` | `Warning` | `ReportOnly` | `set-process-filter`, `make-process`, `make-network-process` |
| `elisp-repeating-timer-handle-discarded` | `Resource` | `Warning` | `ReportOnly` | `run-with-timer`, `run-at-time`, `run-with-idle-timer` |

Both set `dialect_scope()` to `EMACS_LISP_ONLY`. The default is
`COMMON_LISP_ONLY`, so a rule here that forgot it would never run and no test
would say so — `every_rule_is_scoped_to_emacs_lisp` and
`the_same_source_read_as_common_lisp_fires_nothing` pin both halves of that.

## Six proposals, two rules

Every premise was run against GNU Emacs 31.0.91 rather than assumed, and four
of the six proposals died there. Each rule's module docstring carries the
expression that settled it.

**Refuted, and dropped:**

- **`goto-char` arithmetic without a bounds check.** `goto-char` does not
  signal — it clamps. `(goto-char 999)` in a six-character buffer leaves point
  at `point-max` and returns normally. There was no defect to report.
  (`forward-char` signals `end-of-buffer` and `buffer-substring` signals
  `args-out-of-range`, but neither was the proposed rule.)
- **`setq` on a buffer-local variable.** `setq` on an automatically
  buffer-local variable creates a *buffer-local* binding and leaves the global
  alone, and all ten common mode variables tested are `local-variable-if-set-p`.
  The rule was backwards — the same way a previous batch's version of it was.
- **A lambda passed to `advice-add` being unremovable.** `advice-remove`
  compares with `equal`, and two separately byte-compiled identical lambdas are
  `equal`. The advice comes off. Only a lambda closing over a *different* value
  resists removal.
- **`kill-buffer` inside a loop over `buffer-list`.** `buffer-list` returns a
  fresh list on every call, so the iteration is over a snapshot.

**Verified true, but not locally decidable — also dropped:**

- **Narrowing without `save-restriction`.** The premise holds: narrowing
  escapes to the caller, and `save-excursion` does not restore it. But
  persistent narrowing is a legitimate and widespread design — todo-mode,
  rmail, gnus and ediff all narrow as their display model — and 12 of the 93
  findings were in functions whose own name says they narrow for a caller that
  wraps them. 112 findings over 1044 candidates, mostly on correct code.
- **Display text properties marking the buffer modified.** The premise holds:
  `put-text-property` sets `buffer-modified-p` and pushes an undo entry, and
  `with-silent-modifications` is the fix (`inhibit-modification-hooks` is
  *not* — it silences hooks and leaves the flag set). But whether it is a
  defect depends on the **caller**: `font-lock-ensure` leaves
  `buffer-modified-p` nil because font-lock silences the whole pass, so every
  `'face` write inside a fontification function is correct. 500 findings over
  2679 candidates.

## False-positive audit

3916 files parsed across GNU Emacs 31.0.91's own `lisp/` tree, 2585
third-party ELPA packages, and a user configuration.

| corpus | files parsed | filter candidates | timer candidates | findings |
| --- | --- | --- | --- | --- |
| GNU Emacs `lisp/` | 1470 | 308 | 248 | 0 |
| third-party ELPA | 2439 | 320 | 270 | 2 |
| user config | 7 | 0 | 2 | 0 |

The two findings are true positives in `affe`: two repeating timers (0.5 s and
0.1 s) whose handles are discarded. The same file's `(run-at-time 0.5 nil …)`
three lines later is correctly not reported.

Two false positives found during the audit are now pinned as tests:
`pulse.el:260` passes `REPEAT` of `0`, which Emacs treats as a one-shot, and
`affe.el:77` is a correctly written filter that accumulates a value *derived*
from its chunk.

## Cost

Both rules do their head comparison — and the timer rule its `REPEAT` check —
before anything reaches `root_view()`. At the density the `clean/forms/*` gate
models, both are flat in file size and within 1.6x of a shipped rule measured
in the same process. See `repeating_timer_handle_discarded`'s module docstring
for the full table and for why reaching a node's parent is expensive.
