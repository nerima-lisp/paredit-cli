# paredit-feature-lisp-analysis

Fifteen reports about the parts of Common Lisp that are not ordinary forms.

## Responsibilities

Most of this tool reads a balanced-parens tree and asks questions about the
forms in it. That works because most of Common Lisp *is* forms. The parts that
are not are where the language keeps its sharpest edges, and where an
S-expression-shaped analysis sees nothing at all:

- **Reader syntax.** `#+`/`#-` decide at read time whether text exists;
  `#.` runs code at read time; `#n=`/`#n#` build objects the printer cannot
  round-trip. All three are invisible to a rule that looks at the parse,
  because they act *before* there is a parse. `read-conditionals`,
  `read-time-eval`, and `circular-literals` report them.
- **Grammars inside a form.** `loop` has its own clause grammar and `format`
  has its own directive language, both carried in what a tree walker sees as
  an opaque symbol soup and an opaque string. `loop` and `format-directives`
  read them.
- **Symbol identity.** `readtable-case` reports the places where a symbol's
  spelling is load-bearing; `package-locks` reports redefinition of standard
  symbols, which CLHS leaves undefined and implementations may refuse.
- **CLOS.** A generic function's behaviour is distributed across `defgeneric`,
  every `defmethod`, and the method-combination rules. No single form holds
  it, which is why `method-combination`, `class-hierarchy`, and
  `generic-dispatch` are separate reports rather than fields on one.
  `duplicate-methods` and `duplicate-slots` join them: both ask whether one
  CLOS declaration was written twice, which needs the same reading of a
  `defmethod` specializer list and a `defclass` slot specifier.
- **Macros.** `macro-expansion` simulates a `defmacro` template against its own
  call sites; `macro-hygiene` reports the two ways a template goes wrong —
  capturing a caller's variable, and evaluating a caller's form more than once.
- **Conditions.** `restarts` pairs `restart-case` establishments against
  `invoke-restart` uses, and reports each side that has no counterpart.

## Boundaries

Common Lisp only. Every report says so in its output rather than returning an
empty finding list, which a consumer cannot distinguish from a clean file.

Nothing here evaluates. `macro-expansion` substitutes into a template and stops
at the first construct it cannot substitute into; it does not iterate to a
fixpoint, does not expand nested macros, and reports what it declined to do.
