# paredit-core-config

Layered `paredit.toml` configuration with per-key file and line provenance.

## Responsibilities

With 169 lint rules and roughly 275 commands, passing every knob as a flag
stopped scaling. This package is the answer, and it owns four things:

- **A strict TOML subset.** Tables, dotted keys, strings, integers, booleans,
  and arrays — nothing else. Every parsed key keeps the 1-based line it came
  from, because the whole point of `paredit config show` is answering *which
  file and which line decided this*, and a `serde`-shaped parser throws that
  away before the question can be asked.
- **One schema table.** [`schema::SCHEMA`] declares every recognised key once:
  its type, its accepted values, its default, its documentation, and its
  environment variable. Validation, `config show`, `config schema`, and the
  environment overrides are all derived from it, so they cannot disagree.
- **Layering.** User, repository, and directory configuration merged in a
  defined order, plus `extends` inheritance between files and `PAREDIT_*`
  environment overrides on top.
- **Provenance.** The merged result records, per key, the layer, the file, and
  the line that won. A setting nobody can explain is a setting nobody trusts.

### What this package does not own

- **No policy.** It answers "what is configured"; it never decides what a lint
  rule does with that answer. Rule names are validated against a list the
  caller supplies, because the registry is composition root and this is core.
- **No argument parsing.** `clap` never appears here. The CLI maps flags onto
  [`Settings`] itself, so a flag always wins over a file without this package
  needing to know that flags exist.

## Layer order

Lowest precedence first. Later layers replace earlier ones key by key; a key no
layer sets keeps its built-in default.

| # | Layer | Source |
| --- | --- | --- |
| 1 | `default` | Built into [`schema::SCHEMA`] |
| 2 | `user` | `$PAREDIT_CONFIG_HOME`, else `$XDG_CONFIG_HOME/paredit/`, else `~/.config/paredit/` |
| 3 | `repository` | `paredit.toml` beside the nearest ancestor `.git` |
| 4 | `directory` | Every `paredit.toml` from below the repository root down to the start directory, shallowest first |
| 5 | `environment` | `PAREDIT_<KEY>` for each schema key |

An explicit `--config <FILE>` replaces layers 2 through 4 outright: naming a
file means that file, not that file plus whatever discovery happened to find.

`extends` is resolved *within* a layer. The extending file wins over what it
extends, extends lists are applied left to right, and a cycle is an error
rather than a hang.
