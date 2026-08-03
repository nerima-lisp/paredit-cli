# Configuration

With 313 lint rules and 460 commands, passing every knob as a flag
stopped scaling. `paredit.toml` is the answer: a small, strictly validated file
that sets the defaults a repository wants, so a command line carries only what
is unusual about *this* invocation.

A flag always wins over a file. Configuration sets defaults; it never takes an
option away from the person or agent running the command.

## Where the file lives

`paredit` looks for `paredit.toml`, then `.paredit.toml`, in each of these
places. Later layers replace earlier ones **key by key** — a layer that sets
`format.indent` leaves every other key exactly as the layer below it left it.

| # | Layer | Where |
| --- | --- | --- |
| 1 | `default` | Built into the binary |
| 2 | `user` | `$PAREDIT_CONFIG_HOME`, else `$XDG_CONFIG_HOME/paredit/`, else `~/.config/paredit/` |
| 3 | `repository` | Beside the nearest ancestor `.git` |
| 4 | `directory` | Every directory from below the repository root down to the working directory, shallowest first |
| 5 | `environment` | `PAREDIT_<KEY>` for any key |

Discovery stops at the repository root. Without a `.git` anywhere above it,
only the working directory is consulted — walking to `/` would let a stray
`/tmp/paredit.toml` configure an unrelated project.

Lists replace rather than accumulate. A nested file that sets `lint.disable`
replaces the repository's list; it does not add to it. A list assembled from
three files is a list nobody can predict, and `paredit config show` would have
no single line to point at.

## `.editorconfig`

A discovered `.editorconfig` fills the four `[format]` keys its properties map
onto — `indent_size` → `format.indent`, `max_line_length` →
`format.max-inline-width`, `insert_final_newline` →
`format.insert-final-newline`, `trim_trailing_whitespace` →
`format.trim-trailing-whitespace` — beneath every `paredit.toml` layer:
`paredit.toml` always wins on a key both files set, and an `.editorconfig`
only fills a key `paredit.toml` leaves unset. Only a `[*]`/`[**]` section (or
a property written before any section at all) is read; a narrower glob
(`[*.py]`) is skipped, since which file a command will run against is not yet
known at configuration-load time. A project with no `.editorconfig` behaves
exactly as it did before this existed.

## Getting started

```sh
paredit config init            # write a documented paredit.toml
paredit config init --dry-run  # print it instead
paredit config schema          # every key, type, default, and variable
```

The generated file is produced from the same schema table the validator reads,
so it can never document a key the binary does not have.

## A worked example

```toml
# paredit.toml at the repository root
extends = ["./shared/lint-baseline.toml"]

[dialect]
default = "common-lisp"

[paths]
exclude = ["vendor", "third-party"]
max-depth = 12

[format]
indent = 2

[lint]
preset = "recommended"
disable = ["one-armed-if"]
deny = ["correctness"]
fail-on = "error"
```

```toml
# tests/paredit.toml — looser rules, only under tests/
[lint]
preset = "minimal"
```

```sh
$ paredit config show --changed-only --output text
source	repository	/repo/paredit.toml
source	directory	/repo/tests/paredit.toml
lint.preset	"minimal"	directory	/repo/tests/paredit.toml:2
lint.fail-on	"error"	repository	/repo/paredit.toml:19
format.indent	2	repository	/repo/paredit.toml:12
```

Every row names the file and the line that won. That is the question a layered
configuration creates, so it is the question `config show` is built to answer.

## Inheriting between files: `extends`

```toml
extends = ["../shared/paredit.toml", "./local-overrides.toml"]
```

Paths resolve relative to the file that wrote them. Inherited files are applied
*beneath* the file that names them, left to right, so the extending file always
wins. A cycle is an error rather than a hang, and chains deeper than 16 files
are refused.

`extends` is the only key that cannot be set from the environment. A CI run
whose configuration depends on a variable nobody reading the file can see is
not a configuration anyone can reason about.

## Environment overrides

Every key has one, derived from its name — uppercase, with `.` and `-` becoming
`_`:

```sh
PAREDIT_LINT_FAIL_ON=error paredit inspect lint src/
PAREDIT_LINT_DISABLE=one-armed-if,nil-comparison paredit inspect lint src/
PAREDIT_FORMAT_INDENT=4 paredit edit format --file src/x.lisp
```

Lists are comma-separated; that is the only spelling an environment can carry.
Run `paredit config schema` to see the variable for each key.

## Validating it

```sh
paredit config check
```

Exits `3` when any key is unusable, and reports every problem at once with the
file and line it sits on:

```
paredit.toml:2: error [wrong-type] `format.indent` expects an integer in 0..=16, found string
paredit.toml:5: error [not-a-choice] `lint.preset` does not accept "everything" (accepted values: minimal, recommended, pedantic, all)
paredit.toml:6: error [unknown-rule] `lint.disable` names "nil-comparson", which is not a registered lint rule (did you mean "nil-comparison"?)
paredit.toml:7: error [unknown-key] `lint.fail_on` is not a recognised configuration key (did you mean `lint.fail-on`?)
```

Rule names are checked against the registry this binary actually ships, so a
rule renamed by an upgrade is caught by `config check` rather than by silently
linting less than you thought.

A key that fails validation is **not applied**: the layer below it stands. Half
a configuration is worse than none.

The `code` in brackets is stable and machine-readable — `unknown-key`,
`wrong-type`, `not-a-choice`, `out-of-range`, `unknown-rule`,
`unknown-category`, `conflict`, `empty-value` — so an agent can act on the
class of problem without matching English prose.

## How a setting reaches a command

Most keys arrive as the flag the command already documents. Before `clap` sees
the command line, `paredit` appends the configured value for any flag the
command declares and the caller did not pass. So `lint.disable` becomes
`--exclude`, and `format.indent` becomes `--indent`.

That is why "a flag always wins" needs no policy: a flag already on the command
line is never injected over. It is also why `--help` stays honest — the flag a
command documents is the only way that setting reaches it.

To see exactly what your configuration will do to a command:

```sh
$ paredit config show --for "inspect lint" --output text --changed-only
source	repository	/repo/paredit.toml
lint.disable	["nil-comparison"]	repository	/repo/paredit.toml:2
injects	lint.disable	--exclude	nil-comparison
```

`[dialect]` and the discovery half of `[paths]` are the exception: they act
below the argument layer, inside dialect detection and the directory walk,
because most commands have no flag for them.

Two rules keep the rewrite from ever being the reason a command fails. A
configuration with errors contributes nothing at all — you get a warning
pointing at `paredit config check` and the command runs unconfigured. And if
the rewritten command line does not parse for any reason, the original is used
and the dropped keys are named.

## Turning it off

| Flag | Variable | Effect |
| --- | --- | --- |
| `--config <FILE>` | `PAREDIT_CONFIG` | Read exactly this file; skip the user, repository, and directory layers |
| `--no-config` | `PAREDIT_NO_CONFIG` | Read no files at all |
| `--no-config-env` | `PAREDIT_NO_CONFIG_ENV` | Ignore the `PAREDIT_*` setting overrides |
| `--from <DIR>` | — | Resolve discovery from this directory instead of the working one |

The flags exist on the `config` namespace; the variables work everywhere,
which is why they exist — 460 commands do not each need three more flags.

`PAREDIT_NO_CONFIG=1 PAREDIT_NO_CONFIG_ENV=1` is the reproducible-CI
combination: it pins the run to the built-in defaults plus whatever the command
line says.

## What the file may contain

Run `paredit config schema` for the authoritative list on your installed
version. The tables are:

| Table | What it settles |
| --- | --- |
| *(top level)* | `extends` |
| `[dialect]` | Which dialect to assume, and whether to force it over the extension |
| `[paths]` | Workspace discovery: exclusions, hidden and generated directories, depth |
| `[cache]` | Where workspace discovery caches its results between runs |
| `[format]` | Indent width, inline width, block-comment reindenting, trailing-comment alignment, blank-line normalization, per-symbol indent styles, per-style width profiles, reader-prefix printing, numeric-literal letter case, let-binding value alignment, and final-newline/trailing-whitespace toggles for `edit format` |
| `[lint]` | Preset, rule selection, severity overrides, baseline, gate |
| `[macro-hygiene]` | Whether any macro hygiene risk fails `inspect macro-hygiene` |
| `[output]` | Default format, verbosity, token budget, message language |

Despite its name, `[macro-hygiene]` settles one command rather than the subject.
`macro-hygiene.fail-on-risk` arms `inspect macro-hygiene --fail-on-risk`, which
is all-or-nothing: it exits 3 if *any* of the five risks is reported — variable
capture, multiple evaluation, parameter reordering, deep quasiquote nesting, or
a missing Emacs Lisp editor declaration — and there is no way to gate on a
subset. Per-risk enforcement is `inspect lint`'s job instead: each of the five
also ships as its own lint rule, so `lint.deny` and `lint.fail-on` can fail the
build on `macro-variable-capture` alone while leaving the other four as
warnings, and `lint.baseline` and inline `paredit:ignore` comments apply too.

## The dialect it is written in

Deliberately a strict subset of TOML: tables, dotted keys, strings, integers,
booleans, and arrays of those. Floats, datetimes, inline tables, arrays of
tables, and multi-line strings are refused *with a line number* rather than
accepted and ignored. A key this tool does not understand is far more likely to
be a typo than a feature request, and the cost of guessing wrong is a setting
that silently does nothing.
