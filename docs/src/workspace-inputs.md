# Choosing which files to analyse

Every command that takes roots — `inspect workspace`, `inspect sources`,
`inspect similarity`, `refactor workspace-plan`, `refactor workspace-preview`,
`refactor workspace-execute` — shares one flag set for deciding which files it
looks at. This page is the reference for that set.

The flags divide cleanly in two, and the division is worth keeping in mind
because it explains how they combine:

- a **selector** decides where the candidate list comes from;
- a **filter** narrows whatever the selector produced.

Exactly one selector may be given. Filters always apply, whichever selector
ran, so `--since origin/main --include '**/*.lisp'` means what it looks like.

## Seeing the answer

```sh
paredit inspect sources --output json --list-files .
```

`inspect sources` runs selection and stops. It parses nothing, so it is the
cheapest command in the tool, and it reports which rule dropped every file that
did not make it:

```
selector        walk
files           128
skipped_unknown 2951
skipped_ignored 12
skipped_glob    0
ignore_file     /repo/.gitignore
repository      /repo    128
```

Reach for it when a CI run reports nothing and you need to know whether that is
good news or an over-wide pattern.

## Selectors

### Walking the roots (default)

With no selector, each root is walked. A file root is taken as-is.

### `--since <git-ref>`

Analyses only the files that differ from `<git-ref>`, which is usually what a
pull-request check wants and is the single biggest lever on CI time.

```sh
paredit inspect lint --since origin/main .
```

Deleted files are dropped — there is nothing left to parse — and a rename
reports only its destination. Files git does not yet track are *included* by
default, because a pull request that adds a file has not committed it in the
working tree you are running in; `--since-skip-untracked` turns that off.

An unresolvable ref is an error rather than an empty change set. An empty
change set reads as "nothing to check" and would let a gate pass without
examining anything.

### `--from-git`

Takes the file set from `git ls-files`. Faster than a walk on a large tree, and
already filtered by every ignore rule git knows about.

### `--from-manifest`

Takes the file set from the project's own build definition rather than from the
directory layout. Four formats are read:

| File | What is read |
| --- | --- |
| `*.asd` | `defsystem` `:components`, through `:module` and `:pathname`, in declaration order |
| `deps.edn` | `:paths`, plus `:extra-paths` under `:aliases` |
| `shadow-cljs.edn` | `:source-paths` |
| `project.clj` | `:source-paths`, `:test-paths`, falling back to Leiningen's `src`/`test` defaults |
| `*.el` | the `Package-Requires` header, when no named manifest is present |

This is not the same set a walk produces, and that is the point: an ASDF
`:components` list contains exactly the files the system compiles, in the order
it compiles them, and excludes the abandoned experiment sitting next to them.
A component the manifest names but that is not on disk is reported under
`missing` rather than silently dropped.

### `--paths-from <FILE|->`

Reads the file list from a file, or from standard input with `-`. Every
selection rule this tool implements is an approximation of a decision you may
already have made with tools of your own; this is how to hand the answer over.

```sh
git ls-files -z '*.lisp' \
  | paredit inspect lint --paths-from - --paths-from-separator nul .
```

Use `--paths-from-separator nul` for machine-produced lists: a path may contain
a space, and on unix it may contain a newline.

A listed path outside the roots is dropped rather than read. The roots are what
the run was authorised for, and a path list does not widen that.

### `--from-archive <ARCHIVE|-> --extract-to <DIR>`

Analyses an **uncompressed tar** archive, from a file or from standard input.
Compression and transport are deliberately left to the shell:

```sh
curl -sSL https://example.invalid/project.tar.gz | gzip -dc \
  | paredit inspect sources --from-archive - --extract-to /tmp/project /tmp/project
```

That keeps an HTTP client, a TLS stack and a decompressor out of a tool whose
job is to be trusted with your source tree, while still supporting `.tar.gz`,
`.tar.zst`, `git archive` and anything else `curl` can fetch.

Extraction refuses, rather than sanitises, everything tar archives have
historically been abused for: absolute paths, `..` components, symlinks,
hardlinks, devices, and overwriting a file that already exists. Entry count and
total size are bounded.

## Filters

| Flag | Effect |
| --- | --- |
| `--include <GLOB>` | Keep only matching paths. Repeatable. |
| `--exclude-glob <GLOB>` | Skip matching paths, overriding `--include`. Repeatable. |
| `--exclude <PATH>` | Skip an exact path and its subtree. Repeatable. |
| `--no-gitignore` | Do not read `.gitignore` or `.git/info/exclude`. |
| `--no-pareditignore` | Do not read `.pareditignore`. |
| `--no-ignore` | Neither of the above. |
| `--follow-symlinks` | Traverse symlinks that resolve inside the roots. |
| `--include-hidden` | Keep dot-prefixed files and directories. |
| `--include-generated` | Keep `target`, `node_modules`, `dist` and friends. |
| `--include-unknown` | Keep files whose extension names no known dialect. |
| `--max-depth <N>` | Bound recursion from each root. |

### Ignore files

`.gitignore` and `.pareditignore` are honoured by default. A tool that walks a
source tree is far more often wrong for having read a build artifact than for
having skipped a file the project told git to forget about.

`.pareditignore` exists for the case `.gitignore` cannot express: generated Lisp
that is deliberately committed, so git must see it and analysis should not. It
uses the same syntax and outranks `.gitignore` at the same directory level.

Precedence follows git exactly:

- the deeper file wins, so a `!keep.lisp` in a subdirectory overrides a
  `*.lisp` at the root;
- within one file, the last matching pattern wins;
- a repository boundary cuts the stack, so an outer checkout's `.gitignore`
  does not govern a nested one;
- a root named explicitly on the command line is never ignore-filtered — being
  told to scan a directory is a stronger signal than a pattern covering it.

Most commands take explicit paths rather than roots and so carry no
`--no-ignore` flag, yet a directory argument still expands through a walk that
honours ignore files. Three environment variables are the escape hatch there,
and are also the right shape for a per-run adjustment in CI:

| Variable | Effect |
| --- | --- |
| `PAREDIT_NO_IGNORE` | Read neither ignore file |
| `PAREDIT_NO_GITIGNORE` | Do not read `.gitignore` |
| `PAREDIT_NO_PAREDITIGNORE` | Do not read `.pareditignore` |

A variable set to `0`, `false`, `no`, `off` or the empty string counts as
unset, so a CI system that exports every name it knows about cannot switch this
on by accident. A flag always wins over the environment in the narrowing
direction: `--no-ignore` disables ignore files whatever the environment says.

### Glob syntax

The same syntax everywhere: `.gitignore`, `.pareditignore`, `--include` and
`--exclude-glob`. It is `gitignore(5)`:

| Pattern | Meaning |
| --- | --- |
| `target` | a component named `target`, at any depth |
| `/target` | only at the root |
| `src/*.lisp` | anchored; `*` does not cross a `/` |
| `src/**/*.lisp` | `**` spans zero or more directories |
| `build/` | directories only |
| `!keep.lisp` | re-include |
| `file[0-9].lisp` | character class, negated with `[!...]` |
| `a\*b` | `\` escapes the next character |

Command-line patterns resolve against the root they were given, so
`--exclude-glob 'src/*.lisp'` means the same thing as that line in a
`.gitignore` at the root.

### Symlinks

Skipped by default: a symlink is the one filesystem object that can make a
bounded traversal unbounded.

`--follow-symlinks` traverses a link whose target resolves **inside** the
scanned roots. A link pointing outside is counted under
`skipped.symlink_escaped` rather than followed — every read in this tool goes
through a directory capability opened from a canonical root, and a followed
link out of the roots would be read through a capability that was never opened
for it. The answer is to add that directory as a root. Loops are detected by
canonical path and counted under `skipped.symlink_cycle`.

## Several repositories at once

Discovery detects repository boundaries and groups the result:

```json
"repositories": [
  { "path": "/work/first", "file_count": 40 },
  { "path": "/work/second", "file_count": 12 }
],
"files_outside_repositories": 3
```

Each checkout keeps its own ignore rules, and `--since` and `--from-git` query
each repository separately, so a monorepo of independent checkouts behaves the
way each checkout expects.

## Caching the scan

`inspect sources --cache-dir <DIR>` reuses a previous scan of the same roots.

This is not a general speed-up: parsing dominates a lint run, not walking. It is
for the case where the walk genuinely dominates — a very large tree scanned
repeatedly by an editor or an agent. The cache is opt-in for that reason, and
only `inspect sources` accepts it: a cached entry is a *file list*, not the
capability-scoped handles the reading commands need. Piping composes without
weakening anything:

```sh
paredit inspect sources --cache-dir ~/.cache/paredit --list-files --output text . \
  | sed -n 's/^file\t//p' \
  | paredit inspect lint --paths-from - .
```

A stale hit would be far worse than a miss, so validation is deliberately
strict. Every directory the walk listed is re-stat'ed and compared on **both**
mtime and entry count — a one-second timestamp granularity can hide a
same-second rename that the count catches — and every ignore file that
contributed patterns is stamped separately, because editing a `.gitignore`
changes the result without changing any directory's mtime. Anything unexpected
is a miss, never an error. The report says which happened:

```
cache   hit      # reused
cache   missing  # no entry for these options
cache   stale    # the tree changed
cache   unusable # written by another version, or unreadable
```

Keep the cache directory outside the scanned roots. Creating it inside changes
the tree it describes, which invalidates its own first entry.
