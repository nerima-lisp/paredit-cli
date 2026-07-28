# paredit-core-workspace

Workspace discovery and filesystem identity for source inputs.

## Responsibilities

The only package that touches the filesystem. Everything else in the workspace
is handed source text and never goes looking for it:

- **Discovery.** Walking a root to find Lisp sources, honouring include and
  exclude rules, extension-to-dialect matching, and the bounds that stop a
  traversal running away.
- **Filesystem identity.** Recognising that two paths are the same file, so a
  multi-file operation cannot process one file twice or write over its own
  input.

### What this package does not own

- **No parsing.** It yields paths and bytes; `paredit-core-syntax` turns those
  into trees. It depends on syntax only to match extensions against `Dialect`.
- **No reports and no edits.** It never decides anything about the code it
  finds.
- **No CLI.** `--include`, `--exclude` and friends are parsed by the
  composition root and arrive here as `WorkspaceDiscoveryOptions`.
- **No caching or watching.** Discovery is a pure traversal per invocation.

This is the whole of the former `src/infrastructure` layer, which is now a
re-export façade with no code of its own.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | One reference: mapping a file extension to a `Dialect`. Deliberately the only coupling. |
| `cap-std` | Capability-scoped filesystem access, so a traversal cannot escape the root it was given. This is the package's central safety property, not a convenience. |
| `blake3` | Content hashing behind filesystem identity. |
| `libc` | Inode and device identity where the platform exposes it. |
| `anyhow` | Fallible I/O paths, pending §9.2. |
| `thiserror` | Typed discovery failures. |
| `proptest` (dev) | Properties over generated path sets. |

`cap-std` and `libc` appearing *only* here is the point. A second package
taking a filesystem dependency means the boundary has been breached.

## Public API

| Module | Principal items |
| --- | --- |
| `workspace` | `discover_workspace_files`, `WorkspaceDiscoveryOptions` |
| `fs_identity` | `FilesystemIdentity` |

Both are re-exported `pub` from the root crate as `infrastructure::workspace`
and `infrastructure::fs_identity`, matching their original declarations.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| changing which files a workspace scan finds, or the traversal bounds | discovery is here |
| fixing a symlink, hard link or case-insensitive-filesystem bug | that is what filesystem identity exists for |
| adding a new source extension | add it here **and** to `lispIncludes` in `flake.nix`; a contract test cross-checks the two against `Dialect::from_extension` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| reading a file in order to analyse it | take the path from here and hand the text to a feature |
| adding a filesystem dependency to another package | route it through this one instead; the single-owner property is the safety argument |
| deciding what to do with the files found | that is a feature package's job |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
