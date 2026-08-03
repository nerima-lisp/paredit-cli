# paredit-feature-lint-fennel-janet-idiom

Lint rules whose subject is Fennel or Janet specifically.

Both dialects are first-class in the parser, and until this package the
catalogue had seven rules in scope for either of them — six of which model
several dialects at once and none of which encodes a fact about Fennel or Janet
that is not also true elsewhere. Every rule here is keyed on something the
language's own reference, compiler, or shipped linter states.

| rule | dialects | keyed on | primary source |
| --- | --- | --- | --- |
| `var-never-set` | Fennel, Janet | `var`, `var-` | Fennel `src/linter.fnl` `check-unused`, `"declared as var but never set"` |
| `fennel-deprecated-form` | Fennel | `global`, `require-macros`, `pick-args` | `reference.md`, "Deprecated Forms" |
| `fennel-each-over-non-iterator` | Fennel | `each` | `specials.fnl` `SPECIALS.each`, which emits Lua's generic `for … in` |
| `janet-empty-loop-body` | Janet | `loop`, `seq`, `catseq` | `boot.janet` `check-empty-body`, `maclintf :normal "empty loop body"` |
| `janet-mutating-immutable-literal` | Janet | `put`, `array/*`, `buffer/*` | `src/core/value.c` `janet_put`, which panics on a struct |

## Third-party audit

Run over code nobody here wrote: 288 `.fnl` files (`fennel-lang/fennel`,
`Olical/conjure`, `rktjmp/hotpot.nvim`, `udayvir-singh/tangerine.nvim`,
`min-love2d-fennel`) and 241 `.janet` files (`janet-lang/janet`, `spork`,
`jpm`, `circlet`, `andrewchambers/janet-sh`).

| rule | candidates | findings | adjudication |
| --- | --- | --- | --- |
| `var-never-set` (Fennel) | 210 | 8 | all true; 5 more were false positives from a project-local macro expanding to `set`, now suppressed |
| `var-never-set` (Janet) | 464 | 31 | all true; 5 more were the same macro false positive, now suppressed |
| `fennel-deprecated-form` | 29 | 28 | all true; the 29th is a malformed `(global)` the arity guard declines |
| `fennel-each-over-non-iterator` | 194 | 1 | true — and it is `fennel-lang/fennel`'s own assertion that this shape raises |
| `janet-empty-loop-body` | 174 | 0 | 2 findings before the `:iterate` narrowing, both the deliberate drain idiom |
| `janet-mutating-immutable-literal` | 695 | 0 | unproven: a real denominator, no instance in this corpus |

## Known parser limitation for Janet

Janet's long strings are delimited by a run of backticks of any length
(`src/core/parse.c`, `longstring`), and this repository's reader does not
implement them: a backtick is neither a delimiter nor whitespace for
`Dialect::Janet`, so it is absorbed into an atom and the string's contents are
read as code. Over 241 files from `janet-lang/janet`, `spork`, `jpm`, `circlet`
and `janet-sh`, 9 fail to parse outright and 41 more parse into a tree
containing nodes that lie inside a long string's body. Docstrings written with
```` ``` ```` are the dominant cause, and `src/boot/boot.janet` — Janet's own
core library — is one of the nine.

Every rule here is therefore blind on roughly 4% of real Janet files and can be
handed phantom forms on another 17%. The rules' quote guard does not help:
these nodes are not quoted, they are prose. Fixing the reader is out of this
package's scope; the measurement is recorded here so the limitation is not
rediscovered.
