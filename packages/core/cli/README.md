# paredit-core-cli

CLI I/O conventions: input, diffing, atomic writes, and shared argument types.

## Responsibilities

The conventions every command obeys, stated once so no command can quietly
disagree with the others:

- **Reading input.** Stdin or a file, dialect detection from the extension or
  an explicit override, size limits, and parsing to a tree. `--file` behaves
  identically everywhere because it is resolved here.
- **Writing output.** Atomic writes with rollback, multi-file writes that
  either all land or none do, expected-content preconditions so a concurrent
  edit cannot be clobbered, and preservation of file metadata — including
  macOS ACLs and extended attributes.
- **Diffing.** The unified diff every preview and `--diff` path renders.
- **Shared argument types.** `DialectArg`, `SourceInput`, `EditTargetArgs` and
  the other `clap` value enums commands share, so `--dialect` means the same
  thing in all of them.
- **The gate helper.** The small shared piece behind CI-gating flags.

`io.rs` is the bulk of this package at 4,782 lines, and it is the reason the
package exists: the write-with-rollback path is the single most
safety-critical piece of non-domain code in the tree, and it must have exactly
one implementation.

### What this package does not own

- **No commands.** It has no idea what `rename` or `inline-function` are. Every
  subcommand is a feature package that reads its input and writes its output
  through here.
- **No use cases.** `cli::args` deliberately names no use case. The two
  conversions from CLI enums to use-case types (`ExtractFunctionInsert`,
  `FunctionParameterInsert`) were moved out to their feature modules precisely
  so this package could stop importing `application::usecase`.
- **No capability contract.** `presentation::cli::contract` enumerates three
  features' `supports_*_dialect` predicates, which makes it composition root,
  not core — the same reasoning that keeps the lint `REGISTRY` out of
  `core/lint-engine`.
- **No command tree.** The `clap` enum and the dispatch `match` are the
  composition root's; by definition they depend on every feature.
- **No parsing or analysis.** It hands text to `core/syntax` and gets a tree.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Detecting a dialect, parsing input to a tree, and expressing edits over spans. |
| `paredit-core-semantics`, `paredit-core-edit`, `paredit-core-lint-engine` | Shared argument and gating types are expressed in the vocabulary these define, so a flag and the thing it controls cannot drift apart. |
| `paredit-core-workspace` | `--include`/`--exclude` resolve to a workspace scan. |
| `clap` | The **only** package outside a feature's own `cli` module allowed to name `clap`. Shared value enums live here so `--dialect` parses identically everywhere. |
| `cap-std`, `libc`, `xattr` | Atomic writes that preserve permissions, extended attributes and macOS ACLs. |
| `blake3` | Content hashing for expected-write preconditions. |
| `serde_json` | JSON output shared across commands. |
| `anyhow`, `thiserror` | Fallible I/O; `thiserror` is the target shape per §9.2. |

## Public API

| Module | Principal items |
| --- | --- |
| `args` | `DialectArg`, `SourceInput`, `EditTargetArgs`, `MoveInsert`, `ParameterInsert`, `ThreadStyleArg` and the other shared `clap` value enums |
| `shared` | `read_input_and_dialect`, `read_input_dialect_and_tree`, `parse_document`, `detect_dialect`, `require_output_file`, `terminal_safe` |
| `shared::io` | `write_file_with_rollback`, `write_files_with_rollback`, the `_expected` and `_anchored` variants, `ExpectedWriteTarget`, `MAX_SOURCE_INPUT_BYTES` |
| `shared::diff` | `unified_diff` |
| `shared::macos_acl` | `read_acl`, `write_acl` (macOS only, via `#[cfg(target_os = "macos")]`) |
| `gate` | The shared CI-gate helper |

`io`, `diff` and `macos_acl` are declared inside `shared.rs` with `#[path]`, so
they are `shared`'s submodules while living as sibling files. Keep that shape:
flattening them would change every import in the tree for no gain.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing how a file is read, size-limited, or dialect-detected | input handling is here, and every command shares it |
| fixing a write that loses permissions, xattrs or an ACL | the rollback writer is here and is the only one |
| adding a precondition so concurrent edits cannot be clobbered | expected-write targets are here |
| adding a flag that more than one command needs | shared value enums are here |
| changing how a unified diff renders | there is one diff renderer |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a subcommand, or its flags | that is the feature's own `cli` module |
| converting a CLI enum into a use-case type | put it on the feature side; this package must not name a use case |
| enumerating what features support | that is composition root |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
