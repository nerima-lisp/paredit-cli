//! Line and column coordinates over a source file.
//!
//! Every selector but `--at` ultimately reports a byte offset, and every
//! *human* coordinate — the `12:5` an editor shows, the position an LSP client
//! sends — is a line and a column. This is the one conversion between them.
//!
//! Two decisions worth stating, because they are the ones that differ between
//! tools:
//!
//! - **Both are 1-based.** That is what editors display and what `file:12:5`
//!   means everywhere else in this CLI's output.
//! - **A column counts characters, not bytes and not UTF-16 code units.** A
//!   line holding `(λ x)` puts `x` at column 4, which is where a person
//!   counting glyphs would put it. An LSP server built on this has to convert
//!   to UTF-16 at its own boundary; doing it here would make `--line-column`
//!   disagree with every other line/column this tool prints.
//!
//! A line ends after its `\n`; a trailing `\r` belongs to the line, so a CRLF
//! file's last column is the `\r` rather than a phantom position.

use std::fmt;
use std::str::FromStr;

use super::error::{SelectorError, SelectorResult};

/// A 1-based line and column pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinePosition {
    line: usize,
    column: usize,
}

impl LinePosition {
    /// Creates a position, rejecting the 0 that a 0-based caller would pass.
    pub fn new(line: usize, column: usize) -> SelectorResult<Self> {
        if line == 0 || column == 0 {
            return Err(SelectorError::ZeroLineOrColumn {
                value: format!("{line}:{column}"),
            });
        }
        Ok(Self { line, column })
    }

    /// The 1-based line.
    #[must_use]
    pub const fn line(self) -> usize {
        self.line
    }

    /// The 1-based column, counted in characters.
    #[must_use]
    pub const fn column(self) -> usize {
        self.column
    }
}

impl fmt::Display for LinePosition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.line, self.column)
    }
}

/// Parses `LINE` or `LINE:COLUMN`, defaulting the column to 1.
///
/// `--line-column 12` selecting the first form on line 12 is the common case
/// — an agent reading a compiler warning has a line and no column — so the
/// column is optional rather than required.
impl FromStr for LinePosition {
    type Err = SelectorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let malformed = || SelectorError::MalformedPosition {
            value: value.to_owned(),
        };
        let trimmed = value.trim();
        let (line, column) = match trimmed.split_once(':') {
            Some((line, column)) => (line, column),
            None => (trimmed, "1"),
        };
        let line = line.trim().parse::<usize>().map_err(|_| malformed())?;
        let column = column.trim().parse::<usize>().map_err(|_| malformed())?;
        Self::new(line, column)
    }
}

/// Byte offsets of every line start, for constant-ish coordinate conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    /// Byte offset of the first character of each line, `line_starts[0] == 0`.
    line_starts: Vec<usize>,
    length: usize,
}

impl LineIndex {
    /// Builds the index for one source text.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        );
        Self {
            line_starts,
            length: source.len(),
        }
    }

    /// The number of lines, counting a trailing newline as ending the last
    /// line rather than opening an empty one.
    #[must_use]
    pub fn line_count(&self) -> usize {
        // `line_starts` gains an entry for the position *after* a trailing
        // newline. That position is where a next line would begin, not a line
        // that exists, so it is not counted unless something follows it.
        let trailing_empty = self
            .line_starts
            .len()
            .checked_sub(1)
            .is_some_and(|last| self.line_starts[last] == self.length && last > 0);
        self.line_starts.len() - usize::from(trailing_empty)
    }

    /// The 1-based line holding `offset`.
    #[must_use]
    pub fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index,
        }
    }

    /// The 1-based line and character column of `offset`.
    ///
    /// Total, because this is the display direction: a span end may sit on a
    /// line terminator or at end of input, and a report that renders it must
    /// not fail. Such an offset reports the end-of-line column of the last
    /// line it belongs to.
    #[must_use]
    pub fn position_of(&self, source: &str, offset: usize) -> LinePosition {
        let line = self.line_of(offset).clamp(1, self.line_count());
        let start = self.line_starts[line - 1];
        let end = offset.clamp(start, self.line_end(source, line));
        let column = source
            .get(start..end)
            .map_or(1, |text| text.chars().count() + 1);
        LinePosition { line, column }
    }

    /// The byte offset of a 1-based line and character column.
    ///
    /// A column one past the last character of a line is accepted — that is
    /// where a cursor sits at end of line — but anything beyond it is a
    /// coordinate the file does not have, and is refused rather than clamped:
    /// clamping would silently point at a form the caller never named.
    pub fn offset_of(&self, source: &str, position: LinePosition) -> SelectorResult<usize> {
        let line_count = self.line_count();
        if position.line > line_count {
            return Err(SelectorError::LineOutOfRange {
                line: position.line,
                line_count,
            });
        }

        let start = self.line_starts[position.line - 1];
        let text = source
            .get(start..self.line_end(source, position.line))
            .unwrap_or_default();
        let width = text.chars().count();
        if position.column > width + 1 {
            return Err(SelectorError::ColumnOutOfRange {
                line: position.line,
                column: position.column,
                width,
            });
        }

        let byte_offset = text
            .char_indices()
            .nth(position.column - 1)
            .map_or(text.len(), |(index, _)| index);
        Ok(start + byte_offset)
    }

    /// Byte offset just past the last *content* character of a 1-based line,
    /// with the `\n` and a CRLF's `\r` excluded so a CRLF file addresses the
    /// same columns an LF file does.
    fn line_end(&self, source: &str, line: usize) -> usize {
        let Some(next_start) = self.line_starts.get(line).copied() else {
            return source.len();
        };
        let newline = next_start.saturating_sub(1);
        if newline > 0 && source.as_bytes().get(newline - 1) == Some(&b'\r') {
            newline - 1
        } else {
            newline
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_line_defaults_to_column_one() {
        assert_eq!(
            "12".parse::<LinePosition>().unwrap(),
            LinePosition::new(12, 1).unwrap()
        );
    }

    #[test]
    fn line_and_column_parse_together() {
        let position = "12:5".parse::<LinePosition>().unwrap();
        assert_eq!((position.line(), position.column()), (12, 5));
    }

    #[test]
    fn zero_is_refused_because_coordinates_are_one_based() {
        assert!("0:1".parse::<LinePosition>().is_err());
        assert!("1:0".parse::<LinePosition>().is_err());
    }

    #[test]
    fn round_trips_offsets_through_coordinates() {
        let source = "(a b)\n(c d)";
        let index = LineIndex::new(source);
        for offset in 0..=source.len() {
            if !source.is_char_boundary(offset) {
                continue;
            }
            let position = index.position_of(source, offset);
            assert_eq!(
                index.offset_of(source, position).unwrap(),
                offset,
                "offset {offset} did not round-trip through {position}"
            );
        }
    }

    /// The one offset that cannot round-trip: past a trailing newline there is
    /// no line to be on, so it renders as the end of the last line instead.
    #[test]
    fn end_of_input_after_a_trailing_newline_renders_as_end_of_the_last_line() {
        let source = "(a b)\n";
        let index = LineIndex::new(source);
        assert_eq!(
            index.position_of(source, source.len()),
            LinePosition::new(1, 6).unwrap()
        );
    }

    #[test]
    fn a_crlf_line_addresses_the_same_columns_an_lf_line_does() {
        let source = "(a b)\r\n(c)\r\n";
        let index = LineIndex::new(source);
        assert_eq!(
            index
                .offset_of(source, LinePosition::new(2, 1).unwrap())
                .unwrap(),
            7
        );
        // Column 6 is end of line 1: the `\r` is not an addressable column.
        assert_eq!(
            index
                .offset_of(source, LinePosition::new(1, 6).unwrap())
                .unwrap(),
            5
        );
        assert!(
            index
                .offset_of(source, LinePosition::new(1, 7).unwrap())
                .is_err()
        );
    }

    #[test]
    fn a_column_counts_characters_not_bytes() {
        let source = "(λ x)";
        let index = LineIndex::new(source);
        // `λ` is two bytes, so byte offset 3 is the third *character*.
        assert_eq!(index.position_of(source, 3).column(), 3);
        assert_eq!(
            index
                .offset_of(source, LinePosition::new(1, 4).unwrap())
                .unwrap(),
            4
        );
    }

    #[test]
    fn a_trailing_newline_does_not_add_a_line() {
        assert_eq!(LineIndex::new("a\nb\n").line_count(), 2);
        assert_eq!(LineIndex::new("a\nb").line_count(), 2);
        assert_eq!(LineIndex::new("").line_count(), 1);
    }

    #[test]
    fn a_line_past_the_end_is_refused_rather_than_clamped() {
        let source = "(a)\n";
        let index = LineIndex::new(source);
        let error = index
            .offset_of(source, LinePosition::new(9, 1).unwrap())
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "line 9 is past the end of the input, which has 1 lines"
        );
    }

    #[test]
    fn end_of_line_is_addressable_but_beyond_it_is_not() {
        let source = "(a)\n(b)\n";
        let index = LineIndex::new(source);
        assert_eq!(
            index
                .offset_of(source, LinePosition::new(1, 4).unwrap())
                .unwrap(),
            3
        );
        assert!(
            index
                .offset_of(source, LinePosition::new(1, 6).unwrap())
                .is_err()
        );
    }
}
