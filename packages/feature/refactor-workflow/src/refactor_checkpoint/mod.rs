//! `refactor create-checkpoint` / `list-checkpoints` / `restore-checkpoint` /
//! `delete-checkpoint` — a named point in time an agent can return to across
//! separate CLI invocations.
//!
//! `refactor step` and `refactor undo` already give a caller a way back from
//! *one* write, as long as it kept the `--undo-out` path around. Neither
//! helps once several turns have passed: the journal has no name, so an agent
//! that wants "put src/foo.lisp back to how it was before I started this
//! experiment" has to have remembered a file path from three tool calls ago.
//!
//! A checkpoint is that same [`paredit_core_safety::journal::UndoJournal`]
//! machinery, given a name and a home on disk
//! (`.paredit/checkpoints/<name>.json`, the same repo-relative convention
//! `.paredit/kill-ring.json` uses) so it survives between invocations. What it
//! is *not* is a general-purpose "revert N edits" tool: since `create` has
//! nothing to invert yet — no forward edit has happened at the moment a
//! checkpoint is taken — the only journal entry it can honestly build records
//! the identity edit, `before == after == the file as it stood`. That makes
//! `restore` a byte-exact anchor check rather than a time machine: it
//! refuses unless the file is still exactly what it was when the checkpoint
//! was taken, whether the drift since then came from this tool or from a
//! human's editor. That refusal, reusing
//! [`paredit_core_safety::journal::UndoJournalFile::restore`]'s own mismatch
//! detection, is the point — a checkpoint that silently overwrote an
//! intervening edit would be worse than no checkpoint at all.

pub mod cli;
pub mod domain;
pub mod store;
