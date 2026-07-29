//! The pre-image of a write, recorded as reverse edits.
//!
//! A refactor manifest describes a change in the coordinates of the file it
//! read: span `[s, e)` of the *input* becomes `replacement`. That is enough to
//! reproduce the change and not enough to undo it — the bytes that used to sit
//! at `[s, e)` appear nowhere in the manifest, and they are unrecoverable from
//! the output, because a replacement erases exactly the text an inverse would
//! need.
//!
//! An undo journal closes that gap without changing the manifest. At the
//! moment of writing, both texts are in hand, so the inverse can be computed
//! precisely:
//!
//! ```text
//!   before:  (defun old-name (x) ...)
//!   forward: [7, 15) -> "new-name"          coordinates of `before`
//!   after:   (defun new-name (x) ...)
//!   reverse: [7, 15) -> "old-name"          coordinates of `after`
//! ```
//!
//! The two spans coincide in that example only because the replacement happens
//! to be the same length. In general each reverse span is the forward span
//! shifted by the net length change of every earlier edit, which is what
//! [`invert_edits`] computes.
//!
//! Both endpoints carry a content hash. Undo refuses a file that is not
//! byte-for-byte what the write produced, and refuses its own result if it
//! does not reproduce the recorded pre-image — so a journal cannot be replayed
//! twice, cannot be applied to a file someone edited in between, and cannot
//! half-apply.

use std::path::{Path, PathBuf};

use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::hash::stable_text_hash;

/// The journal format version written into every journal.
pub const JOURNAL_SCHEMA_VERSION: u64 = 1;

/// A journal that cannot be built, parsed, or applied.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JournalError {
    #[error("edit span {start}..{end} lies outside the {length}-byte document")]
    SpanOutOfBounds {
        start: usize,
        end: usize,
        length: usize,
    },
    #[error("edit span {start}..{end} is not on a character boundary")]
    SpanNotOnCharBoundary { start: usize, end: usize },
    #[error("edit spans {previous_end} and {start} overlap; an inverse would be ambiguous")]
    OverlappingSpans { previous_end: usize, start: usize },
    #[error("undo journal for {path} expects content hashing to {expected}, found {actual}")]
    ContentMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("undo of {path} reproduced {actual} rather than the recorded pre-image {expected}")]
    RestoreMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("undo journal is not valid JSON: {0}")]
    Malformed(String),
    #[error("undo journal has schema version {found}, expected {expected}")]
    UnsupportedSchema { found: u64, expected: u64 },
}

/// One reverse edit, in the coordinates of the file that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoEdit {
    /// The region of the *written* file to replace.
    pub span: ByteSpan,
    /// The bytes that region held before the write.
    pub replacement: String,
}

/// One file's entry in a journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoJournalFile {
    pub path: PathBuf,
    /// Hash of the content before the write; undo must reproduce it.
    pub before_hash: String,
    /// Hash of the content the write produced; undo requires it on disk.
    pub after_hash: String,
    pub edits: Vec<UndoEdit>,
}

impl UndoJournalFile {
    /// Records the inverse of one file's forward edits.
    ///
    /// `before` is the text that was read and `forward` the edits that were
    /// applied to it, in that text's coordinates — exactly the pair the apply
    /// path already holds.
    pub fn record(
        path: PathBuf,
        before: &str,
        forward: &[(ByteSpan, String)],
    ) -> Result<Self, JournalError> {
        let (edits, after) = invert_edits(before, forward)?;
        Ok(Self {
            path,
            before_hash: stable_text_hash(before),
            after_hash: stable_text_hash(&after),
            edits,
        })
    }

    /// Rebuilds the pre-image from the file's current content.
    ///
    /// Fails unless the current content is what the write produced, and fails
    /// again unless the result is what the write replaced. Both checks are
    /// necessary: the first stops an undo from clobbering an intervening edit,
    /// the second stops a corrupt journal from writing a document nobody
    /// authored.
    pub fn restore(&self, current: &str) -> Result<String, JournalError> {
        let actual = stable_text_hash(current);
        if actual != self.after_hash {
            return Err(JournalError::ContentMismatch {
                path: self.path.display().to_string(),
                expected: self.after_hash.clone(),
                actual,
            });
        }

        let restored = apply_reverse_edits(current, &self.edits)?;
        let restored_hash = stable_text_hash(&restored);
        if restored_hash != self.before_hash {
            return Err(JournalError::RestoreMismatch {
                path: self.path.display().to_string(),
                expected: self.before_hash.clone(),
                actual: restored_hash,
            });
        }
        Ok(restored)
    }

    /// Whether this entry would change the file at all.
    #[must_use]
    pub fn changes_content(&self) -> bool {
        self.before_hash != self.after_hash
    }

    fn to_json(&self) -> Value {
        json!({
            "path": self.path.display().to_string(),
            "before_hash": self.before_hash,
            "after_hash": self.after_hash,
            "edits": self
                .edits
                .iter()
                .map(|edit| json!({
                    "start": edit.span.start().get(),
                    "end": edit.span.end().get(),
                    "replacement": edit.replacement,
                }))
                .collect::<Vec<_>>(),
        })
    }
}

/// Everything one `--write` touched, and how to put it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoJournal {
    /// The command that produced the write, for the operator reading the file.
    pub operation: String,
    /// The tool version that wrote it. A journal is only meaningful against
    /// the edit semantics that produced it.
    pub tool_version: String,
    pub files: Vec<UndoJournalFile>,
}

impl UndoJournal {
    #[must_use]
    pub fn new(operation: impl Into<String>, files: Vec<UndoJournalFile>) -> Self {
        Self {
            operation: operation.into(),
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            files,
        }
    }

    /// The journal's on-disk form.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": JOURNAL_SCHEMA_VERSION,
            "operation": self.operation,
            "tool_version": self.tool_version,
            "file_count": self.files.len(),
            "files": self.files.iter().map(UndoJournalFile::to_json).collect::<Vec<_>>(),
        })
    }

    /// Parses a journal, rejecting a schema this build does not understand.
    pub fn from_json(value: &Value) -> Result<Self, JournalError> {
        let object = value
            .as_object()
            .ok_or_else(|| JournalError::Malformed("journal must be a JSON object".to_owned()))?;

        let schema_version = required_u64(object, "schema_version")?;
        if schema_version != JOURNAL_SCHEMA_VERSION {
            return Err(JournalError::UnsupportedSchema {
                found: schema_version,
                expected: JOURNAL_SCHEMA_VERSION,
            });
        }

        let files = object
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| JournalError::Malformed("journal field files must be an array".into()))?
            .iter()
            .map(parse_file)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            operation: required_string(object, "operation")?,
            tool_version: required_string(object, "tool_version")?,
            files,
        })
    }

    /// The entries that would actually rewrite something.
    #[must_use]
    pub fn changed_files(&self) -> Vec<&UndoJournalFile> {
        self.files
            .iter()
            .filter(|file| file.changes_content())
            .collect()
    }

    /// Finds the entry for a path, comparing as written.
    #[must_use]
    pub fn entry_for(&self, path: &Path) -> Option<&UndoJournalFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// Turns forward edits over `before` into reverse edits over the text they
/// produce, returning both.
///
/// The forward edits need not arrive sorted; they are ordered here and then
/// checked for overlap, because two edits that touch the same bytes have no
/// well-defined inverse — undoing either would have to guess what the other
/// replaced.
pub fn invert_edits(
    before: &str,
    forward: &[(ByteSpan, String)],
) -> Result<(Vec<UndoEdit>, String), JournalError> {
    let mut ordered = forward.to_vec();
    ordered.sort_by_key(|(span, _)| (span.start().get(), span.end().get()));

    let mut previous_end: Option<usize> = None;
    for (span, _) in &ordered {
        let (start, end) = (span.start().get(), span.end().get());
        if end > before.len() || start > end {
            return Err(JournalError::SpanOutOfBounds {
                start,
                end,
                length: before.len(),
            });
        }
        if !before.is_char_boundary(start) || !before.is_char_boundary(end) {
            return Err(JournalError::SpanNotOnCharBoundary { start, end });
        }
        if previous_end.is_some_and(|previous_end| start < previous_end) {
            return Err(JournalError::OverlappingSpans {
                previous_end: previous_end.unwrap_or_default(),
                start,
            });
        }
        previous_end = Some(end);
    }

    // One left-to-right pass builds the output and the inverse together. The
    // output's own length *is* the coordinate mapping: after copying the text
    // that precedes an edit, `after.len()` is where that edit lands in the
    // written file. Deriving the offset that way rather than accumulating a
    // signed shift keeps the arithmetic in `usize` and makes the invariant
    // "reverse spans index the output" true by construction.
    let mut after = String::with_capacity(before.len());
    let mut reverse = Vec::with_capacity(ordered.len());
    let mut copied = 0_usize;

    for (span, replacement) in &ordered {
        let (start, end) = (span.start().get(), span.end().get());
        after.push_str(&before[copied..start]);

        let after_start = after.len();
        after.push_str(replacement);
        let after_end = after.len();

        reverse.push(UndoEdit {
            span: ByteSpan::new(ByteOffset::new(after_start), ByteOffset::new(after_end)),
            replacement: before[start..end].to_owned(),
        });

        copied = end;
    }
    after.push_str(&before[copied..]);

    Ok((reverse, after))
}

/// Applies reverse edits to the text they were computed against.
fn apply_reverse_edits(current: &str, edits: &[UndoEdit]) -> Result<String, JournalError> {
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| (edit.span.start().get(), edit.span.end().get()));

    let mut restored = String::with_capacity(current.len());
    let mut copied = 0_usize;
    for edit in &ordered {
        let (start, end) = (edit.span.start().get(), edit.span.end().get());
        if end > current.len() || start > end {
            return Err(JournalError::SpanOutOfBounds {
                start,
                end,
                length: current.len(),
            });
        }
        if !current.is_char_boundary(start) || !current.is_char_boundary(end) {
            return Err(JournalError::SpanNotOnCharBoundary { start, end });
        }
        if start < copied {
            return Err(JournalError::OverlappingSpans {
                previous_end: copied,
                start,
            });
        }
        restored.push_str(&current[copied..start]);
        restored.push_str(&edit.replacement);
        copied = end;
    }
    restored.push_str(&current[copied..]);
    Ok(restored)
}

fn parse_file(value: &Value) -> Result<UndoJournalFile, JournalError> {
    let object = value.as_object().ok_or_else(|| {
        JournalError::Malformed("journal file entry must be an object".to_owned())
    })?;

    let edits = object
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| JournalError::Malformed("journal file entry needs edits".to_owned()))?
        .iter()
        .map(parse_edit)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(UndoJournalFile {
        path: PathBuf::from(required_string(object, "path")?),
        before_hash: required_string(object, "before_hash")?,
        after_hash: required_string(object, "after_hash")?,
        edits,
    })
}

fn parse_edit(value: &Value) -> Result<UndoEdit, JournalError> {
    let object = value
        .as_object()
        .ok_or_else(|| JournalError::Malformed("journal edit must be an object".to_owned()))?;

    let start = usize::try_from(required_u64(object, "start")?)
        .map_err(|_| JournalError::Malformed("journal edit start is too large".to_owned()))?;
    let end = usize::try_from(required_u64(object, "end")?)
        .map_err(|_| JournalError::Malformed("journal edit end is too large".to_owned()))?;
    let span = ByteSpan::try_new(ByteOffset::new(start), ByteOffset::new(end)).ok_or(
        JournalError::SpanOutOfBounds {
            start,
            end,
            length: end,
        },
    )?;

    Ok(UndoEdit {
        span,
        replacement: required_string(object, "replacement")?,
    })
}

fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, JournalError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| JournalError::Malformed(format!("journal field {field} must be a string")))
}

fn required_u64(object: &Map<String, Value>, field: &str) -> Result<u64, JournalError> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| JournalError::Malformed(format!("journal field {field} must be a number")))
}

#[cfg(test)]
mod tests {
    use super::{JournalError, UndoJournal, UndoJournalFile, invert_edits};
    use crate::hash::stable_text_hash;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan};
    use std::path::PathBuf;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    fn edit(start: usize, end: usize, replacement: &str) -> (ByteSpan, String) {
        (span(start, end), replacement.to_owned())
    }

    #[test]
    fn a_same_length_replacement_keeps_its_coordinates() {
        let before = "(defun old-name (x) x)\n";
        let (reverse, after) =
            invert_edits(before, &[edit(7, 15, "new-name")]).expect("invert one edit");

        assert_eq!(after, "(defun new-name (x) x)\n");
        assert_eq!(reverse.len(), 1);
        assert_eq!(reverse[0].span, span(7, 15));
        assert_eq!(reverse[0].replacement, "old-name");
    }

    /// The interesting case: a replacement that changes length shifts every
    /// later reverse span. Getting this wrong produces a journal that applies
    /// cleanly and restores garbage, which is why the hash check exists too.
    #[test]
    fn later_reverse_spans_shift_by_the_net_length_change() {
        let before = "(a bb ccc)";
        let (reverse, after) = invert_edits(
            before,
            &[edit(1, 2, "xxxx"), edit(3, 5, "y"), edit(6, 9, "zz")],
        )
        .expect("invert three edits");

        assert_eq!(after, "(xxxx y zz)");
        assert_eq!(reverse[0].span, span(1, 5));
        assert_eq!(reverse[0].replacement, "a");
        assert_eq!(reverse[1].span, span(6, 7));
        assert_eq!(reverse[1].replacement, "bb");
        assert_eq!(reverse[2].span, span(8, 10));
        assert_eq!(reverse[2].replacement, "ccc");
    }

    #[test]
    fn unsorted_forward_edits_are_ordered_before_inversion() {
        let before = "(a bb ccc)";
        let (sorted, sorted_after) = invert_edits(
            before,
            &[edit(1, 2, "xxxx"), edit(3, 5, "y"), edit(6, 9, "zz")],
        )
        .expect("invert sorted");
        let (shuffled, shuffled_after) = invert_edits(
            before,
            &[edit(6, 9, "zz"), edit(1, 2, "xxxx"), edit(3, 5, "y")],
        )
        .expect("invert shuffled");

        assert_eq!(sorted, shuffled);
        assert_eq!(sorted_after, shuffled_after);
    }

    #[test]
    fn an_insertion_inverts_to_a_deletion() {
        let before = "(f x)";
        let (reverse, after) = invert_edits(before, &[edit(3, 3, "y ")]).expect("invert insertion");

        assert_eq!(after, "(f y x)");
        assert_eq!(reverse[0].span, span(3, 5));
        assert_eq!(reverse[0].replacement, "");
    }

    #[test]
    fn a_deletion_inverts_to_an_insertion() {
        let before = "(f y x)";
        let (reverse, after) = invert_edits(before, &[edit(3, 5, "")]).expect("invert deletion");

        assert_eq!(after, "(f x)");
        assert_eq!(reverse[0].span, span(3, 3));
        assert_eq!(reverse[0].replacement, "y ");
    }

    #[test]
    fn overlapping_edits_have_no_inverse() {
        assert_eq!(
            invert_edits("(abcdef)", &[edit(1, 4, "x"), edit(3, 6, "y")]),
            Err(JournalError::OverlappingSpans {
                previous_end: 4,
                start: 3
            })
        );
    }

    #[test]
    fn a_span_past_the_end_is_rejected() {
        assert_eq!(
            invert_edits("(ab)", &[edit(1, 99, "x")]),
            Err(JournalError::SpanOutOfBounds {
                start: 1,
                end: 99,
                length: 4
            })
        );
    }

    #[test]
    fn a_span_splitting_a_character_is_rejected() {
        // "。" is three bytes, so offset 2 is inside it.
        assert_eq!(
            invert_edits("(a 。)", &[edit(2, 4, "x")]),
            Err(JournalError::SpanNotOnCharBoundary { start: 2, end: 4 })
        );
    }

    #[test]
    fn restore_reproduces_the_pre_image() {
        let before = "(defun old (x) (old x))\n";
        let forward = vec![edit(7, 10, "renamed"), edit(16, 19, "renamed")];
        let (_, after) = invert_edits(before, &forward).expect("compute after");
        let entry = UndoJournalFile::record(PathBuf::from("src/core.lisp"), before, &forward)
            .expect("record journal entry");

        assert_eq!(after, "(defun renamed (x) (renamed x))\n");
        assert!(entry.changes_content());
        assert_eq!(entry.restore(&after).expect("restore"), before);
    }

    #[test]
    fn restore_refuses_a_file_that_changed_since_the_write() {
        let before = "(a)\n";
        let forward = vec![edit(1, 2, "b")];
        let entry = UndoJournalFile::record(PathBuf::from("src/core.lisp"), before, &forward)
            .expect("record journal entry");

        let error = entry
            .restore("(c)\n")
            .expect_err("an intervening edit must block undo");
        assert_eq!(
            error,
            JournalError::ContentMismatch {
                path: "src/core.lisp".to_owned(),
                expected: stable_text_hash("(b)\n"),
                actual: stable_text_hash("(c)\n"),
            }
        );
    }

    /// Replaying an undo must fail on the second run: after the first, the file
    /// holds the pre-image, whose hash is not the recorded post-image.
    #[test]
    fn a_journal_cannot_be_applied_twice() {
        let before = "(a)\n";
        let forward = vec![edit(1, 2, "b")];
        let entry = UndoJournalFile::record(PathBuf::from("src/core.lisp"), before, &forward)
            .expect("record journal entry");
        let restored = entry.restore("(b)\n").expect("first undo");

        assert_eq!(restored, before);
        assert!(entry.restore(&restored).is_err());
    }

    #[test]
    fn a_no_op_write_records_an_entry_that_changes_nothing() {
        let entry = UndoJournalFile::record(PathBuf::from("src/core.lisp"), "(a)\n", &[])
            .expect("record empty journal entry");

        assert!(!entry.changes_content());
        assert_eq!(entry.restore("(a)\n").expect("restore"), "(a)\n");
    }

    #[test]
    fn a_journal_round_trips_through_json() {
        let before = "(defun old (x) x)\n";
        let journal = UndoJournal::new(
            "refactor apply",
            vec![
                UndoJournalFile::record(
                    PathBuf::from("src/core.lisp"),
                    before,
                    &[edit(7, 10, "n")],
                )
                .expect("record entry"),
            ],
        );

        let parsed = UndoJournal::from_json(&journal.to_json()).expect("parse journal");
        assert_eq!(parsed, journal);
        assert_eq!(parsed.changed_files().len(), 1);
        assert!(
            parsed
                .entry_for(std::path::Path::new("src/core.lisp"))
                .is_some()
        );
    }

    #[test]
    fn a_journal_from_a_future_schema_is_refused() {
        let mut value = UndoJournal::new("refactor apply", Vec::new()).to_json();
        value["schema_version"] = serde_json::json!(99);

        assert_eq!(
            UndoJournal::from_json(&value),
            Err(JournalError::UnsupportedSchema {
                found: 99,
                expected: super::JOURNAL_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn a_journal_missing_a_field_names_the_field() {
        let mut value = UndoJournal::new("refactor apply", Vec::new()).to_json();
        value.as_object_mut().expect("object").remove("operation");

        assert_eq!(
            UndoJournal::from_json(&value),
            Err(JournalError::Malformed(
                "journal field operation must be a string".to_owned()
            ))
        );
    }
}
