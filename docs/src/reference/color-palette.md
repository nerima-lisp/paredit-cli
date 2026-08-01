# Terminal Color

Color in this tool is decoration painted on at the last moment. Every signal
it carries is also carried by a word, a symbol, or a prefix character that is
present whether or not color is on — so a reader who cannot distinguish two
of these hues, a reader on a monochrome terminal, and a `grep` in a pipeline
all see the same information.

Whether color is emitted at all is decided by `--color`, `NO_COLOR`,
`TERM=dumb`, `CLICOLOR_FORCE`, `FORCE_COLOR`, and an isatty check on the
destination stream (`NO_COLOR`/`TERM=dumb` win over `CLICOLOR_FORCE`/
`FORCE_COLOR` when both are present); see
[`--color` in the global options](api.md#global-options). This page is about
*which* colors may be emitted once that decision says yes.

## The approved palette

Every ANSI escape this tool writes comes from `Painter` in
`packages/core/cli/src/color.rs`. It is the only place in the codebase that
emits an SGR sequence, and it emits exactly these six:

| SGR code | ANSI name | Emitted by | Used for |
| --- | --- | --- | --- |
| `31` | red | `Painter::red` | the `Error` prefix on a failed run; the `error` severity word in a lint report; removed (`-`) lines in a diff |
| `33` | yellow | `Painter::yellow` | the `warning` severity word in a lint report |
| `32` | green | `Painter::green` | added (`+`) lines in a diff |
| `36` | cyan | `Painter::cyan` | the `try` prefix on a repair suggestion; `@@` hunk headers in a diff |
| `1` | bold | `Painter::bold` | `---`/`+++` file headers in a diff |
| `2` | dim | `Painter::dim` | de-emphasized decoration; defined but with no call site today |

`Painter::wrap` closes every one of them with the reset `0`, so no escape
outlives the string it was applied to.

## Color is never the only signal

Any new severity, status, or category color
**must pair with a mandatory text label** — a word, a symbol, or a prefix
character that is printed regardless of whether color is enabled. Adding a hue
that is the sole carrier of a distinction is not permitted, no matter how
well-chosen the hue is.

This is what makes the red/green pair above safe despite red–green being the
most common form of color vision deficiency. The two never distinguish
anything on their own:

- **Severity** uses red and yellow, and colors the severity *word itself* —
  the text reads `error` or `warning` either way. A lint report row is
  tab-separated and machine-readable before it is colored.
- **Diffs** use red and green, but the underlying text is a unified diff:
  every line already begins with a literal `-`, `+`, or space, and the
  coloring is applied to the finished diff rather than folded into its
  generation.
- **Failures** color a prefix that spells `Error`, and repairs one that
  spells `try`.

Remove all six codes and no output becomes ambiguous. That property is the
requirement; the specific hues are not.

## Why the base eight, and not 256-color or truecolor

The codes above are the original eight-color SGR set, which a terminal
resolves against the user's own theme. A reader who runs a palette chosen for
their vision — or simply one with better contrast than the default — gets
that remapping applied to this tool for free. Emitting `38;5;<n>` or
`38;2;<r>;<g>;<b>` would pin an exact RGB value and override the one
adaptation the reader has already made for themselves.

So extended-color escapes are excluded on purpose, not for lack of need.
Bold and dim are attributes rather than colors and remain available for
emphasis that must not depend on hue at all.

## Enforcement

`tests/cli/color_palette_contract.rs` reads `color.rs` as text and asserts
that the set of SGR codes it emits is exactly the six above. A seventh code —
including any 256-color or truecolor escape — fails the test until it is
added to this table, which is the point at which the pairing rule above has
to be answered for it.

The same test checks that this page still lists every code and the function
that emits it, so the table cannot drift from the source.

## HTML report color

`--output html` is a separate rendering surface from the terminal: it is CSS,
not SGR escapes, and it is styled by the `HTML_STYLE` constant in
`packages/core/cli/src/report/interop.rs`. It is a small, fixed palette in its
own right:

| Hex | CSS class | Used for |
| --- | --- | --- |
| `#D55E00` | `.gate.fail` | the gate line when `flat.gate_passed` is false |
| `#0072B2` | `.gate.pass` | the gate line when `flat.gate_passed` is true |

The same rule as the terminal palette applies here: **color is never the only
signal**. The gate line always prints the literal word `passed` or `failed`
next to the colored text, so the distinction survives grayscale printing,
`prefers-contrast`, or a reader who cannot separate the two hues.

These two hex values are not the terminal palette's red/green — they are
drawn from the Okabe-Ito colorblind-safe palette (vermillion and blue)
instead, because unlike the ANSI codes above, a browser renders a CSS hex
value exactly as pinned, with no user theme remapping it. Red/green is the
pair that is hardest to tell apart under deuteranopia/protanopia, so this
surface avoids it where the terminal palette does not need to.

`tests/cli/html_report_palette_contract.rs` reads `interop.rs` as text and
asserts that `HTML_STYLE` uses exactly these two hex values for `.gate.fail`
and `.gate.pass`, and that this page documents both.
