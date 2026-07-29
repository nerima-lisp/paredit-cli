# paredit-core-safety

Resource limits, deadlines, undo journals, and external verification for
guarded edits.

## Responsibilities

The bounds and the reversibility of an operation, separated from the operation
itself. Four concerns that every writing command needs and none of them owns:

- **Limits.** How large an input may be, how many files a run may touch, and
  how many bytes it may read in total. These bounds already existed as
  hardcoded constants; this package makes them one type that a caller can
  lower.
- **Deadlines.** A wall-clock budget checked between units of work, so a
  pathological input fails with a report instead of hanging. Unarmed by
  default, which is what keeps output byte-identical across runs.
- **Undo journals.** The pre-image of a write, recorded as *reverse edits* in
  the coordinates of the file that was produced. A journal plus the current
  file is enough to restore the original, and both ends are hash-guarded.
- **External verification.** Running a caller-supplied command against the tree
  after a write, and parsing a Lisp implementation's own diagnostics into
  findings. This is the only package that spawns a process.

### What this package does not own

- **No filesystem access.** It is handed text and returns text.
  `paredit-core-workspace` and `paredit-core-cli` own reading and writing; a
  journal is applied by the transactional writer, not by this package.
- **No CLI.** `--max-total-bytes`, `--timeout-ms` and `--verify-command` are
  parsed by the composition root and arrive here as typed values.
- **No policy.** It reports that a limit was exceeded or a command failed; the
  caller decides whether that blocks a write.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | `ByteSpan`, so a reverse edit is expressed in the same coordinates as a forward one. |
| `serde_json` | The undo journal's on-disk form, and the structured shape of an external diagnostic. |
| `thiserror` | Typed limit, timeout, and journal failures. |
| `proptest` (dev) | Round-trip properties: forward edits then their inverse reproduce the input. |

## Public API

| Module | Principal items |
| --- | --- |
| `limits` | `ResourceLimits`, `parse_byte_size` |
| `deadline` | `Deadline`, `TimeoutError` |
| `journal` | `UndoJournal`, `UndoJournalFile`, `UndoEdit`, `invert_edits` |
| `external` | `VerificationCommand`, `VerificationOutcome`, `sbcl` |
| `hash` | `stable_text_hash` |

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a bound on how much work one invocation may do | limits and deadlines are here |
| making a write reversible, or teaching undo a new edit shape | the journal is the only description of a pre-image |
| running or parsing an external Lisp implementation | this is the one package allowed to spawn a process |

| You are… | and it does **not** belong here because… |
| --- | --- |
| reading or writing a file | hand the text in; `paredit-core-cli` owns the transactional writer |
| deciding whether a violated limit should fail the run | that is the caller's policy, and it differs per command |
| adding an analysis | a limit is not an analysis; put it in a feature package |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
