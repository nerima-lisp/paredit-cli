# paredit-feature-lint-carp-idiom

Lint rules for Carp, the statically typed, ownership-tracked Lisp that compiles
to C.

Carp is the last of this tool's ten dialects to get rules of its own. It is
also the dialect where a linter has the least to say, and the reason is worth
stating up front: **Carp's compiler already rejects almost everything a lint
rule would want to report.** Its linear type system catches use-after-move,
invalid references and dangling references outright — `docs/Memory.md` walks
through each in turn, and each ends "the memory management system detects this
and reports an error". A rule that re-reports a compile error is worthless.

So the rules here are confined to what the compiler has *no opinion about*:
spellings that build cleanly and silently while being wrong. That is a small
category in Carp, and this package is small accordingly.

## Rules

| Rule | Category | Severity | Fixability | Heads |
| --- | --- | --- | --- | --- |
| `carp-deprecated-thread-macro` | Portability | Warning | Fixable | `=>`, `==>` |

`core/ControlMacros.carp:27,31` declares `=>` and `==>` deprecated in favour of
`->` and `-->`. The declaration expands to `meta-set!` and nothing else, and
the compiler reads that metadata key in exactly two places — `primitiveInfo`
(the REPL's `(info …)` command) and the HTML doc renderer. **No compilation
path reads it**, so the deprecated spelling builds with no diagnostic at all.
Carp's own `core/Binary.carp:68,77` still uses `==>`.

The fix is a rename rather than a rewrite: `=>` and `->` are both defined
`(defmacro _ [:rest forms] (thread-first-internal forms))`, with identical
bodies. It is withheld when the file defines its own `->` or `-->`.

## What this workspace's reader does with Carp

Investigating the rules turned up four reader defects, all of them larger than
the rules. They are recorded here because they bound what any Carp rule can
do; fixing them belongs in `core/syntax`, not this package.

Measured over `carp-lang/Carp` at 248 `.carp` files:

**1. `@` and `&` are not reader prefixes.** Carp's guide
(`docs/LanguageGuide.md`, "Reader Macros") defines `&x` as `(ref x)` and `@x`
as `(copy x)`. `reader_policy.rs` routes Carp through `classify_legacy`, which
implements neither. So `@x` lexes as a single atom `"@x"`, and `@(f x)` lexes
as a **bare `@` atom followed by a sibling list** — which inflates the
enclosing call's arity by one:

```text
(f @(g y))   Carp    => 3 children: ["f", "@", "(g y)"]     wrong
(f @(g y))   Clojure => 2 children: ["f", "@(g y)"]         right
```

1493 such bare sigil atoms occur in 116 of the 248 files. Byte spans stay
intact, so a round trip is lossless — but **no arity or argument-position
analysis is trustworthy for Carp**, which is why the rule here keys on the head
symbol alone.

**2. A string literal directly after `@` is not lexed as a string.** Because
`@` glues to the following token, `@"…"` is read as an atom that swallows the
opening quote. `(f @"a b")` silently becomes *two* atoms, `@"a` and `b"`, with
no error; `(f @"{")` fails to parse outright. 46 silently split string atoms
occur across 10 files that otherwise parse cleanly, and this is the cause of
three of the six outright parse failures (`core/Map.carp`, `core/Pattern.carp`,
`core/Test.carp`).

**3. Character literals are not recognized.** Carp spells them `\a`, and
`character_literal_prefix_width` has arms for Scheme, Racket, Clojure and Emacs
Lisp but none for Carp. `\a` and `\space` survive by luck; `\{`, `\}`, `\[`,
`\]`, `\(`, `\)` and `\"` do not, because the delimiter is read as a real
delimiter. This is the cause of `core/Format.carp` (`\{`) and, with defect 2,
`examples/json_parser.carp` (`\]`, `\"`).

**4. `#"…"` pattern literals are not recognized.** Carp's `Pattern` type has a
literal syntax, used 36 times in 2 files. It is the cause of
`test/pattern.carp`.

Together these make **6 of 248 files fail to parse**, four of them in `core/`
— and a file that does not parse is one no command in this tool can say
anything about.

A fifth, benign observation: Carp's unquote is `%` and its unquote-splicing is
`%@` (`docs/Quasiquotation.md`), neither of which the reader recognizes as a
prefix. Everything textually inside a `` ` `` template therefore reads as data,
which suppresses findings rather than inventing them.

## Cost

`HeadFilter::Heads` means the rule is never invoked at all on a file with no
`=>` or `==>` — 239 of the corpus's 248 files. When it does fire, a finding
costs one `SyntaxTree::root_view`, which materializes the document; on a
67140-byte fixture that measured 2.21 M ns/call against 4.50 M at double the
size, a ratio of 2.04 — linear in file size, as `root_view` is. The shipped
`self-recursive-tail-call`, timed in the same pass, ran 161 ns/call at ratio
1.01. The per-finding document cost is a `root_view` property this package
shares with shipped rules elsewhere in the workspace; fixing it belongs in
`core/syntax`.

## Candidates that were investigated and rejected

- **Ownership and `@`/`&` misuse.** Compiler-caught (`docs/Memory.md`), and in
  any case unreachable given reader defect 1.
- **`fmt` specifier/argument mismatch.** `core/Format.carp` raises
  `macro-error` at expansion time; the guide states the check explicitly.
- **`Array.unsafe-nth`.** Used ~20 times legitimately inside `core/Array.carp`
  where the index is provably in bounds. A false-positive machine.
- **`Debug.sanitize-addresses`.** Four uses in the corpus, all deliberate, all
  in `bench/`.
- **`Debug.trace`, `Debug.leak-array`, `Pointer.unsafe-alloc`, `Unsafe.*`.**
  Defensible in principle but with zero or all-deliberate corpus occurrences,
  so nothing could be demonstrated. Left unwritten rather than shipped on a
  zero denominator.
