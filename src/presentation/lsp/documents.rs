//! The open documents, and the translation between LSP positions and byte
//! offsets.
//!
//! Everything else in this tool addresses source by byte offset. LSP addresses
//! it by `(line, character)`, where *character* is counted in UTF-16 code units
//! unless the client and server agree otherwise. That disagreement is the
//! single most common source of off-by-one bugs in a language server, and it
//! only shows up on non-ASCII input — so it is handled here, once, with the
//! negotiated encoding carried explicitly rather than assumed.

use std::collections::BTreeMap;
use std::path::PathBuf;

use paredit_core_syntax::dialect::Dialect;

/// How a client counts the `character` field of a position.
///
/// The default is UTF-16 because the protocol's is: a client that says nothing
/// is promising UTF-16, and a server that assumed bytes would be wrong about
/// every position on every line holding a non-ASCII character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PositionEncoding {
    /// Bytes. Cheapest and exact, but a client must opt in (LSP 3.17).
    Utf8,
    /// UTF-16 code units. The protocol's default, and what every client
    /// supports.
    #[default]
    Utf16,
    /// Unicode scalar values.
    Utf32,
}

impl PositionEncoding {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16 => "utf-16",
            Self::Utf32 => "utf-32",
        }
    }

    pub(crate) fn parse(label: &str) -> Option<Self> {
        match label {
            "utf-8" => Some(Self::Utf8),
            "utf-16" => Some(Self::Utf16),
            "utf-32" => Some(Self::Utf32),
            _ => None,
        }
    }

    /// How many units `character` advances for one character of source.
    const fn units(self, character: char) -> usize {
        match self {
            Self::Utf8 => character.len_utf8(),
            Self::Utf16 => character.len_utf16(),
            Self::Utf32 => 1,
        }
    }
}

/// One open document.
#[derive(Debug, Clone)]
pub(crate) struct Document {
    pub text: String,
    pub dialect: Dialect,
    pub version: i64,
    /// The byte offset each line starts at. Recomputed on every change rather
    /// than patched, because a full-sync server replaces the whole text anyway
    /// and an incrementally-maintained index that drifts is worse than no index.
    line_starts: Vec<usize>,
}

impl Document {
    pub(crate) fn new(text: String, dialect: Dialect, version: i64) -> Self {
        let line_starts = line_starts(&text);
        Self {
            text,
            dialect,
            version,
            line_starts,
        }
    }

    pub(crate) fn replace(&mut self, text: String, version: i64) {
        self.line_starts = line_starts(&text);
        self.text = text;
        self.version = version;
    }

    /// The byte offset of an LSP position.
    ///
    /// Clamps rather than failing. A client can legitimately send a position
    /// one past the end of a line (an end-exclusive range often does), and
    /// several send a stale position after a change they have not yet
    /// acknowledged. Neither is worth refusing a request over.
    pub(crate) fn offset_of(
        &self,
        line: usize,
        character: usize,
        encoding: PositionEncoding,
    ) -> usize {
        let Some(&start) = self.line_starts.get(line) else {
            return self.text.len();
        };
        let end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        let slice = self.text.get(start..end).unwrap_or_default();

        let mut units = 0;
        for (byte_offset, character_value) in slice.char_indices() {
            if units >= character {
                return start + byte_offset;
            }
            units += encoding.units(character_value);
        }
        end
    }

    /// The LSP position of a byte offset.
    pub(crate) fn position_of(&self, offset: usize, encoding: PositionEncoding) -> (usize, usize) {
        let mut offset = offset.min(self.text.len());
        // A byte within a UTF-8 sequence has no LSP position; use the start
        // of that scalar rather than losing the whole prefix of the line.
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        // `partition_point` gives the first line starting after the offset, so
        // one less is the line the offset is on.
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let start = self.line_starts.get(line).copied().unwrap_or(0);
        let character = self
            .text
            .get(start..offset)
            .unwrap_or_default()
            .chars()
            .map(|character| encoding.units(character))
            .sum();
        (line, character)
    }
}

/// The byte offset each line begins at, including the first.
///
/// Splitting on `\n` alone is deliberate: a `\r\n` file has its `\r` at the end
/// of the preceding line, which is exactly where a byte-offset scheme should
/// put it, and treating a lone `\r` as a break would disagree with every
/// client's idea of what line a position is on.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(index, _)| index + 1),
    );
    starts
}

/// The open documents, keyed by URI.
#[derive(Debug, Default)]
pub(crate) struct Documents {
    open: BTreeMap<String, Document>,
}

impl Documents {
    pub(crate) fn open(&mut self, uri: String, text: String, version: i64) {
        let dialect = dialect_for(&uri, &text);
        self.open.insert(uri, Document::new(text, dialect, version));
    }

    pub(crate) fn change(&mut self, uri: &str, text: String, version: i64) {
        if let Some(document) = self.open.get_mut(uri) {
            if version > document.version {
                document.replace(text, version);
            }
        }
    }

    pub(crate) fn close(&mut self, uri: &str) {
        self.open.remove(uri);
    }

    pub(crate) fn get(&self, uri: &str) -> Option<&Document> {
        self.open.get(uri)
    }

    #[cfg(test)]
    pub(crate) fn uris(&self) -> Vec<String> {
        self.open.keys().cloned().collect()
    }
}

fn dialect_for(uri: &str, text: &str) -> Dialect {
    let path = path_from_uri(uri);
    Dialect::detect_in_source(path.as_deref(), None, text)
}

/// The filesystem path a `file:` URI names, or `None` for any other scheme.
///
/// A language server sees URIs it cannot open — `untitled:`, a virtual document
/// from another extension — and the dialect of those is decided from content
/// alone. Returning `None` rather than a bogus path keeps that distinction.
pub(crate) fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///path` has an empty authority; `file://host/path` has one, and
    // this tool cannot read a remote host's file anyway.
    let path = rest.strip_prefix('/').map(|path| format!("/{path}"))?;
    let decoded = percent_decode(&path);
    // A Windows URI is `file:///C:/…`, whose path component starts with a
    // slash that is not part of the path.
    let trimmed = decoded
        .strip_prefix('/')
        .filter(|rest| {
            let mut characters = rest.chars();
            characters
                .next()
                .is_some_and(|drive| drive.is_ascii_alphabetic())
                && characters.next() == Some(':')
        })
        .map_or(decoded.clone(), ToOwned::to_owned);
    Some(PathBuf::from(trimmed))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        Document::new(text.to_owned(), Dialect::CommonLisp, 1)
    }

    #[test]
    fn a_position_on_the_first_line_is_its_byte_offset_for_ascii() {
        let document = document("(defun f ())\n(defun g ())\n");
        assert_eq!(document.offset_of(1, 7, PositionEncoding::Utf16), 20);
        assert_eq!(document.position_of(20, PositionEncoding::Utf16), (1, 7));
    }

    /// The encoding is the whole point of this module. An emoji is two UTF-16
    /// code units, one scalar value, and four bytes; a server that assumes any
    /// one of those puts every later position on that line in the wrong place.
    #[test]
    fn the_three_encodings_disagree_about_the_same_character() {
        let document = document("(f \"🎈\" x)\n");
        let after_the_balloon = document.text.find('"').expect("a quote") + 1 + 4;
        assert_eq!(
            document.position_of(after_the_balloon, PositionEncoding::Utf8),
            (0, 8)
        );
        assert_eq!(
            document.position_of(after_the_balloon, PositionEncoding::Utf16),
            (0, 6)
        );
        assert_eq!(
            document.position_of(after_the_balloon, PositionEncoding::Utf32),
            (0, 5)
        );
    }

    #[test]
    fn offset_and_position_round_trip_through_a_multibyte_line() {
        let document = document("(f \"héllo→\" x)\n(g)\n");
        for encoding in [
            PositionEncoding::Utf8,
            PositionEncoding::Utf16,
            PositionEncoding::Utf32,
        ] {
            for offset in 0..document.text.len() {
                if !document.text.is_char_boundary(offset) {
                    continue;
                }
                let (line, character) = document.position_of(offset, encoding);
                assert_eq!(
                    document.offset_of(line, character, encoding),
                    offset,
                    "{encoding:?} at byte {offset}"
                );
            }
        }
    }

    #[test]
    fn positions_inside_utf8_scalars_clamp_to_the_preceding_boundary() {
        let document = document("\u{3042}\u{1f388}x\n");

        for offset in [1, 2] {
            assert_eq!(
                document.position_of(offset, PositionEncoding::Utf8),
                (0, 0),
                "Japanese character byte {offset}"
            );
        }
        for offset in [4, 5, 6] {
            assert_eq!(
                document.position_of(offset, PositionEncoding::Utf8),
                (0, 3),
                "emoji byte {offset}"
            );
            assert_eq!(
                document.position_of(offset, PositionEncoding::Utf16),
                (0, 1),
                "emoji byte {offset}"
            );
        }
    }

    #[test]
    fn positions_inside_encoded_scalars_clamp_to_the_following_boundary() {
        let document = document("\u{3042}\u{1f388}x\n");

        assert_eq!(document.offset_of(0, 1, PositionEncoding::Utf8), 3);
        for character in [4, 5, 6] {
            assert_eq!(
                document.offset_of(0, character, PositionEncoding::Utf8),
                7,
                "emoji byte unit {character}"
            );
        }
        assert_eq!(document.offset_of(0, 2, PositionEncoding::Utf16), 7);
    }

    /// Clients do send positions past the end of a line, and a stale one after
    /// a change they have not acknowledged. Neither is worth refusing over.
    #[test]
    fn an_out_of_range_position_clamps_instead_of_panicking() {
        let document = document("(f)\n");
        assert_eq!(document.offset_of(99, 0, PositionEncoding::Utf16), 4);
        assert_eq!(document.offset_of(0, 999, PositionEncoding::Utf16), 4);
    }

    #[test]
    fn a_crlf_document_puts_the_carriage_return_on_the_line_it_ends() {
        let document = document("(f)\r\n(g)\r\n");
        assert_eq!(document.position_of(5, PositionEncoding::Utf16), (1, 0));
    }

    /// Editors percent-encode a space in a path, and a server that does not
    /// decode it opens the wrong file — or, here, detects the wrong dialect.
    #[test]
    fn a_percent_encoded_path_is_decoded() {
        assert_eq!(
            path_from_uri("file:///tmp/my%20project/core.lisp"),
            Some(PathBuf::from("/tmp/my project/core.lisp"))
        );
    }

    /// A server sees URIs it cannot open. Inventing a path for one would make
    /// dialect detection read an extension that is not a filesystem extension.
    #[test]
    fn a_non_file_uri_has_no_path() {
        assert_eq!(path_from_uri("untitled:Untitled-1"), None);
        assert_eq!(path_from_uri("vscode-vfs://host/x.lisp"), None);
    }

    #[test]
    fn the_dialect_follows_the_uris_extension() {
        let mut documents = Documents::default();
        documents.open("file:///tmp/a.el".to_owned(), "(defun f ())".to_owned(), 1);
        assert_eq!(
            documents.get("file:///tmp/a.el").expect("open").dialect,
            Dialect::EmacsLisp
        );
    }

    #[test]
    fn opening_the_same_uri_replaces_its_document() {
        let mut documents = Documents::default();
        let uri = "file:///tmp/a.lisp";

        documents.open(uri.to_owned(), "(first)".to_owned(), 1);
        documents.open(uri.to_owned(), "(second)\n(next)".to_owned(), 7);

        let document = documents.get(uri).expect("reopened document");
        assert_eq!(document.text, "(second)\n(next)");
        assert_eq!(document.version, 7);
        assert_eq!(document.offset_of(1, 0, PositionEncoding::Utf16), 9);
        assert_eq!(documents.uris(), vec![uri.to_owned()]);
    }

    #[test]
    fn changes_require_an_open_document_and_a_newer_version() {
        let mut documents = Documents::default();
        let uri = "file:///tmp/a.lisp";

        documents.change(uri, "(unknown)".to_owned(), 1);
        assert!(documents.get(uri).is_none());

        documents.open(uri.to_owned(), "(initial)".to_owned(), 3);
        documents.change(uri, "(current)".to_owned(), 4);
        documents.change(uri, "(duplicate)".to_owned(), 4);
        documents.change(uri, "(stale)".to_owned(), 2);

        let document = documents.get(uri).expect("open document");
        assert_eq!(document.text, "(current)");
        assert_eq!(document.version, 4);
    }

    #[test]
    fn closing_a_document_removes_it_and_later_changes_do_not_restore_it() {
        let mut documents = Documents::default();
        let uri = "file:///tmp/a.lisp";

        documents.open(uri.to_owned(), "(open)".to_owned(), 1);
        documents.close(uri);
        documents.close(uri);
        documents.change(uri, "(changed)".to_owned(), 2);

        assert!(documents.get(uri).is_none());
        assert!(documents.uris().is_empty());
    }
}
