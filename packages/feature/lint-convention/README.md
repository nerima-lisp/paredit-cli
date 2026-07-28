# paredit-feature-lint-convention

Lint rules about what a definition *says about itself*: whether its name obeys
the convention its own defining form implies, whether it documents what it
takes, whether its declarations agree with its lambda list, and whether its
CLOS options are ones CLOS has.

Three of the four categories here (`naming`, `documentation`, and the softer
half of `object-system`) are tagged `pedantic`, which means `--preset
recommended` leaves them out. That is the honest placement: a project that has
not adopted `+constant+` is not making a mistake by not adopting it, and a rule
that fires on every definition in such a project is noise, not information.

The `declaration` rules are not pedantic and not warnings. `(declare (ignore
x))` on a variable the body goes on to use is a compile-time error in most
implementations and a latent one in the rest.
