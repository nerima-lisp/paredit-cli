# paredit-feature-schema-check

The `schema` namespace: `paredit schema check` validates one S-expression
data file (an *instance*) against a small schema language of its own,
`defschema`, written as ordinary Lisp forms rather than an embedded foreign
syntax.

```lisp
(defschema config
  (fields
    (:name (:type string))
    (:port (:type integer :min 1 :max 65535))
    (:mode (:type string :one-of ("dev" "staging" "prod")))
    (:label (:type string :matches "^[a-z][a-z0-9-]*$" :optional t))))
```

## Nothing here evaluates anything

This crate never runs the file it reads, on either side of the check. The
schema language has a closed, five-member type vocabulary (`string`,
`integer`, `boolean`, `symbol`, `list`) and four refinements (`:min`, `:max`,
`:one-of`, `:matches`), and a `:type` or refinement keyword this crate does
not recognize is rejected with a parse error — never interpreted as code.
There are no predicates, no lambdas, and no evaluated expressions anywhere in
the grammar.

`:matches` is a small hand-rolled glob matcher (`*` = any run of characters,
`?` = one character, everything else literal), not a regular expression: this
tool stays dependency-light by design, and a pattern written with regex
syntax such as `^[a-z]+$` will not behave like one.

## Instance shapes

An instance may be alist-shaped (`((key . value) ...)` or `((key value)
...)`) or plist-shaped (`(:key value ...)`); both validate identically. A
file that is neither is refused with a clear "not a recognizable instance
shape" error rather than a confusing cascade of per-field mismatches.

Depends on `paredit-core-syntax` and `paredit-core-cli` only.
