# paredit-feature-semantic-report

Six reports that print what `paredit-core-semantics` already proves.

## Responsibilities

The semantics core builds three side tables per file — bindings, values, and
types — and until now every one of them was consumed only by lint rules, which
report the *conclusion* (`this divisor is zero`) and never the evidence. An
agent that wants to know whether a rewrite is safe has no way to ask "what do
you actually know about this expression?".

These six reports are that question, one table at a time:

- **`types`** prints the type table: every binding and expression the layer
  proved a type for, plus the declarations (`declare`, `the`, `declaim`) that
  contradict what inference derived. A contradiction surfaces as `Ty::Bottom`,
  which is the layer's honest record of an impossible declaration.
- **`narrowing`** prints the flow-narrowing sites: where a type predicate or a
  `typecase` clause proves something about a binding that is only true inside
  one branch.
- **`constants`** prints the fold: every expression that provably evaluates to
  a literal, and the file-level `defconstant` definitions.
- **`value-propagation`** prints the value table's reach — which bindings carry
  a constant, and for those that do not, *which* of the four propagation
  conditions they failed. The failure reason is the useful half: it is the
  difference between "not constant" and "constant, but reassigned on line 40".
- **`effects`** classifies each definition as pure, effectful, or unknown, by
  looking for the effect sources a Common Lisp body can reach. Many refactor
  safety questions ("may I hoist this?", "may I fold these duplicates?")
  reduce to this one, which is why it is here rather than inside each of them.
- **`magic-numbers`** prints every numeric literal inside a function or method
  body that has no name of its own — everything except a small idiomatic set
  (`0`, `1`, `-1`, `2`) and a `defconstant`/`defparameter`/`defvar`'s own
  direct value, which already has the name this report would otherwise
  propose giving it.

## Boundaries

Common Lisp only, and not by omission: `build_binding_table` and
`build_value_table` return an empty table for every other dialect, so these
reports would print zeroes rather than findings. The reports say so in their
output — `dialect_supported: false` — rather than emitting an empty finding
list that reads like a clean bill of health.

Ordering is imposed here, not inherited. The semantic tables are `HashMap`s, so
every report sorts by source span before printing; the byte-identical-output
contract depends on it.
