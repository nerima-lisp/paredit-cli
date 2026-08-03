# paredit-feature-lint-pathname-io

Lint rules whose subject is Common Lisp's pathname and stream layer: how a file
gets named, what happens when it is already there, and how long the stream that
opens it stays usable.

Five rules, in two families that share the package because they share a
vocabulary — every one of them anchors on the same handful of filesystem
operators, so they see the same nodes and cost one head lookup between them.

**Naming a file.** `pathname-built-by-concatenation` reports a designator glued
together with `concatenate` or `format` around a separator;
`directory-without-wild-component` reports a listing request that matches only
the directory it names; `pathname-component-compared-case-sensitively` reports a
`string=` against a component whose case is the host's to choose.

**Opening and closing one.** `output-stream-without-if-exists` reports a file
opened for output with no policy for an existing one;
`with-open-file-result-captures-stream` reports a closure or an aggregate that
carries the stream out past the form that closes it.

## Every premise here was run, not recalled

The rules are documented with the SBCL 2.6.0 expression that demonstrates each
defect, because three of the five were proposed with a justification that turned
out to be wrong. `directory-without-wild-component` in particular is *not* about
CLHS underspecifying anything — it specifies the behaviour completely, and the
behaviour is the surprise. Each rule's module docstring records what was checked
and what was refuted.

Two rules that were proposed for this package are not here.
`open-without-unwind-protect` is `unclosed-stream` in
`paredit-feature-lint-safety`, which already anchors on `let`/`let*` and already
distinguishes "never closed" from "closed only on success". A `read-line` EOF
rule was dropped: CLHS specifies `eof-error-p` exactly, and the reframing that
would have survived that — a `NIL` reaching a function that cannot take one — is
refuted by `(length (read-line s nil))` returning `0` rather than signalling.

## Scope

Common Lisp only. Every rule uses `HeadFilter::Heads`, so a file that names none
of these operators pays one hash lookup per list node and nothing else. Note
that a call inside a reader conditional is invisible to all of them: this repo's
dialect-aware parse folds `#+sbcl (open …)` into a single atom, so there is no
head to anchor on. That is a coverage limit, not a source of false positives.
