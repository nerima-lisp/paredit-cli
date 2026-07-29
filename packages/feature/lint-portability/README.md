# paredit-feature-lint-portability

Lint rules whose subject is *where the code will run*: what it assumes about
the implementation, the character set, the host's path syntax, and the reader's
floating-point defaults.

Two categories share the package because both report the same kind of defect —
code that is correct here and wrong somewhere else — differing only in what
"somewhere else" means. `portability` is another implementation or another
host; `numeric-precision` is another value of `*read-default-float-format*`, or
the same computation with rounding.

Every rule here is silent on the majority of code and, when it does fire,
names the assumption rather than the fix: telling someone their `sb-ext:`
call is SBCL-specific is useful, and guessing at a portable replacement for it
is not.
