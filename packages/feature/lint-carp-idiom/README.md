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
the rules. **All four are now fixed in `core/syntax`**, which is where the fix
belonged; they are kept on record here because they are why the rule in this
package is shaped the way it is.

Measured over `carp-lang/Carp` at 248 `.carp` files, as they stood:

**1. `@` and `&` were not reader prefixes.** Carp's guide
(`docs/LanguageGuide.md`, "Reader Macros") defines `&x` as `(ref x)` and `@x`
as `(copy x)`. `reader_policy.rs` routed Carp through `classify_legacy`, which
implements neither. So `@x` lexed as a single atom `"@x"`, and `@(f x)` lexed
as a **bare `@` atom followed by a sibling list** — which inflated the
enclosing call's arity by one:

```text
(f @(g y))   was  => 3 children: ["f", "@", "(g y)"]     wrong
(f @(g y))   now  => 2 children: ["f", "@(g y)"]         right
```

1493 such bare sigil atoms occurred in 116 of the 248 files. Byte spans stayed
intact, so a round trip was lossless — but **no arity or argument-position
analysis was trustworthy for Carp**, which is why the rule here keys on the
head symbol alone. It still does: that was never the weaker choice.

Carp now has its own `classify_carp`, covering `&` (`Ref`), `@` (`Copy`), `~`
(`Deref`) and `$[…]` (`StaticArray`) as well as `'` and `` ` ``.

**2. A string literal directly after `@` was not lexed as a string.** Because
`@` glued to the following token, `@"…"` was read as an atom that swallowed the
opening quote. `(f @"a b")` silently became *two* atoms, `@"a` and `b"`, with
no error; `(f @"{")` failed to parse outright. 46 silently split string atoms
occurred across 10 files that otherwise parsed cleanly, and this was the cause
of three of the six outright parse failures (`core/Map.carp`,
`core/Pattern.carp`, `core/Test.carp`). Fixed as a consequence of defect 1: `@`
now prefixes the following *form*, so the string stays whole.

**3. Character literals were not recognized.** Carp spells them `\a`, and
`character_literal_prefix_width` had arms for Scheme, Racket, Clojure and Emacs
Lisp but none for Carp. `\a` and `\space` survived by luck; `\{`, `\}`, `\[`,
`\]`, `\(`, `\)` and `\"` did not, because the delimiter was read as a real
delimiter. This was the cause of `core/Format.carp` (`\{`) and, with defect 2,
`examples/json_parser.carp` (`\]`, `\"`).

**4. `#"…"` pattern literals were not recognized.** Carp's `Pattern` type has a
literal syntax, used 36 times in 2 files. It was the cause of
`test/pattern.carp`.

Together these made **6 of 248 files fail to parse**, four of them in `core/`
— and a file that does not parse is one no command in this tool can say
anything about. The corpus now parses at **0 failures**.

Two further defects were found while fixing the four above: `~` (deref) was
also missing, at 76 glued atoms in 15 files, and `,` is *whitespace* in Carp
(`emptyCharacters` in `src/Parsing.hs`) rather than an unquote, which the
legacy reader had been reading as a phantom `Unquote` prefix at 39 sites.

A benign observation that still stands: Carp's unquote is `%` and its
unquote-splicing is `%@` (`docs/Quasiquotation.md`), neither of which the
reader recognizes as a prefix. Recognizing them would make the interior of
every `` ` `` template read as code and *add* findings on macro bodies, so it
is deliberately deferred. Everything textually inside a `` ` `` template
therefore reads as data, which suppresses findings rather than inventing them.

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
