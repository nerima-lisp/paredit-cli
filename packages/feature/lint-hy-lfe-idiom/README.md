# paredit-feature-lint-hy-lfe-idiom

Lint rules for Hy and LFE.

Both languages are a Lisp surface over a non-Lisp runtime, and that is the
whole premise of this crate: a Hy defect is usually a **Python** defect wearing
Lisp syntax, and an LFE defect is usually an **Erlang** defect wearing Lisp
syntax. Neither is a Common Lisp defect, and a rule ported across from the
Common Lisp catalogue would be wrong in both directions — `hy-mutable-default-argument`
is the sharpest example, since the identical shape is *correct* Common Lisp.

Every premise below was settled by running the language, not by reasoning about
it: Hy 1.3.1 on CPython 3.14.6, and LFE 2.2.0 on Erlang 27.3.4.15.

| rule | dialect | category | severity | fix | heads |
| --- | --- | --- | --- | --- | --- |
| `hy-mutable-default-argument` | Hy | suspicious | error | report-only | `defn` `fn` `defn/a` `fn/a` |
| `hy-identity-comparison-with-literal` | Hy | suspicious | warning | report-only | `is` `is-not` `is_not` |
| `hy-bare-except` | Hy | conditions | warning | report-only | `except` |
| `lfe-catch-swallows-exit` | LFE | conditions | warning | report-only | `catch` |

`lfe-catch-swallows-exit` carries the `pedantic` tag; the other three are in the
default presets.

## Known reader limitations

These are properties of this workspace's reader, not of the rules, but they
bound what the rules can see and are recorded here because a lint pass that
silently reads the wrong tree is worse than one that reports nothing.

Measured over a third-party corpus of 2825 `.hy` and 2701 `.lfe` files:

**Hy — 16.6% of files do not parse at all.**

- **Interpolated f-strings are unimplemented** and account for 390 of the 469
  failures (13.8% of all files). `f"hi {name}"` is read as the token `f"hi`,
  then a brace list, then an unterminated string. The failure is loud, which is
  the better direction, but no rule here can see inside such a file.
- **`#!` shebang lines are not stripped.** 393 files (13.9%) parse at exit 0
  with the shebang read as two junk top-level atoms. Nothing head-filtered
  matches them, so no rule misfires, but the tree is wrong.
- **Bracket strings `#[[…]]` are parsed as code**, silently, in 33 files
  (1.2%). `#[[(defn evil [] 1)]]` produces a real `defn` node inside what is a
  raw *string*. This is the one that can make a rule fire on text that is not
  code, so `is_plain_list` refuses `#`-prefixed lists throughout this crate and
  a test pins it. `#[delim[…]]` fails to parse outright.

**LFE — 3.5% of files do not parse, and the larger problem is silent.**

- **`#B(…)` and `#M(…)` are orphaned from their list** in 243 files (9.0%).
  The reader emits a bare `#B` atom followed by a plain list, so `(f #B(1 2) X)`
  has four children instead of three. Arity is inflated at exit 0.
- **`|quoted atoms|` are split at whitespace** in 95 files (3.5%):
  `'|foo bar|` becomes the two atoms `'|foo` and `bar|`.
- **`#\(` and `#\)` restructure the tree.** `(list #\( #\))` parses as a nested
  list, because the reader takes the escaped paren as a real delimiter. Rare (1
  file), but unbounded in effect.

**Hy quasiquote is degraded, in the safe direction.** Hy spells unquote `~`,
which this reader does not recognize as a prefix — it emits a bare `~` atom.
`shared::QuoteState`'s `quasi` counter therefore never counts down for Hy, so
everything textually inside a Hy `` ` `` template reads as data and no rule
fires there. That suppresses findings rather than inventing them. LFE spells
unquote `,`, which *is* a real prefix, so the two-counter model works there
exactly as intended and is tested.

## Cost

Every rule applies its cheap head and shape checks before touching
`shared::node_context`, which calls `SyntaxTree::root_view` and so costs the
whole document rather than the node's depth. `lfe-catch-swallows-exit` needs
both the enclosing head and the quote state, and takes them from one descent
rather than two — measured, that is a 1.6x saving.

The residual per-finding cost is proportional to file size and is a property of
`root_view`, shared with the already-shipped `elisp-hook-lambda`: doubling the
file doubles that rule's ns/invocation just as it doubles these. Fixing it
belongs in `core/syntax`.
