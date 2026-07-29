# paredit-feature-migrate

The migrate namespace: named, reviewable codemod recipes over the pattern
language.

## Why it is its own namespace

`query replace` gives a caller one pattern and one template. That is enough
for a rename and not enough for a migration, because a migration is *ordered*
and *scoped*:

- **Ordered.** `(if (not p) a nil)` should become `(unless p a)`, not
  `(when (not p) a)`. Which one it becomes depends entirely on which rule runs
  first, and a caller re-typing two `query replace` invocations in the right
  order every time is a caller who will eventually type them in the wrong one.
- **Scoped.** `(incf x)` → `(cl-incf x)` modernizes Emacs Lisp and breaks
  Common Lisp, where `incf` is the correct spelling. The dialect a recipe is
  correct for is part of the recipe, not part of the invocation.

So a recipe is a named, versioned, reviewable artifact: a list of steps, a
dialect scope, and a description of what it deliberately leaves alone.

## The format

Lisp, read by the same reader everything else here is read by, following the
precedent the custom lint rules (`.paredit/rules/*.lisp`) set:

```lisp
(defmigration nil-conditionals
  :description "one-armed `if' with a nil else-branch to `when' and `unless'"
  :dialects (common-lisp emacs-lisp)
  :steps ((:query (if (not ?test) ?then nil)
           :rewrite (unless ?test ?then)
           :note "first, so the general step below cannot claim a negated test")
          (:query (if ?test ?then nil)
           :rewrite (when ?test ?then))))
```

The built-in recipes in `recipes/` are embedded source text parsed by the same
function a project's `.paredit/migrations/*.lisp` goes through — they can
express nothing a user's recipe cannot, and they double as worked examples.

## Boundaries

The matcher and the rewriter are `paredit_core_syntax::selector`, shared with
the `query` namespace. This crate owns the recipe format, the catalogue, and
the step sequencing, and holds no matching logic of its own.
