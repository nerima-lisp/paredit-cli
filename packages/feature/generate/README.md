# paredit-feature-generate

Six generators, all scoped to Common Lisp, all producing new source from
analysis this tool already does elsewhere.

## Responsibilities

- **`defpackage`.** Reads one file's top-level definitions and its qualified
  symbol references, and emits a `defpackage` form: every public definition
  (a name not starting with `%`) becomes an export, every referenced package
  other than `cl` becomes a use.
- **`defsystem`.** Walks a directory for Lisp sources and emits an ASDF
  `defsystem` form: one `(:file "name")` component per source file, and a
  `:depends-on` entry for every package a file uses that no file in the
  directory defines.
- **`tests`.** Reuses `inspect test-map`'s coverage pairing to find definitions
  no test names, and emits one `deftest` skeleton per untested definition.
- **`accessors`.** Selects one `defclass` form and adds `:accessor` to every
  slot that has neither `:accessor`, `:reader`, nor `:writer`.
- **`defgeneric`.** Selects one file (or one generic function's method group)
  and, for a name that has `defmethod` forms but no `defgeneric`, synthesizes
  one from the methods' congruent lambda list.
- **`docstring`.** Selects one definition and inserts a docstring template at
  the position Common Lisp expects it: after the lambda list for a function or
  macro, at the fixed value slot for a variable, before the slots for a
  `defstruct`, or as a `(:documentation ...)` option for a class, condition,
  or generic function.

Every generator refuses a non-Common-Lisp dialect rather than emitting
CLOS or ASDF forms a caller cannot use, and every generator that rewrites an
existing file re-parses the result before returning success, so a caller never
receives source this tool would refuse to read back.
