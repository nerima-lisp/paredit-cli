//! `refactor step` — approving a preview manifest one edit at a time.
//!
//! `refactor apply` is all-or-nothing, which is the right default: a rename
//! half-applied is a broken program. It is the wrong shape for *review*,
//! though. A reader who disagrees with one of forty edits has to discard the
//! manifest and derive a narrower one, and in practice discards the review.
//!
//! This makes the same manifest steppable — numbered edits, each with the line
//! it sits on and the text it replaces — so a caller can take a subset. The
//! safety story is unchanged: the manifest hash guard and the per-file input
//! hash guard both still apply, and a subset that would not parse is refused
//! before anything is written.

pub mod cli;
pub mod domain;
