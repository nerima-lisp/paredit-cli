//! Editor invariants that do not depend on a terminal or filesystem.

/// A cursor position measured in Unicode scalar values, not bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    line: usize,
    column: usize,
}

impl Cursor {
    /// Creates a cursor at a line and character column.
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// Returns the zero-based line.
    #[must_use]
    pub const fn line(self) -> usize {
        self.line
    }

    /// Returns the zero-based Unicode scalar-value column.
    #[must_use]
    pub const fn column(self) -> usize {
        self.column
    }
}

/// A non-empty sequence of UTF-8 lines with cursor-safe editing operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBuffer {
    lines: Vec<String>,
}

impl TextBuffer {
    /// Builds a buffer while preserving trailing newlines.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(ToOwned::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self { lines }
    }

    /// Serializes the buffer back to UTF-8 text.
    #[must_use]
    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Returns all lines in source order.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Returns one line, if it exists.
    #[must_use]
    pub fn line(&self, line: usize) -> Option<&str> {
        self.lines.get(line).map(String::as_str)
    }

    /// Returns the number of lines, always at least one.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Inserts one character at the cursor and advances the cursor.
    pub fn insert_char(&mut self, cursor: &mut Cursor, character: char) -> bool {
        if character == '\n' {
            return self.newline(cursor);
        }

        self.normalize_cursor(cursor);
        let byte_index = self.byte_index(*cursor);
        self.lines[cursor.line].insert(byte_index, character);
        cursor.column += 1;
        true
    }

    /// Inserts a string one Unicode scalar value at a time.
    pub fn insert_str(&mut self, cursor: &mut Cursor, text: &str) -> bool {
        let mut changed = false;
        for character in text.chars() {
            changed |= self.insert_char(cursor, character);
        }
        changed
    }

    /// Splits the current line at the cursor and moves to the new line.
    pub fn newline(&mut self, cursor: &mut Cursor) -> bool {
        self.normalize_cursor(cursor);
        let byte_index = self.byte_index(*cursor);
        let suffix = self.lines[cursor.line].split_off(byte_index);
        self.lines.insert(cursor.line + 1, suffix);
        cursor.line += 1;
        cursor.column = 0;
        true
    }

    /// Deletes the character before the cursor, joining lines when needed.
    pub fn backspace(&mut self, cursor: &mut Cursor) -> bool {
        self.normalize_cursor(cursor);
        if cursor.column > 0 {
            let byte_index = self.byte_index(Cursor::new(cursor.line, cursor.column - 1));
            self.lines[cursor.line].remove(byte_index);
            cursor.column -= 1;
            return true;
        }

        if cursor.line == 0 {
            return false;
        }

        let current_line = self.lines.remove(cursor.line);
        cursor.line -= 1;
        cursor.column = self.lines[cursor.line].chars().count();
        self.lines[cursor.line].push_str(&current_line);
        true
    }

    /// Deletes the character under the cursor, joining lines when needed.
    pub fn delete(&mut self, cursor: &mut Cursor) -> bool {
        self.normalize_cursor(cursor);
        let line_length = self.lines[cursor.line].chars().count();
        if cursor.column < line_length {
            let byte_index = self.byte_index(*cursor);
            self.lines[cursor.line].remove(byte_index);
            return true;
        }

        if cursor.line + 1 == self.lines.len() {
            return false;
        }

        let next_line = self.lines.remove(cursor.line + 1);
        self.lines[cursor.line].push_str(&next_line);
        true
    }

    /// Moves one character left, crossing the previous line when needed.
    pub fn move_left(&self, cursor: &mut Cursor) {
        self.normalize_cursor(cursor);
        if cursor.column > 0 {
            cursor.column -= 1;
        } else if cursor.line > 0 {
            cursor.line -= 1;
            cursor.column = self.lines[cursor.line].chars().count();
        }
    }

    /// Moves one character right, crossing the next line when needed.
    pub fn move_right(&self, cursor: &mut Cursor) {
        self.normalize_cursor(cursor);
        let line_length = self.lines[cursor.line].chars().count();
        if cursor.column < line_length {
            cursor.column += 1;
        } else if cursor.line + 1 < self.lines.len() {
            cursor.line += 1;
            cursor.column = 0;
        }
    }

    /// Moves up while keeping the requested horizontal column when possible.
    pub fn move_up(&self, cursor: &mut Cursor) {
        self.normalize_cursor(cursor);
        if cursor.line > 0 {
            cursor.line -= 1;
            cursor.column = cursor.column.min(self.lines[cursor.line].chars().count());
        }
    }

    /// Moves down while keeping the requested horizontal column when possible.
    pub fn move_down(&self, cursor: &mut Cursor) {
        self.normalize_cursor(cursor);
        if cursor.line + 1 < self.lines.len() {
            cursor.line += 1;
            cursor.column = cursor.column.min(self.lines[cursor.line].chars().count());
        }
    }

    /// Moves to the beginning of the current line.
    pub fn move_home(&self, cursor: &mut Cursor) {
        self.normalize_cursor(cursor);
        cursor.column = 0;
    }

    /// Moves to the end of the current line.
    pub fn move_end(&self, cursor: &mut Cursor) {
        self.normalize_cursor(cursor);
        cursor.column = self.lines[cursor.line].chars().count();
    }

    fn normalize_cursor(&self, cursor: &mut Cursor) {
        cursor.line = cursor.line.min(self.lines.len().saturating_sub(1));
        cursor.column = cursor.column.min(self.lines[cursor.line].chars().count());
    }

    fn byte_index(&self, cursor: Cursor) -> usize {
        self.lines[cursor.line]
            .char_indices()
            .nth(cursor.column)
            .map_or_else(|| self.lines[cursor.line].len(), |(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_trailing_newline() {
        let buffer = TextBuffer::from_text("(a)\n");

        assert_eq!(buffer.lines(), &["(a)".to_owned(), String::new()]);
        assert_eq!(buffer.to_text(), "(a)\n");
    }

    #[test]
    fn edits_unicode_by_character_column() {
        let mut buffer = TextBuffer::from_text("λx");
        let mut cursor = Cursor::new(0, 1);

        buffer.insert_char(&mut cursor, '🙂');
        buffer.backspace(&mut cursor);

        assert_eq!(buffer.to_text(), "λx");
        assert_eq!(cursor, Cursor::new(0, 1));
    }

    #[test]
    fn joins_and_splits_lines_at_cursor() {
        let mut buffer = TextBuffer::from_text("ab\ncd");
        let mut cursor = Cursor::new(1, 1);

        buffer.newline(&mut cursor);
        assert_eq!(buffer.to_text(), "ab\nc\nd");
        buffer.backspace(&mut cursor);
        assert_eq!(buffer.to_text(), "ab\ncd");
        assert_eq!(cursor, Cursor::new(1, 1));
    }

    #[test]
    fn movement_crosses_lines_and_clamps_columns() {
        let buffer = TextBuffer::from_text("long\nx");
        let mut cursor = Cursor::new(0, 4);

        buffer.move_right(&mut cursor);
        assert_eq!(cursor, Cursor::new(1, 0));
        buffer.move_left(&mut cursor);
        assert_eq!(cursor, Cursor::new(0, 4));
        buffer.move_down(&mut cursor);
        assert_eq!(cursor, Cursor::new(1, 1));
    }
}
